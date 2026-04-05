//! Mock LLM Provider
//!
//! 用于测试的模拟 LLM 提供商，可以预设响应内容。

use std::sync::Arc;
use tokio::sync::RwLock;
use async_trait::async_trait;
use crate::providers::{LLMProvider, LLMResponse, ToolCall, ChatMessage, ToolDefinition, Usage};

/// Mock LLM Provider
pub struct MockLLMProvider {
    /// 预设的响应队列
    responses: Arc<RwLock<Vec<String>>>,
    /// 预设的工具调用队列
    tool_calls_responses: Arc<RwLock<Vec<Vec<ToolCall>>>>,
    /// 默认模型名称
    default_model: String,
    /// 记录收到的消息（用于验证）
    received_messages: Arc<RwLock<Vec<Vec<ChatMessage>>>>,
    /// 记录收到的工具（用于验证）
    received_tools: Arc<RwLock<Vec<Option<Vec<ToolDefinition>>>>>,
}

impl MockLLMProvider {
    /// 创建新的 Mock Provider
    pub fn new() -> Self {
        Self {
            responses: Arc::new(RwLock::new(Vec::new())),
            tool_calls_responses: Arc::new(RwLock::new(Vec::new())),
            default_model: "gpt-4".to_string(),
            received_messages: Arc::new(RwLock::new(Vec::new())),
            received_tools: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 设置默认模型名称
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = model.into();
        self
    }

    /// 添加响应到队列
    pub async fn add_response(&self, response: impl Into<String>) {
        self.responses.write().await.push(response.into());
    }

    /// 添加工具调用响应到队列
    pub async fn add_tool_calls(&self, tool_calls: Vec<ToolCall>) {
        self.tool_calls_responses.write().await.push(tool_calls);
    }

    /// 设置单个响应（覆盖队列）
    pub async fn set_response(&self, response: impl Into<String>) {
        let mut responses = self.responses.write().await;
        responses.clear();
        responses.push(response.into());
    }

    /// 获取收到的消息（用于测试验证）
    pub async fn get_received_messages(&self) -> Vec<Vec<ChatMessage>> {
        self.received_messages.read().await.clone()
    }

    /// 获取收到的工具（用于测试验证）
    pub async fn get_received_tools(&self) -> Vec<Option<Vec<ToolDefinition>>> {
        self.received_tools.read().await.clone()
    }

    /// 清除所有记录
    pub async fn clear(&self) {
        self.responses.write().await.clear();
        self.tool_calls_responses.write().await.clear();
        self.received_messages.write().await.clear();
        self.received_tools.write().await.clear();
    }

    /// 创建简单的响应（包含文本内容）
    pub fn simple_response(content: impl Into<String>) -> LLMResponse {
        LLMResponse {
            content: Some(content.into()),
            tool_calls: Vec::new(),
            finish_reason: "stop".to_string(),
            usage: Some(Usage {
                prompt_tokens: 10,
                completion_tokens: 20,
                total_tokens: 30,
            }),
        }
    }

    /// 创建工具调用响应
    pub fn tool_calls_response(tool_calls: Vec<ToolCall>) -> LLMResponse {
        LLMResponse {
            content: None,
            tool_calls,
            finish_reason: "tool_calls".to_string(),
            usage: Some(Usage {
                prompt_tokens: 10,
                completion_tokens: 0,
                total_tokens: 10,
            }),
        }
    }
}

impl Default for MockLLMProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LLMProvider for MockLLMProvider {
    async fn chat(
        &self,
        messages: Vec<ChatMessage>,
        tools: Option<Vec<ToolDefinition>>,
        _model: Option<&str>,
    ) -> anyhow::Result<LLMResponse> {
        // 记录收到的消息和工具
        self.received_messages.write().await.push(messages.clone());
        self.received_tools.write().await.push(tools.clone());

        // 优先返回工具调用响应
        let mut tool_calls_resp = self.tool_calls_responses.write().await;
        if !tool_calls_resp.is_empty() {
            let tool_calls = tool_calls_resp.remove(0);
            return Ok(Self::tool_calls_response(tool_calls));
        }

        // 返回文本响应
        let mut responses = self.responses.write().await;
        if responses.is_empty() {
            // 默认响应
            return Ok(Self::simple_response("Mock response"));
        }

        let content = responses.remove(0);
        Ok(Self::simple_response(content))
    }

    fn get_default_model(&self) -> String {
        self.default_model.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ChatMessage;

    #[tokio::test]
    async fn test_mock_provider_simple_response() {
        let provider = MockLLMProvider::new();

        provider.add_response("Hello, world!").await;

        let messages = vec![ChatMessage::user("Test")];
        let response = provider.chat(messages, None, None).await.unwrap();

        assert_eq!(response.content, Some("Hello, world!".to_string()));
        assert!(!response.has_tool_calls());
    }

    #[tokio::test]
    async fn test_mock_provider_tool_calls() {
        let provider = MockLLMProvider::new();

        let tool_calls = vec![ToolCall {
            id: "call_1".to_string(),
            name: "test_tool".to_string(),
            arguments: serde_json::json!({"arg": "value"}),
        }];

        provider.add_tool_calls(tool_calls).await;

        let messages = vec![ChatMessage::user("Test")];
        let response = provider.chat(messages, None, None).await.unwrap();

        assert!(response.has_tool_calls());
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].name, "test_tool");
    }

    #[tokio::test]
    async fn test_mock_provider_records_messages() {
        let provider = MockLLMProvider::new();

        let messages = vec![
            ChatMessage::system("You are helpful"),
            ChatMessage::user("Hello"),
        ];

        provider.chat(messages.clone(), None, None).await.unwrap();

        let received = provider.get_received_messages().await;
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].len(), 2);
    }
}
