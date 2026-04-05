//! 消息事件定义

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 入站消息 - 从渠道发送到 Agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundMessage {
    /// 渠道名称 (telegram, whatsapp, etc.)
    pub channel: String,
    /// 发送者 ID
    pub sender_id: String,
    /// 聊天 ID
    pub chat_id: String,
    /// 消息内容
    pub content: String,
    /// 时间戳
    #[serde(default = "Utc::now")]
    pub timestamp: DateTime<Utc>,
    /// 媒体附件 URL 列表
    #[serde(default)]
    pub media: Vec<String>,
    /// 渠道特定元数据
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl InboundMessage {
    /// 获取会话唯一标识
    pub fn session_key(&self) -> String {
        format!("{}:{}", self.channel, self.chat_id)
    }

    /// 创建新的入站消息
    pub fn new(
        channel: impl Into<String>,
        sender_id: impl Into<String>,
        chat_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            channel: channel.into(),
            sender_id: sender_id.into(),
            chat_id: chat_id.into(),
            content: content.into(),
            timestamp: Utc::now(),
            media: Vec::new(),
            metadata: serde_json::Value::Null,
        }
    }
}

/// 出站消息 - 从 Agent 发送到渠道
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundMessage {
    /// 目标渠道
    pub channel: String,
    /// 目标聊天 ID
    pub chat_id: String,
    /// 消息内容
    pub content: String,
    /// 回复的消息 ID（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    /// 媒体附件 URL 列表
    #[serde(default)]
    pub media: Vec<String>,
    /// 渠道特定元数据
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl OutboundMessage {
    /// 创建新的出站消息
    pub fn new(
        channel: impl Into<String>,
        chat_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            channel: channel.into(),
            chat_id: chat_id.into(),
            content: content.into(),
            reply_to: None,
            media: Vec::new(),
            metadata: serde_json::Value::Null,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_inbound_message_new() {
        let msg = InboundMessage::new("telegram", "user123", "chat456", "Hello!");
        assert_eq!(msg.channel, "telegram");
        assert_eq!(msg.sender_id, "user123");
        assert_eq!(msg.chat_id, "chat456");
        assert_eq!(msg.content, "Hello!");
        assert!(msg.media.is_empty());
    }

    #[test]
    fn test_inbound_message_session_key() {
        let msg = InboundMessage::new("telegram", "user123", "chat456", "Hello!");
        assert_eq!(msg.session_key(), "telegram:chat456");
    }

    #[test]
    fn test_outbound_message_new() {
        let msg = OutboundMessage::new("whatsapp", "chat789", "Hi there!");
        assert_eq!(msg.channel, "whatsapp");
        assert_eq!(msg.chat_id, "chat789");
        assert_eq!(msg.content, "Hi there!");
        assert!(msg.reply_to.is_none());
        assert!(msg.media.is_empty());
    }

    #[test]
    fn test_inbound_message_serialization() {
        let msg = InboundMessage {
            channel: "test".to_string(),
            sender_id: "user1".to_string(),
            chat_id: "chat1".to_string(),
            content: "Test message".to_string(),
            timestamp: Utc::now(),
            media: vec!["media1.jpg".to_string()],
            metadata: json!({"key": "value"}),
        };

        let serialized = serde_json::to_string(&msg).unwrap();
        let deserialized: InboundMessage = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.channel, msg.channel);
        assert_eq!(deserialized.content, msg.content);
        assert_eq!(deserialized.media, msg.media);
        assert_eq!(deserialized.metadata, msg.metadata);
    }

    #[test]
    fn test_outbound_message_serialization() {
        let msg = OutboundMessage {
            channel: "test".to_string(),
            chat_id: "chat1".to_string(),
            content: "Response".to_string(),
            reply_to: Some("msg123".to_string()),
            media: vec![],
            metadata: json!({"reply": true}),
        };

        let serialized = serde_json::to_string(&msg).unwrap();
        let deserialized: OutboundMessage = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.channel, msg.channel);
        assert_eq!(deserialized.reply_to, msg.reply_to);
        assert_eq!(deserialized.metadata, msg.metadata);
    }

    #[test]
    fn test_inbound_message_with_media() {
        let mut msg = InboundMessage::new("telegram", "user1", "chat1", "Photo!");
        msg.media.push("photo1.jpg".to_string());
        msg.media.push("photo2.jpg".to_string());

        assert_eq!(msg.media.len(), 2);
        assert_eq!(msg.media[0], "photo1.jpg");
    }

    #[test]
    fn test_outbound_message_with_reply() {
        let mut msg = OutboundMessage::new("telegram", "chat1", "Reply!");
        msg.reply_to = Some("original_msg".to_string());

        assert_eq!(msg.reply_to, Some("original_msg".to_string()));
    }
}
