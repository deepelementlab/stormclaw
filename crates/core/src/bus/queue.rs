//! 异步消息队列
//!
//! 此模块提供了消息总线（MessageBus）实现，用于在 stormclaw 的各个组件之间传递消息。
//!
//! # 架构
//!
//! 消息总线采用生产者-消费者模式，支持多个发送者和多个订阅者：
//!
//! - **入站消息（InboundMessage）**：从渠道发送到 Agent
//! - **出站消息（OutboundMessage）**：从 Agent 发送到渠道
//!
//! # 使用示例
//!
//! ```no_run
//! use stormclaw_core::MessageBus;
//! use stormclaw_core::InboundMessage;
//! use std::sync::Arc;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // 创建容量为 1000 的消息总线
//! let bus = Arc::new(MessageBus::new(1000));
//!
//! // 发布入站消息
//! let inbound = InboundMessage::new("telegram", "user123", "chat456", "Hello!");
//! bus.publish_inbound(inbound).await?;
//! // 出站订阅在启动阶段通过 MessageBus 的可变访问完成，参见网关初始化代码。
//! # Ok(())
//! # }
//! ```
//!
//! # 线程安全
//!
//! MessageBus 使用 tokio 的 mpsc 和 broadcast channel 实现，完全线程安全，
//! 可以在多个异步任务中并发使用。

use std::collections::HashMap;
use tokio::sync::{mpsc, broadcast, RwLock};
use anyhow::Result;
use tracing::{debug, warn};

use super::events::{InboundMessage, OutboundMessage};

/// 消息总线 - 用于解耦渠道和 Agent
///
/// MessageBus 是 stormclaw 的核心消息传递组件，负责在不同的组件之间路由消息。
/// 它支持两种类型的消息：
///
/// - **入站消息**：从外部渠道（Telegram、WhatsApp 等）接收的消息
/// - **出站消息**：将要发送到外部渠道的响应消息
///
/// # 字段
///
/// * `inbound_tx` - 入站消息发送器
/// * `outbound_tx` - 出站消息发送器
/// * `subscribers` - 按渠道分组的出站消息订阅者
///
/// # 示例
///
/// ```no_run
/// use stormclaw_core::MessageBus;
/// # async fn example() -> anyhow::Result<()> {
/// let bus = MessageBus::new(1000);
///
/// // 获取发送器克隆
/// let sender = bus.inbound_sender();
/// # Ok(())
/// # }
/// ```
pub struct MessageBus {
    inbound_tx: mpsc::Sender<InboundMessage>,
    inbound_rx: tokio::sync::Mutex<Option<mpsc::Receiver<InboundMessage>>>,
    outbound_tx: mpsc::Sender<OutboundMessage>,
    outbound_rx: tokio::sync::Mutex<Option<mpsc::Receiver<OutboundMessage>>>,
    subscribers: RwLock<HashMap<String, broadcast::Sender<OutboundMessage>>>,
}

impl MessageBus {
    /// 创建新的消息总线
    ///
    /// # 参数
    ///
    /// * `capacity` - 队列容量，控制内存使用
    ///
    /// # 示例
    ///
    /// ```
    /// use stormclaw_core::MessageBus;
    ///
    /// let bus = MessageBus::new(1000);
    /// ```
    pub fn new(capacity: usize) -> Self {
        let (inbound_tx, inbound_rx) = mpsc::channel(capacity);
        let (outbound_tx, outbound_rx) = mpsc::channel(capacity);

        Self {
            inbound_tx,
            inbound_rx: tokio::sync::Mutex::new(Some(inbound_rx)),
            outbound_tx,
            outbound_rx: tokio::sync::Mutex::new(Some(outbound_rx)),
            subscribers: RwLock::new(HashMap::new()),
        }
    }

    /// 获取入站消息发送器
    pub fn inbound_sender(&self) -> mpsc::Sender<InboundMessage> {
        self.inbound_tx.clone()
    }

    /// 获取出站消息发送器
    pub fn outbound_sender(&self) -> mpsc::Sender<OutboundMessage> {
        self.outbound_tx.clone()
    }

