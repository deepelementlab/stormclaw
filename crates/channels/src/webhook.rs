//! Webhook 渠道
//!
//! 通过 HTTP Webhook 接收消息的通用渠道

use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::RwLock;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
    body::Body,
};
use serde::Deserialize;
use stormclaw_core::{MessageBus, OutboundMessage};
use super::{BaseChannel, ChannelState};

/// Webhook 请求格式
#[derive(Debug, Deserialize)]
struct WebhookRequest {
    #[serde(default)]
    pub channel: String,
    pub sender_id: String,
    pub chat_id: String,
    pub content: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Webhook 渠道
///
/// 启动 HTTP 服务器接收 webhook 请求
pub struct WebhookChannel {
    bind_address: String,
    port: u16,
    api_key: Option<String>,
    bus: Arc<MessageBus>,
    state: Arc<RwLock<ChannelState>>,
    running: Arc<AtomicBool>,
}

impl WebhookChannel {
    pub fn new(
        bind_address: String,
        port: u16,
        api_key: Option<String>,
        bus: Arc<MessageBus>,
    ) -> Self {
        Self {
            bind_address,
            port,
            api_key,
            bus,
            state: Arc::new(RwLock::new(ChannelState::default())),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 构建路由
    fn build_router(&self) -> Router {
        let bus = self.bus.clone();
        let api_key = self.api_key.clone();

        Router::new()
            .route("/webhook", post(webhook_handler))
            .route("/webhook/:key", post(webhook_handler_with_key))
            .with_state((bus, api_key))
    }

    /// 验证 API Key
    fn verify_key(&self, key: Option<&str>) -> bool {
        match (&self.api_key, key) {
            (None, _) => true,
            (Some(config_key), Some(request_key)) => config_key == request_key,
            (Some(_), None) => false,
        }
    }
}

/// Webhook 处理器（无密钥）
async fn webhook_handler(
    State((bus, _api_key)): State<(Arc<MessageBus>, Option<String>)>,
    Json(req): Json<WebhookRequest>,
) -> Response {
    webhook_handler_inner(bus, req).await
}

/// Webhook 处理器（带密钥）
async fn webhook_handler_with_key(
    State((bus, api_key)): State<(Arc<MessageBus>, Option<String>)>,
    axum::extract::Path(key): axum::extract::Path<String>,
    Json(req): Json<WebhookRequest>,
) -> Response {
    // 验证密钥
    if let Some(config_key) = &api_key {
        if config_key != &key {
            return (StatusCode::UNAUTHORIZED, "Invalid API key").into_response();
        }
    }

    webhook_handler_inner(bus, req).await
}

/// 内部 webhook 处理逻辑
async fn webhook_handler_inner(
    bus: Arc<MessageBus>,
    req: WebhookRequest,
) -> Response {
    let inbound = stormclaw_core::InboundMessage {
        channel: if req.channel.is_empty() { "webhook".to_string() } else { req.channel },
        sender_id: req.sender_id,
        chat_id: req.chat_id,
        content: req.content,
        timestamp: chrono::Utc::now(),
        media: vec![],
        metadata: req.metadata,
    };

    match bus.publish_inbound(inbound).await {
        Ok(()) => (StatusCode::OK, "Message received").into_response(),
        Err(e) => {
            tracing::error!("Failed to publish inbound message: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal error").into_response()
        }
    }
}

#[async_trait]
impl BaseChannel for WebhookChannel {
    fn name(&self) -> &str {
        "webhook"
    }

    async fn start(&self) -> anyhow::Result<()> {
        if self.running.load(Ordering::Relaxed) {
            return Ok(());
        }

        tracing::info!("Starting webhook channel on {}:{}", self.bind_address, self.port);

        self.running.store(true, Ordering::Relaxed);
        {
            let mut state = self.state.write().await;
            state.is_running = true;
            state.connected_at = Some(chrono::Utc::now());
        }

        let app = self.build_router();
        let addr = format!("{}:{}", self.bind_address, self.port);

        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        tracing::info!("Stopping webhook channel");
        self.running.store(false, Ordering::Relaxed);

        let mut state = self.state.write().await;
        state.is_running = false;

        Ok(())
    }

    async fn send(&self, msg: &OutboundMessage) -> anyhow::Result<()> {
        // Webhook 渠道通常只接收消息，不发送
        // 如果需要发送，可以配置回调 URL
        tracing::warn!("Webhook channel doesn't support sending messages (chat_id: {})", msg.chat_id);
        Ok(())
    }

    fn bus(&self) -> &MessageBus {
        &self.bus
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}
