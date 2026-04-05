//! 消息发送工具

use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::bus::{MessageBus, OutboundMessage};
use super::base::Tool;

/// 消息发送工具
///
/// 允许 Agent 向聊天渠道发送消息
pub struct MessageTool {
    bus: Arc<MessageBus>,
    context: Arc<RwLock<Option<MessageContext>>>,
}

struct MessageContext {
    channel: String,
    chat_id: String,
}

impl MessageTool {
    /// 创建新的消息工具
    pub fn new(bus: Arc<MessageBus>) -> Self {
        Self {
            bus,
            context: Arc::new(RwLock::new(None)),
        }
    }

    /// 设置当前上下文（渠道和聊天 ID）
    pub async fn set_context(&self, channel: String, chat_id: String) {
        *self.context.write().await = Some(MessageContext { channel, chat_id });
    }
}

#[async_trait]
impl Tool for MessageTool {
    fn name(&self) -> &str {
        "message"
    }

    fn description(&self) -> &str {
        "Send a message to a user on a chat channel (Telegram, WhatsApp, etc.)"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "Message content to send"
                },
                "channel": {
                    "type": "string",
                    "description": "Target channel (optional, uses current context if not specified)"
                },
                "chat_id": {
                    "type": "string",
                    "description": "Target chat ID (optional, uses current context if not specified)"
                }
            },
            "required": ["content"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let content = args["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'content' argument"))?;

        // 获取目标渠道和聊天 ID
        let (channel, chat_id) = {
            let ctx = self.context.read().await;

            let channel = args["channel"]
                .as_str()
                .or_else(|| ctx.as_ref().map(|c| c.channel.as_str()))
                .ok_or_else(|| anyhow::anyhow!("No channel specified and no context available"))?;

            let chat_id = args["chat_id"]
                .as_str()
                .or_else(|| ctx.as_ref().map(|c| c.chat_id.as_str()))
                .ok_or_else(|| anyhow::anyhow!("No chat_id specified and no context available"))?;

            (channel.to_string(), chat_id.to_string())
        };

        let msg = OutboundMessage::new(channel, chat_id, content);
        self.bus.publish_outbound(msg).await?;

        Ok("Message sent successfully".to_string())
    }
}
