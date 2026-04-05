//! Slack 渠道实现
//!
//! 通过 Slack Bot API 和 Socket Mode 集成

use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::RwLock;
use futures_util::StreamExt;
use stormclaw_core::{MessageBus, OutboundMessage, InboundMessage};
use super::{BaseChannel, ChannelState};
use chrono::Utc;

/// Slack 渠道配置
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct SlackConfig {
    pub enabled: bool,
    pub bot_token: String,
    pub app_token: String,
    pub allow_from: Vec<String>, // Team IDs or Channel IDs
    pub mode: SlackMode,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SlackMode {
    Http,
    Socket,
}

impl Default for SlackConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bot_token: String::new(),
            app_token: String::new(),
            allow_from: Vec::new(),
            mode: SlackMode::Http,
        }
    }
}

/// Slack 渠道
pub struct SlackChannel {
    config: SlackConfig,
    bus: Arc<MessageBus>,
    state: Arc<RwLock<ChannelState>>,
    running: Arc<AtomicBool>,
    client: Arc<reqwest::Client>,
    message_count: Arc<AtomicU64>,
}

impl SlackChannel {
    pub fn new(
        config: SlackConfig,
        bus: Arc<MessageBus>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            config,
            bus,
            state: Arc::new(RwLock::new(ChannelState::default())),
            running: Arc::new(AtomicBool::new(false)),
            client: Arc::new(
                reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(30))
                    .build()?
            ),
            message_count: Arc::new(AtomicU64::new(0)),
        })
    }

    /// 发送 HTTP 请求到 Slack API
    async fn slack_request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
        use_app_token: bool,
    ) -> anyhow::Result<serde_json::Value> {
        let url = format!("https://slack.com/api{}", path);

        let token = if use_app_token {
            &self.config.app_token
        } else {
            &self.config.bot_token
        };

        let mut request = self.client
            .request(method, &url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json; charset=utf-8");

        if let Some(body) = body {
            request = request.json(&body);
        }

        let response = request.send().await?;
        let json: serde_json::Value = response.json().await?;

        // 检查 Slack API 响应
        if let Some(false) = json.get("ok").and_then(|v| v.as_bool()) {
            let error = json.get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            anyhow::bail!("Slack API error: {}", error);
        }

        Ok(json)
    }

    /// 测试认证
    pub async fn auth_test(&self) -> anyhow::Result<SlackAuthTestResponse> {
        let response = self.slack_request(
            reqwest::Method::POST,
            "/auth.test",
            None,
            false,
        ).await?;

        Ok(serde_json::from_value(response)?)
    }

    /// 启动 WebSocket Mode 连接
    async fn start_socket_mode(&self) -> anyhow::Result<()> {
        // 首先获取 WebSocket URL
        let response = self.slack_request(
            reqwest::Method::POST,
            "/apps.connections.open",
            None,
            true,
        ).await?;

        let url = response.get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("No WebSocket URL in response"))?;

        tracing::info!("Connecting to Slack WebSocket: {}", url);

        // 使用 tokio-tungstenite 连接
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;

        let mut request = url.into_client_request()?;
        request.headers_mut().insert(
            "User-Agent",
            "stormclaw/1.0".parse().unwrap()
        );

        let (ws_stream, _) = tokio_tungstenite::connect_async(request).await?;
        tracing::info!("Connected to Slack WebSocket Mode");

        // 处理 WebSocket 消息
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        let running = self.running.clone();
        let bus = self.bus.clone();
        let config = self.config.clone();
        let message_count = self.message_count.clone();

        // 启动消息处理任务
        tokio::spawn(async move {
            while running.load(Ordering::Relaxed) {
                match ws_receiver.next().await {
                    Some(Ok(message)) => {
                        if let Err(e) = handle_slack_message(message, &bus, &config, &message_count).await {
                            tracing::error!("Error handling Slack message: {}", e);
                        }
                    }
                    Some(Err(e)) => {
                        tracing::error!("WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        tracing::info!("WebSocket connection closed");
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// 获取团队信息
    pub async fn get_team_info(&self) -> anyhow::Result<SlackTeam> {
        let response = self.slack_request(
            reqwest::Method::GET,
            "/auth.test",
            None,
            false,
        ).await?;

        Ok(SlackTeam {
            id: response.get("team_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            name: response.get("team").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        })
    }
}

/// 处理 Slack WebSocket 消息
async fn handle_slack_message(
    message: tokio_tungstenite::tungstenite::Message,
    bus: &Arc<MessageBus>,
    config: &SlackConfig,
    message_count: &Arc<AtomicU64>,
) -> anyhow::Result<()> {
    use tokio_tungstenite::tungstenite::Message;

    match message {
        Message::Text(text) => {
            // 解析 Slack 消息
            if let Ok(envelope) = serde_json::from_str::<SlackMessageEnvelope>(&text) {
                match envelope.type_.as_str() {
                    "event_callback" => {
                        if let Some(event) = envelope.payload {
                            if let Ok(event) = serde_json::from_value::<SlackEvent>(event) {
                                match event.type_.as_str() {
                                    "message" => {
                                        // 处理消息
                                        if let Some(text) = event.text {
                                            let sender_id = event.user.unwrap_or_else(|| event.bot_id.unwrap_or_default());
                                            let channel_id = event.channel;

                                            // 检查权限
                                            if !config.allow_from.is_empty()
                                                && !config.allow_from.contains(&sender_id)
                                                && !config.allow_from.contains(&channel_id) {
                                                return Ok(());
                                            }

                                            let inbound = InboundMessage {
                                                channel: "slack".to_string(),
                                                sender_id,
                                                chat_id: channel_id,
                                                content: text,
                                                timestamp: Utc::now(),
                                                media: Vec::new(),
                                                metadata: serde_json::json!({}),
                                            };

                                            bus.publish_inbound(inbound).await?;
                                            message_count.fetch_add(1, Ordering::Relaxed);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    "hello" => {
                        tracing::info!("Slack WebSocket connection established");
                    }
                    "error" => {
                        tracing::error!("Slack WebSocket error: {:?}", envelope.payload);
                    }
                    _ => {
                        tracing::debug!("Unhandled Slack message type: {}", envelope.type_);
                    }
                }
            }
        }
        Message::Ping(_) => {
            // 响应 ping
        }
        _ => {}
    }

    Ok(())
}

#[async_trait]
impl BaseChannel for SlackChannel {
    fn name(&self) -> &str {
        "slack"
    }

    async fn start(&self) -> anyhow::Result<()> {
        if self.running.load(Ordering::Relaxed) {
            return Ok(());
        }

        tracing::info!("Starting Slack channel (mode: {:?})", self.config.mode);

        // 验证 token
        match self.auth_test().await {
            Ok(auth) => {
                tracing::info!("Connected to Slack as team {} ({})", auth.team, auth.team_id);
            }
            Err(e) => {
                tracing::error!("Failed to authenticate with Slack: {}", e);
                return Err(e);
            }
        }

        self.running.store(true, Ordering::Relaxed);
        {
            let mut state = self.state.write().await;
            state.is_running = true;
            state.connected_at = Some(Utc::now());
        }

        // 根据模式启动
        match self.config.mode {
            SlackMode::Http => {
                tracing::info!("Slack channel started in HTTP mode");
            }
            SlackMode::Socket => {
                self.start_socket_mode().await?;
                tracing::info!("Slack channel started in Socket Mode");
            }
        }

        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        tracing::info!("Stopping Slack channel");
        self.running.store(false, Ordering::Relaxed);

        let mut state = self.state.write().await;
        state.is_running = false;

        Ok(())
    }

    async fn send(&self, msg: &OutboundMessage) -> anyhow::Result<()> {
        tracing::debug!("Sending Slack message to {}: {}", msg.chat_id, msg.content);

        let slack_msg = serde_json::json!({
            "channel": msg.chat_id,
            "text": msg.content,
        });

        self.slack_request(
            reqwest::Method::POST,
            "/chat.postMessage",
            Some(slack_msg),
            false,
        ).await?;

        // 更新统计
        let mut state = self.state.write().await;
        state.messages_sent += 1;

        Ok(())
    }

    fn is_allowed(&self, sender_id: &str) -> bool {
        if self.config.allow_from.is_empty() {
            return true;
        }
        self.config.allow_from.iter().any(|a| a == sender_id)
    }

    fn bus(&self) -> &MessageBus {
        &self.bus
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

// Slack API 类型定义

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SlackAuthTestResponse {
    pub ok: bool,
    pub url: String,
    pub team: String,
    pub user: String,
    pub team_id: String,
    pub user_id: String,
}

#[derive(Debug, Clone)]
pub struct SlackTeam {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SlackMessageEnvelope {
    #[serde(rename = "type")]
    pub type_: String,
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SlackEvent {
    #[serde(rename = "type")]
    pub type_: String,
    pub text: Option<String>,
    pub user: Option<String>,
    pub bot_id: Option<String>,
    pub channel: String,
    pub ts: Option<String>,
    pub thread_ts: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slack_config_default() {
        let config = SlackConfig::default();
        assert!(!config.enabled);
        assert!(config.bot_token.is_empty());
        assert!(matches!(config.mode, SlackMode::Http));
    }

    #[test]
    fn test_allow_from() {
        let config = SlackConfig {
            allow_from: vec!["U123".to_string(), "C456".to_string()],
            ..Default::default()
        };

        let bus = Arc::new(MessageBus::new(10));
        let channel = SlackChannel::new(config, bus).unwrap();

        assert!(channel.is_allowed("U123"));
        assert!(channel.is_allowed("C456"));
        assert!(!channel.is_allowed("U789"));
    }
}
