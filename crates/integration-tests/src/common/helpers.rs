//! 集成测试辅助（与当前 `MessageBus` API 一致）

use std::sync::Arc;
use std::time::Duration;
use stormclaw_core::{InboundMessage, MessageBus, OutboundMessage};
use tokio::sync::RwLock;

/// 消息捕获器
pub struct MessageCapture {
    inbound_messages: Arc<RwLock<Vec<InboundMessage>>>,
    outbound_messages: Arc<RwLock<Vec<OutboundMessage>>>,
}

impl MessageCapture {
    pub fn new() -> Self {
        Self {
            inbound_messages: Arc::new(RwLock::new(Vec::new())),
            outbound_messages: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn capture_inbound(&self, msg: InboundMessage) {
        self.inbound_messages.write().await.push(msg);
    }

    pub async fn capture_outbound(&self, msg: OutboundMessage) {
        self.outbound_messages.write().await.push(msg);
    }

    pub async fn get_inbound_count(&self) -> usize {
        self.inbound_messages.read().await.len()
    }

    pub async fn get_outbound_count(&self) -> usize {
        self.outbound_messages.read().await.len()
    }

    pub async fn get_last_inbound(&self) -> Option<InboundMessage> {
        self.inbound_messages.read().await.last().cloned()
    }

    pub async fn get_last_outbound(&self) -> Option<OutboundMessage> {
        self.outbound_messages.read().await.last().cloned()
    }

    pub async fn clear(&self) {
        self.inbound_messages.write().await.clear();
        self.outbound_messages.write().await.clear();
    }
}

/// 在后台消费入站队列，直到收到一条消息（用于与 `publish_inbound` 配对）
pub async fn wait_for_inbound(
    bus: Arc<MessageBus>,
    timeout_ms: u64,
) -> anyhow::Result<InboundMessage> {
    let handle = tokio::spawn(async move { bus.consume_inbound().await });
    let msg = tokio::time::timeout(Duration::from_millis(timeout_ms), handle).await??;
    msg.ok_or_else(|| anyhow::anyhow!("no inbound message"))
}

pub fn create_test_message_with_content(content: &str) -> InboundMessage {
    InboundMessage::new("test", "sender1", "chat1", content)
}

pub fn create_test_batch(count: usize) -> Vec<InboundMessage> {
    (0..count)
        .map(|i| InboundMessage::new("test", "sender1", "chat1", format!("Test message {}", i)))
        .collect()
}

/// 消息计数器
#[derive(Clone)]
pub struct MessageCounter {
    inbound_count: Arc<RwLock<usize>>,
    outbound_count: Arc<RwLock<usize>>,
}

impl MessageCounter {
    pub fn new() -> Self {
        Self {
            inbound_count: Arc::new(RwLock::new(0)),
            outbound_count: Arc::new(RwLock::new(0)),
        }
    }

    pub async fn increment_inbound(&self) {
        let mut count = self.inbound_count.write().await;
        *count += 1;
    }

    pub async fn increment_outbound(&self) {
        let mut count = self.outbound_count.write().await;
        *count += 1;
    }

    pub async fn get_inbound_count(&self) -> usize {
        *self.inbound_count.read().await
    }

    pub async fn get_outbound_count(&self) -> usize {
        *self.outbound_count.read().await
    }

    pub async fn reset(&self) {
        *self.inbound_count.write().await = 0;
        *self.outbound_count.write().await = 0;
    }
}

/// 启动后台任务消费入站/出站 mpsc，用于统计消息数量（各仅一个消费者）
pub async fn create_counting_bus(capacity: usize) -> (Arc<MessageBus>, MessageCounter) {
    let bus = Arc::new(MessageBus::new(capacity));
    let counter = MessageCounter::new();

    let bus_in = bus.clone();
    let c_in = counter.clone();
    tokio::spawn(async move {
        while let Some(_) = bus_in.consume_inbound().await {
            c_in.increment_inbound().await;
        }
    });

    let bus_out = bus.clone();
    let c_out = counter.clone();
    tokio::spawn(async move {
        while let Some(_) = bus_out.consume_outbound().await {
            c_out.increment_outbound().await;
        }
    });

    (bus, counter)
}

pub async fn with_timeout<F, T>(future: F, timeout_ms: u64) -> anyhow::Result<T>
where
    F: std::future::Future<Output = anyhow::Result<T>>,
{
    tokio::time::timeout(Duration::from_millis(timeout_ms), future).await?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_message_capture() {
        let capture = MessageCapture::new();

        let inbound = create_test_message_with_content("x");
        capture.capture_inbound(inbound).await;

        assert_eq!(capture.get_inbound_count().await, 1);
        assert!(capture.get_last_inbound().await.is_some());
    }

    #[tokio::test]
    async fn test_message_counter() {
        let counter = MessageCounter::new();

        counter.increment_inbound().await;
        counter.increment_inbound().await;
        counter.increment_outbound().await;

        assert_eq!(counter.get_inbound_count().await, 2);
        assert_eq!(counter.get_outbound_count().await, 1);

        counter.reset().await;
        assert_eq!(counter.get_inbound_count().await, 0);
    }

    #[tokio::test]
    async fn test_with_timeout() {
        let result = with_timeout(async { Ok::<(), anyhow::Error>(()) }, 100).await;
        assert!(result.is_ok());

        let result = with_timeout(
            async {
                tokio::time::sleep(Duration::from_millis(200)).await;
                Ok::<(), anyhow::Error>(())
            },
            100,
        )
        .await;
        assert!(result.is_err());
    }

}
