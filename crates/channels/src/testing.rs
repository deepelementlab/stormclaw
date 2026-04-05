//! 渠道测试工具
//!
//! 提供渠道测试和调试功能

use std::sync::Arc;
use std::time::Duration;
use stormclaw_core::MessageBus;

/// 渠道测试器
pub struct ChannelTester {
    bus: Arc<MessageBus>,
}

impl ChannelTester {
    pub fn new(bus: Arc<MessageBus>) -> Self {
        Self { bus }
    }

    /// 发送测试消息到指定渠道
    pub async fn send_test_message(
        &self,
        channel: &str,
        chat_id: &str,
        content: &str,
    ) -> anyhow::Result<()> {
        let inbound = stormclaw_core::InboundMessage::new(
            channel,
            "test_user",
            chat_id,
            content,
        );

        self.bus.publish_inbound(inbound).await?;
        Ok(())
    }

    /// 等待指定渠道的出站消息（依赖 [MessageBus::subscribe_outbound]）
    pub async fn wait_for_outbound(
        &self,
        channel: &str,
        timeout: Duration,
    ) -> Option<stormclaw_core::OutboundMessage> {
        let mut rx = self
            .bus
            .subscribe_outbound(channel.to_string())
            .await;

        tokio::time::timeout(timeout, async move {
            loop {
                match rx.recv().await {
                    Ok(msg) => return Some(msg),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                }
            }
        })
        .await
        .ok()
        .flatten()
    }

    /// 运行连接测试
    pub async fn test_connection(&self, channel: &str) -> anyhow::Result<TestResult> {
        let start = std::time::Instant::now();

        // 发送测试消息
        self.send_test_message(channel, "test", "ping").await?;

        // 等待响应
        let response = tokio::time::timeout(
            Duration::from_secs(30),
            self.wait_for_outbound(channel, Duration::from_secs(30)),
        )
        .await;

        let elapsed = start.elapsed();

        match response {
            Ok(Some(msg)) => Ok(TestResult {
                channel: channel.to_string(),
                success: true,
                latency: elapsed,
                response: Some(msg.content),
                error: None,
            }),
            Ok(None) => Ok(TestResult {
                channel: channel.to_string(),
                success: false,
                latency: elapsed,
                response: None,
                error: Some("No response received".to_string()),
            }),
            Err(_) => Ok(TestResult {
                channel: channel.to_string(),
                success: false,
                latency: elapsed,
                response: None,
                error: Some("Timeout".to_string()),
            }),
        }
    }

    /// 批量测试多个渠道
    pub async fn test_all_channels(&self, channels: &[&str]) -> Vec<TestResult> {
        let mut results = Vec::new();

        for channel in channels {
            match self.test_connection(channel).await {
                Ok(result) => results.push(result),
                Err(e) => {
                    results.push(TestResult {
                        channel: channel.to_string(),
                        success: false,
                        latency: Duration::from_secs(0),
                        response: None,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        results
    }
}

/// 测试结果
#[derive(Debug, Clone)]
pub struct TestResult {
    pub channel: String,
    pub success: bool,
    pub latency: Duration,
    pub response: Option<String>,
    pub error: Option<String>,
}

/// 消息记录器
///
/// 记录通过渠道的所有消息用于调试
pub struct MessageLogger {
    messages: Arc<tokio::sync::RwLock<Vec<LoggedMessage>>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LoggedMessage {
    pub direction: String, // "inbound" or "outbound"
    pub channel: String,
    pub chat_id: String,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl MessageLogger {
    pub fn new() -> Self {
        Self {
            messages: Arc::new(tokio::sync::RwLock::new(Vec::new())),
        }
    }

    pub async fn log_incoming(&self, msg: &stormclaw_core::InboundMessage) {
        let logged = LoggedMessage {
            direction: "inbound".to_string(),
            channel: msg.channel.clone(),
            chat_id: msg.chat_id.clone(),
            content: msg.content.clone(),
            timestamp: msg.timestamp,
        };
        self.messages.write().await.push(logged);
    }

    pub async fn log_outgoing(&self, msg: &stormclaw_core::OutboundMessage) {
        let logged = LoggedMessage {
            direction: "outbound".to_string(),
            channel: msg.channel.clone(),
            chat_id: msg.chat_id.clone(),
            content: msg.content.clone(),
            timestamp: chrono::Utc::now(),
        };
        self.messages.write().await.push(logged);
    }

    pub async fn get_messages(&self, limit: usize) -> Vec<LoggedMessage> {
        let messages = self.messages.read().await;
        let start = if messages.len() > limit {
            messages.len() - limit
        } else {
            0
        };
        messages[start..].to_vec()
    }

    pub async fn clear(&self) {
        self.messages.write().await.clear();
    }
}

impl Default for MessageLogger {
    fn default() -> Self {
        Self::new()
    }
}
