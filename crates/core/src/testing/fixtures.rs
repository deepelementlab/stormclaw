//! 测试数据夹具
//!
//! 提供测试中常用的测试数据和配置。

use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use chrono::Utc;
use serde_json::json;

use crate::{
    MessageBus, InboundMessage, OutboundMessage,
    providers::{ChatMessage, ToolCall},
};

/// 创建测试配置
pub fn create_test_config() -> serde_json::Value {
    json!({
        "agents": {
            "defaults": {
                "model": "gpt-4",
                "maxIterations": 10
            }
        },
        "providers": {
            "openrouter": {
                "apiKey": "test-key-12345"
            }
        },
        "channels": {
            "telegram": {
                "enabled": false,
                "token": "test-token",
                "allowFrom": []
            }
        }
    })
}

/// 创建测试入站消息
pub fn create_test_message() -> InboundMessage {
    InboundMessage {
        channel: "test".to_string(),
        sender_id: "user123".to_string(),
        chat_id: "chat123".to_string(),
        content: "Hello, test!".to_string(),
        timestamp: Utc::now(),
        media: Vec::new(),
        metadata: json!({
            "test": true,
            "source": "fixture"
        }),
    }
}

/// 创建测试入站消息（带自定义内容）
pub fn create_test_message_with_content(content: impl Into<String>) -> InboundMessage {
    let mut msg = create_test_message();
    msg.content = content.into();
    msg
}

/// 创建测试出站消息
pub fn create_test_outbound_message() -> OutboundMessage {
    OutboundMessage {
        channel: "test".to_string(),
        chat_id: "chat123".to_string(),
        content: "Test response".to_string(),
        reply_to: None,
        media: Vec::new(),
        metadata: json!({}),
    }
}

/// 创建测试消息总线
pub fn create_test_message_bus() -> Arc<MessageBus> {
    Arc::new(MessageBus::new(100))
}

/// 创建测试工作区
pub async fn create_test_workspace() -> anyhow::Result<(PathBuf, TempDir)> {
    let temp_dir = tempfile::tempdir()?;
    let workspace = temp_dir.path().join("workspace");
    tokio::fs::create_dir_all(&workspace).await?;
    Ok((workspace, temp_dir))
}

/// 创建测试会话目录
pub async fn create_test_session_dir() -> anyhow::Result<(PathBuf, TempDir)> {
    let temp_dir = tempfile::tempdir()?;
    let sessions_dir = temp_dir.path().join("sessions");
    tokio::fs::create_dir_all(&sessions_dir).await?;
    Ok((sessions_dir, temp_dir))
}

/// 创建测试记忆目录
pub async fn create_test_memory_dir() -> anyhow::Result<(PathBuf, TempDir)> {
    let temp_dir = tempfile::tempdir()?;
    let memory_dir = temp_dir.path().join("memory");
    tokio::fs::create_dir_all(&memory_dir).await?;
    Ok((memory_dir, temp_dir))
}

/// 创建测试聊天消息列表
pub fn create_test_chat_messages() -> Vec<ChatMessage> {
    vec![
        ChatMessage::system("You are a helpful assistant"),
        ChatMessage::user("Hello, how are you?"),
    ]
}

/// 创建测试工具调用
pub fn create_test_tool_call() -> ToolCall {
    ToolCall {
        id: "call_test_123".to_string(),
        name: "test_function".to_string(),
        arguments: json!({
            "param1": "value1",
            "param2": 42
        }),
    }
}

/// 创建文件读取工具调用
pub fn create_read_file_tool_call(path: impl Into<String>) -> ToolCall {
    ToolCall {
        id: "call_read_file".to_string(),
        name: "read_file".to_string(),
        arguments: json!({
            "path": path.into()
        }),
    }
}

/// 创建写入文件工具调用
pub fn create_write_file_tool_call(path: impl Into<String>, content: impl Into<String>) -> ToolCall {
    ToolCall {
        id: "call_write_file".to_string(),
        name: "write_file".to_string(),
        arguments: json!({
            "path": path.into(),
            "content": content.into()
        }),
    }
}

/// 创建 Shell 命令工具调用
pub fn create_exec_tool_call(command: impl Into<String>) -> ToolCall {
    ToolCall {
        id: "call_exec".to_string(),
        name: "exec".to_string(),
        arguments: json!({
            "command": command.into()
        }),
    }
}

/// 创建 Web 搜索工具调用
pub fn create_web_search_tool_call(query: impl Into<String>) -> ToolCall {
    ToolCall {
        id: "call_search".to_string(),
        name: "web_search".to_string(),
        arguments: json!({
            "query": query.into(),
            "count": 5
        }),
    }
}

/// 测试辅助结构：延迟器
pub struct Delayer {
    duration: std::time::Duration,
}

impl Delayer {
    pub fn new(duration: std::time::Duration) -> Self {
        Self { duration }
    }

    pub async fn delay(&self) {
        tokio::time::sleep(self.duration).await;
    }
}

/// 创建短延迟器（10ms）
pub fn create_short_delayer() -> Delayer {
    Delayer::new(std::time::Duration::from_millis(10))
}

/// 创建中等延迟器（100ms）
pub fn create_medium_delayer() -> Delayer {
    Delayer::new(std::time::Duration::from_millis(100))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_test_config() {
        let config = create_test_config();
        assert_eq!(config["agents"]["defaults"]["model"], "gpt-4");
        assert_eq!(config["providers"]["openrouter"]["apiKey"], "test-key-12345");
    }

    #[test]
    fn test_create_test_message() {
        let msg = create_test_message();
        assert_eq!(msg.channel, "test");
        assert_eq!(msg.sender_id, "user123");
        assert_eq!(msg.content, "Hello, test!");
    }

    #[test]
    fn test_create_test_tool_call() {
        let tool_call = create_test_tool_call();
        assert_eq!(tool_call.name, "test_function");
        assert_eq!(tool_call.arguments["param1"], "value1");
        assert_eq!(tool_call.arguments["param2"], 42);
    }

    #[tokio::test]
    async fn test_create_test_workspace() {
        let (workspace, _temp_dir) = create_test_workspace().await.unwrap();
        assert!(workspace.exists());
        assert!(workspace.is_dir());
    }

    #[tokio::test]
    async fn test_delayer() {
        let delayer = create_short_delayer();
        let start = std::time::Instant::now();
        delayer.delay().await;
        assert!(start.elapsed() >= std::time::Duration::from_millis(10));
    }
}