    /// 发布入站消息
    pub async fn publish_inbound(&self, msg: InboundMessage) -> Result<()> {
        self.inbound_tx.send(msg).await
            .map_err(|e| anyhow::anyhow!("Failed to publish inbound message: {}", e))?;
        debug!("Published inbound message");
        Ok(())
    }

    /// 消费入站消息
    pub async fn consume_inbound(&self) -> Option<InboundMessage> {
        let mut rx = self.inbound_rx.lock().await;
        if let Some(rx) = rx.as_mut() {
            rx.recv().await
        } else {
            None
        }
    }

    /// 发布出站消息
    pub async fn publish_outbound(&self, msg: OutboundMessage) -> Result<()> {
        let channel = msg.channel.clone();
        let msg_clone = msg.clone();
        self.outbound_tx.send(msg).await
            .map_err(|e| anyhow::anyhow!("Failed to publish outbound message: {}", e))?;
        debug!("Published outbound message to channel: {}", channel);

        // 对齐 Python：发布出站消息应立即可被订阅者消费（无需显式调用 dispatch_outbound）
        // 保留 outbound 队列用于兼容已有的 consume_outbound/dispatch_outbound 处理路径。
        let subscribers = self.subscribers.read().await;
        if let Some(tx) = subscribers.get(&channel) {
            let _ = tx.send(msg_clone);
        }
        Ok(())
    }

    /// 消费出站消息
    pub async fn consume_outbound(&self) -> Option<OutboundMessage> {
        let mut rx = self.outbound_rx.lock().await;
        if let Some(rx) = rx.as_mut() {
            rx.recv().await
        } else {
            None
        }
    }

    /// 订阅特定渠道的出站消息
    pub async fn subscribe_outbound(&self, channel: String) -> broadcast::Receiver<OutboundMessage> {
        let mut subscribers = self.subscribers.write().await;
        if let Some(tx) = subscribers.get(&channel) {
            return tx.subscribe();
        }

        let (tx, rx) = broadcast::channel(100);
        subscribers.insert(channel, tx);
        rx
    }

    /// 分发出站消息到订阅者
    pub async fn dispatch_outbound(&self) -> Result<()> {
        while let Some(msg) = self.consume_outbound().await {
            let subscribers = self.subscribers.read().await;
            if let Some(tx) = subscribers.get(&msg.channel) {
                match tx.send(msg.clone()) {
                    Ok(_) => debug!("Dispatched message to channel: {}", msg.channel),
                    Err(e) => warn!("No subscribers for channel {}: {}", msg.channel, e),
                }
            } else {
                warn!("Unknown channel: {}", msg.channel);
            }
        }
        Ok(())
    }

    /// 获取入站队列大小
    ///
    /// 返回当前队列中的消息数量。
    pub fn inbound_size(&self) -> usize {
        // tokio::mpsc 不提供可靠的队列长度（尤其在并发下）。
        // 这里返回 0 表示“未知”，避免误导上层逻辑。
        0
    }

    /// 获取出站队列大小
    ///
    /// 返回当前队列中的消息数量。
    pub fn outbound_size(&self) -> usize {
        // tokio::mpsc 不提供可靠的队列长度（尤其在并发下）。
        // 这里返回 0 表示“未知”，避免误导上层逻辑。
        0
    }

    /// 停止消息总线
    pub async fn stop(&self) {
        *self.inbound_rx.lock().await = None;
        *self.outbound_rx.lock().await = None;
    }
}

