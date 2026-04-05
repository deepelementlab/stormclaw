//! 渠道适配器基类和通用接口

use async_trait::async_trait;
use std::sync::Arc;
use stormclaw_core::{MessageBus, InboundMessage, OutboundMessage};
use anyhow::Result;

/// 渠道适配器基类
///
/// 所有渠道实现都需要实现此 trait
#[async_trait]
pub trait BaseChannel: Send + Sync {
    /// 获取渠道名称
    fn name(&self) -> &str;

    /// 启动渠道监听
    ///
    /// 这是一个长时间运行的异步任务，负责：
    /// 1. 连接到聊天平台
    /// 2. 监听传入的消息
    /// 3. 通过 handle_message() 转发消息到总线
    async fn start(&self) -> Result<()>;

    /// 停止渠道并清理资源
    async fn stop(&self) -> Result<()>;

    /// 发送消息到渠道
    async fn send(&self, msg: &OutboundMessage) -> Result<()>;

    /// 检查发送者是否被允许使用此机器人
    fn is_allowed(&self, sender_id: &str) -> bool {
        true // 默认允许所有人
    }

    /// 处理从聊天平台接收到的消息
    ///
    /// 此方法检查权限并转发消息到总线
    async fn handle_message(
        &self,
        sender_id: String,
        chat_id: String,
        content: String,
    ) -> Result<()> {
        if !self.is_allowed(&sender_id) {
            tracing::debug!("Rejected message from unauthorized sender: {}", sender_id);
            return Ok(());
        }

        let msg = InboundMessage::new(self.name(), sender_id, chat_id, content);
        self.bus().publish_inbound(msg).await?;
        Ok(())
    }

    /// 获取消息总线引用
    fn bus(&self) -> &MessageBus;

    /// 检查渠道是否正在运行
    fn is_running(&self) -> bool {
        false
    }
}

/// 渠道工厂
///
/// 根据配置创建渠道实例
pub struct ChannelFactory;

impl ChannelFactory {
    /// 创建 Telegram 渠道
    pub fn create_telegram(
        config: &stormclaw_config::TelegramConfig,
        bus: Arc<MessageBus>,
    ) -> Result<Arc<dyn BaseChannel>> {
        Ok(Arc::new(crate::telegram::TelegramChannel::new(
            config.token.clone(),
            config.allow_from.clone(),
            bus,
        )?))
    }

    /// 创建 WhatsApp 渠道
    pub fn create_whatsapp(
        config: &stormclaw_config::WhatsAppConfig,
        bus: Arc<MessageBus>,
    ) -> Result<Arc<dyn BaseChannel>> {
        Ok(Arc::new(crate::whatsapp::WhatsAppChannel::new(
            config.bridge_url.clone(),
            config.allow_from.clone(),
            bus,
        )?))
    }
}

/// 运行时状态跟踪
#[derive(Debug, Clone, Copy)]
pub struct ChannelState {
    pub is_running: bool,
    pub connected_at: Option<chrono::DateTime<chrono::Utc>>,
    pub messages_sent: u64,
    pub messages_received: u64,
}

impl Default for ChannelState {
    fn default() -> Self {
        Self {
            is_running: false,
            connected_at: None,
            messages_sent: 0,
            messages_received: 0,
        }
    }
}
