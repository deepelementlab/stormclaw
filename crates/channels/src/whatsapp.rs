//! WhatsApp 渠道实现
//!
//! 通过 WebSocket 连接到 Node.js 网桥

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::RwLock;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};
use futures_util::{StreamExt, SinkExt};
use serde::{Deserialize, Serialize};
use stormclaw_core::{MessageBus, OutboundMessage};
use super::{BaseChannel, ChannelState};

/// WhatsApp 渠道
///
/// 通过 WebSocket 连接到独立的 Node.js 网桥
pub struct WhatsAppChannel {
    bridge_url: String,
    allow_from: Vec<String>,
    bus: Arc<MessageBus>,
    state: Arc<RwLock<ChannelState>>,
    running: Arc<AtomicBool>,
    pending_messages: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

/// 网桥消息类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum BridgeMessage {
    #[serde(rename = "message")]
    Message {
        id: String,
        sender: String,
        content: String,
        timestamp: i64,
        #[serde(rename = "isGroup")]
        is_group: bool,
    },
    #[serde(rename = "status")]
    Status {
        status: String,
    },
    #[serde(rename = "qr")]
    Qr {
        qr: String,
    },
    #[serde(rename = "error")]
    Error {
        error: String,
    },
    #[serde(rename = "sent")]
    Sent {
        to: String,
    },
}

/// 发送到网桥的消息
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BridgeSendRequest {
    r#type: String,
    to: String,
    text: String,
}

impl WhatsAppChannel {
    /// 创建新的 WhatsApp 渠道
    pub fn new(
        bridge_url: String,
        allow_from: Vec<String>,
        bus: Arc<MessageBus>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            bridge_url,
            allow_from,
            bus,
            state: Arc::new(RwLock::new(ChannelState::default())),
            running: Arc::new(AtomicBool::new(false)),
            pending_messages: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// 连接到网桥
    async fn connect(&self) -> anyhow::Result<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>> {
        tracing::info!("Connecting to WhatsApp bridge at {}", self.bridge_url);

        let url = if self.bridge_url.starts_with("ws://") || self.bridge_url.starts_with("wss://") {
            self.bridge_url.clone()
        } else {
            format!("ws://{}", self.bridge_url)
        };

        let (ws_stream, _) = connect_async(&url).await?;
        Ok(ws_stream)
    }

    /// 处理网桥消息
    async fn handle_bridge_message(&self, msg: BridgeMessage) -> anyhow::Result<()> {
        match msg {
            BridgeMessage::Message { id, sender, content, timestamp, is_group } => {
                tracing::debug!("WhatsApp message from {}: {}", sender, content);

                // 更新统计
                {
                    let mut state = self.state.write().await;
                    state.messages_received += 1;
                }

                // 对齐 Python：
                // - sender_id 用手机号（用于 allowlist）
                // - chat_id 用完整 JID（用于回复路由，session_key 也以此为准）
                let sender_id = self.clean_phone_number(&sender);
                let chat_id = sender.clone();

                // 检查权限
                if !self.is_allowed(&sender_id) {
                    tracing::debug!("Rejected message from unauthorized sender: {}", sender_id);
                    return Ok(());
                }

                // 转发到消息总线
                let inbound = stormclaw_core::InboundMessage {
                    channel: self.name().to_string(),
                    sender_id,
                    chat_id,
                    content,
                    timestamp: chrono::DateTime::from_timestamp(timestamp, 0).unwrap_or_else(chrono::Utc::now),
                    media: vec![],
                    metadata: serde_json::json!({
                        "message_id": id,
                        "is_group": is_group,
                    }),
                };

                self.bus.publish_inbound(inbound).await?;
            }
            BridgeMessage::Status { status } => {
                tracing::info!("WhatsApp status: {}", status);
                if status == "connected" {
                    let mut state = self.state.write().await;
                    state.connected_at = Some(chrono::Utc::now());
                }
            }
            BridgeMessage::Qr { .. } => {
                // QR 展示由 bridge 负责打印；这里仅记录
                tracing::info!("WhatsApp bridge provided QR code");
            }
            BridgeMessage::Error { error } => {
                tracing::error!("WhatsApp bridge error: {}", error);
            }
            BridgeMessage::Sent { to } => {
                tracing::debug!("WhatsApp message sent ack to={}", to);
            }
        }

        Ok(())
    }

    /// 清理手机号码格式
    ///
    /// 移除 + 前缀和空格
    fn clean_phone_number(&self, phone: &str) -> String {
        phone.chars()
            .filter(|c| c.is_ascii_digit())
            .collect()
    }

    /// 运行 WebSocket 消息循环
    async fn run_message_loop(&self) -> anyhow::Result<()> {
        let ws_stream = self.connect().await?;

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        let pending_messages = self.pending_messages.clone();
        let running = self.running.clone();
        let bus = self.bus.clone();
        let state = self.state.clone();
        let allow_from = self.allow_from.clone();

        // 启动发送任务
        let send_running = running.clone();
        let send_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(100));

            while send_running.load(Ordering::Relaxed) {
                interval.tick().await;

                let mut pending = pending_messages.write().await;
                let mut messages_to_send = HashMap::new();

                for (chat_id, msgs) in pending.iter() {
                    if !msgs.is_empty() {
                        messages_to_send.insert(chat_id.clone(), msgs.clone());
                    }
                }

                for (chat_id, messages) in messages_to_send {
                    for msg in messages {
                        let request = BridgeSendRequest {
                            r#type: "send".to_string(),
                            to: chat_id.clone(),
                            text: msg,
                        };

                        match serde_json::to_string(&request) {
                            Ok(json) => {
                                if let Err(e) = ws_sender.send(WsMessage::Text(json)).await {
                                    tracing::error!("Failed to send message to bridge: {}", e);
                                } else {
                                    // 清空已发送的消息
                                    if let Some(msgs) = pending.get_mut(&chat_id) {
                                        msgs.remove(0);
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!("Failed to serialize message: {}", e);
                            }
                        }
                    }
                }
            }
        });

        // 接收消息
        while running.load(Ordering::Relaxed) {
            match ws_receiver.next().await {
                Some(Ok(msg)) => {
                    match msg {
                        WsMessage::Text(text) => {
                            if let Ok(bridge_msg) = serde_json::from_str::<BridgeMessage>(&text) {
                                // 处理消息（统一走 handle_bridge_message 逻辑，避免两处不一致）
                                if let Err(e) = self.handle_bridge_message(bridge_msg).await {
                                    tracing::error!("Failed to handle bridge message: {}", e);
                                }
                            } else {
                                tracing::warn!("Invalid bridge message: {}", text);
                            }
                        }
                        WsMessage::Close(_) => {
                            tracing::info!("WebSocket connection closed by bridge");
                            break;
                        }
                        WsMessage::Ping(_) => {
                            // Ping/Pong 由 tungstenite 自动处理
                        }
                        _ => {}
                    }
                }
                Some(Err(e)) => {
                    tracing::error!("WebSocket error: {}", e);
                    break;
                }
                None => {
                    tracing::info!("WebSocket stream ended");
                    break;
                }
            }
        }

        send_task.abort();

        Ok(())
    }
}

