//! 自定义渠道模板
//!
//! 此文件提供了一个模板，方便用户创建自定义渠道适配器

use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::RwLock;
use stormclaw_core::{MessageBus, OutboundMessage};
use super::{BaseChannel, ChannelState};

/// 自定义渠道模板
///
/// 复制此文件并修改以创建你自己的渠道
pub struct CustomChannelTemplate {
    name: String,
    config: CustomConfig,
    bus: Arc<MessageBus>,
    state: Arc<RwLock<ChannelState>>,
    running: Arc<AtomicBool>,
}

/// 自定义渠道配置
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CustomConfig {
    pub enabled: bool,
    pub api_endpoint: String,
    pub api_key: Option<String>,
    pub allow_from: Vec<String>,
}

impl CustomChannelTemplate {
    /// 创建新的自定义渠道
    pub fn new(
        name: String,
        config: CustomConfig,
        bus: Arc<MessageBus>,
    ) -> Self {
        Self {
            name,
            config,
            bus,
            state: Arc::new(RwLock::new(ChannelState::default())),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 连接到平台 API：对 `api_endpoint` 做 HTTP GET 探测（2xx 视为可达）
    async fn connect(&self) -> anyhow::Result<()> {
        tracing::info!(
            "Connecting to {} platform at {}",
            self.name,
            self.config.api_endpoint
        );

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()?;
        let resp = client.get(&self.config.api_endpoint).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!(
                "api_endpoint returned HTTP {} for {}",
                resp.status(),
                self.config.api_endpoint
            );
        }
        Ok(())
    }

    /// 模板占位：真实渠道应在此实现 WebSocket / Webhook / 长轮询。此处仅 sleep 以保持任务存活。
    async fn listen_for_messages(&self) -> anyhow::Result<()> {
        while self.running.load(Ordering::Relaxed) {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
        Ok(())
    }
}

#[async_trait]
impl BaseChannel for CustomChannelTemplate {
    fn name(&self) -> &str {
        &self.name
    }

    async fn start(&self) -> anyhow::Result<()> {
        if self.running.load(Ordering::Relaxed) {
            return Ok(());
        }

        tracing::info!("Starting {} channel", self.name);

        self.running.store(true, Ordering::Relaxed);

        // 连接到平台
        self.connect().await?;

        // 更新状态
        {
            let mut state = self.state.write().await;
            state.is_running = true;
            state.connected_at = Some(chrono::Utc::now());
        }

        // 开始监听消息
        self.listen_for_messages().await?;

        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        tracing::info!("Stopping {} channel", self.name);
        self.running.store(false, Ordering::Relaxed);

        let mut state = self.state.write().await;
        state.is_running = false;

        Ok(())
    }

    async fn send(&self, msg: &OutboundMessage) -> anyhow::Result<()> {
        tracing::debug!(
            "Sending {} message to {}: {}",
            self.name,
            msg.chat_id,
            msg.content
        );

        let client = reqwest::Client::new();
        let response = client
            .post(&self.config.api_endpoint)
            .header("Authorization",
                self.config.api_key.as_ref()
                    .map(|k| format!("Bearer {}", k))
                    .unwrap_or_default())
            .json(&serde_json::json!({
                "chat_id": msg.chat_id,
                "text": msg.content,
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to send message: {}", response.status());
        }

        {
            let mut state = self.state.write().await;
            state.messages_sent += 1;
        }

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

/// 创建自定义渠道的辅助函数
///
/// 使用示例：
/// ```ignore
/// let channel = create_custom_channel(
///     "myplatform",
///     CustomConfig {
///         enabled: true,
///         api_endpoint: "https://api.example.com".to_string(),
///         api_key: Some("secret".to_string()),
///         allow_from: vec![],
///     },
///     bus,
/// );
/// ```
pub fn create_custom_channel(
    name: String,
    config: CustomConfig,
    bus: Arc<MessageBus>,
) -> Arc<dyn BaseChannel> {
    Arc::new(CustomChannelTemplate::new(name, config, bus))
}