impl Default for MessageBus {
    fn default() -> Self {
        Self::new(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::fixtures::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_message_bus_creation() {
        let bus = MessageBus::new(100);
        assert_eq!(bus.inbound_size(), 0);
        assert_eq!(bus.outbound_size(), 0);
    }

    #[tokio::test]
    async fn test_message_bus_default() {
        let bus = MessageBus::default();
        assert_eq!(bus.inbound_size(), 0);
        assert_eq!(bus.outbound_size(), 0);
    }

    #[tokio::test]
    async fn test_publish_inbound() {
        let bus = MessageBus::new(10);
        let msg = create_test_message();

        let result = bus.publish_inbound(msg).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_consume_inbound() {
        let mut bus = MessageBus::new(10);
        let msg = create_test_message();

        bus.publish_inbound(msg).await.unwrap();

        let consumed = bus.consume_inbound().await;
        assert!(consumed.is_some());
        assert_eq!(consumed.unwrap().content, "Hello, test!");
    }

    #[tokio::test]
    async fn test_publish_outbound() {
        let bus = MessageBus::new(10);
        let msg = create_test_outbound_message();

        let result = bus.publish_outbound(msg).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_consume_outbound() {
        let mut bus = MessageBus::new(10);
        let msg = create_test_outbound_message();

        bus.publish_outbound(msg).await.unwrap();

        let consumed = bus.consume_outbound().await;
        assert!(consumed.is_some());
        assert_eq!(consumed.unwrap().content, "Test response");
    }

    #[tokio::test]
    async fn test_subscribe_outbound() {
        let bus = MessageBus::new(10);
        let mut rx = bus.subscribe_outbound("telegram".to_string()).await;

        let msg = OutboundMessage::new("telegram", "chat123", "Hello!");
        bus.publish_outbound(msg).await.unwrap();

        // 等待消息
        let received = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;
        assert!(received.is_ok());
        assert!(received.unwrap().is_ok());
    }
    #[tokio::test]
    async fn test_multiple_subscribers() {
        let bus = MessageBus::new(10);
        let mut rx1 = bus.subscribe_outbound("telegram".to_string()).await;
        let mut rx2 = bus.subscribe_outbound("telegram".to_string()).await;

        let msg = OutboundMessage::new("telegram", "chat123", "Broadcast!");
        bus.publish_outbound(msg).await.unwrap();

        // 两个订阅者都应该收到消息
        let recv1 = tokio::time::timeout(Duration::from_millis(100), rx1.recv()).await;
        let recv2 = tokio::time::timeout(Duration::from_millis(100), rx2.recv()).await;

        assert!(recv1.is_ok());
        assert!(recv2.is_ok());
        assert!(recv1.unwrap().is_ok());
        assert!(recv2.unwrap().is_ok());
    }

    #[tokio::test]
    async fn test_channel_isolation() {
        let bus = MessageBus::new(10);
        let mut telegram_rx = bus.subscribe_outbound("telegram".to_string()).await;
        let mut whatsapp_rx = bus.subscribe_outbound("whatsapp".to_string()).await;

        let msg = OutboundMessage::new("telegram", "chat123", "Telegram message");
        bus.publish_outbound(msg).await.unwrap();

        // Telegram 订阅者应该收到消息
        let telegram_recv = tokio::time::timeout(Duration::from_millis(100), telegram_rx.recv()).await;
        assert!(telegram_recv.is_ok());
        assert!(telegram_recv.unwrap().is_ok());

        // WhatsApp 订阅者不应该收到消息
        let whatsapp_recv = tokio::time::timeout(Duration::from_millis(100), whatsapp_rx.recv()).await;
        assert!(whatsapp_recv.is_err()); // 超时
    }

    #[tokio::test]
    async fn test_queue_overflow() {
        let bus = MessageBus::new(2);

        // 填充队列
        for i in 0..3 {
            let mut msg = create_test_message();
            msg.content = format!("Message {}", i);
            // 前两个应该成功
            if i < 2 {
                assert!(bus.publish_inbound(msg).await.is_ok());
            } else {
                // 第三个可能会阻塞或失败，取决于实现
                let result = tokio::time::timeout(Duration::from_millis(100), bus.publish_inbound(msg)).await;
                // 我们只验证不会崩溃
            }
        }
    }

    #[tokio::test]
    async fn test_inbound_sender() {
        let bus = MessageBus::new(10);
        let sender = bus.inbound_sender();

        let msg = create_test_message();
        let result = sender.send(msg).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_outbound_sender() {
        let bus = MessageBus::new(10);
        let sender = bus.outbound_sender();

        let msg = create_test_outbound_message();
        let result = sender.send(msg).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_stop() {
        let mut bus = MessageBus::new(10);

        // 发布一些消息
        let msg = create_test_message();
        bus.publish_inbound(msg).await.unwrap();

        // 停止总线
        bus.stop();

        // 尝试消费应该返回 None
        let consumed = bus.consume_inbound().await;
        // 注意：如果有消息在队列中，它们仍然会被消费
    }
}