#[async_trait]
impl BaseChannel for WhatsAppChannel {
    fn name(&self) -> &str {
        "whatsapp"
    }

    async fn start(&self) -> anyhow::Result<()> {
        if self.running.load(Ordering::Relaxed) {
            tracing::warn!("WhatsApp channel is already running");
            return Ok(());
        }

        tracing::info!("Starting WhatsApp channel");

        self.running.store(true, Ordering::Relaxed);

        {
            let mut state = self.state.write().await;
            state.is_running = true;
        }

        // 运行消息循环（对齐 Python：断线重连）
        while self.running.load(Ordering::Relaxed) {
            if let Err(e) = self.run_message_loop().await {
                tracing::error!("WhatsApp message loop error: {}", e);
                if self.running.load(Ordering::Relaxed) {
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
        }

        {
            let mut state = self.state.write().await;
            state.is_running = false;
        }
        self.running.store(false, Ordering::Relaxed);

        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        tracing::info!("Stopping WhatsApp channel");
        self.running.store(false, Ordering::Relaxed);

        let mut state = self.state.write().await;
        state.is_running = false;

        Ok(())
    }

    async fn send(&self, msg: &OutboundMessage) -> anyhow::Result<()> {
        tracing::debug!("Queueing WhatsApp message to {}: {}", msg.chat_id, msg.content);

        // 将消息添加到待发送队列
        let mut pending = self.pending_messages.write().await;
        pending.entry(msg.chat_id.clone())
            .or_insert_with(Vec::new)
            .push(msg.content.clone());

        // 更新统计
        {
            let mut state = self.state.write().await;
            state.messages_sent += 1;
        }

        Ok(())
    }

    fn is_allowed(&self, sender_id: &str) -> bool {
        if self.allow_from.is_empty() {
            return true;
        }

        // 清理并比较
        let clean_sender = self.clean_phone_number(sender_id);
        self.allow_from.iter()
            .any(|allowed| self.clean_phone_number(allowed) == clean_sender)
    }

    fn bus(&self) -> &MessageBus {
        &self.bus
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

/// WhatsApp 网桥健康检查
pub async fn check_bridge_health(bridge_url: &str) -> anyhow::Result<bool> {
    let url = if bridge_url.starts_with("http://") || bridge_url.starts_with("https://") {
        bridge_url.to_string()
    } else {
        format!("http://{}", bridge_url)
    };

    match reqwest::get(&url).await {
        Ok(resp) => Ok(resp.status().is_success()),
        Err(_) => Ok(false),
    }
}
