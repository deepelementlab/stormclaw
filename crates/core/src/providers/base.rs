//! LLM 提供商抽象接口

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 聊天消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// 消息角色 (system, user, assistant, tool)
    pub role: String,
    /// 消息内容
    pub content: String,
    /// 工具调用 ID (仅 tool 角色使用)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// 工具调用列表 (仅 assistant 角色使用)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl ChatMessage {
    /// 创建系统消息
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    /// 创建用户消息
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    /// 创建助手消息
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    /// 创建工具消息
    pub fn tool(content: impl Into<String>, tool_call_id: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: None,
        }
    }
}

/// 工具调用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// 工具调用 ID
    pub id: String,
    /// 工具名称
    pub name: String,
    /// 工具参数
    pub arguments: serde_json::Value,
}

/// 工具定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// 类型（通常为 "function"）
    pub r#type: String,
    /// 函数定义
    pub function: FunctionDefinition,
}

/// 函数定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// 函数名称
    pub name: String,
    /// 函数描述
    pub description: String,
    /// 参数 JSON Schema
    pub parameters: serde_json::Value,
}

/// Token 使用统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    /// 输入 token 数
    pub prompt_tokens: u32,
    /// 输出 token 数
    pub completion_tokens: u32,
    /// 总 token 数
    pub total_tokens: u32,
}

/// LLM 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMResponse {
    /// 响应内容
    pub content: Option<String>,
    /// 工具调用列表
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    /// 结束原因
    pub finish_reason: String,
    /// Token 使用统计
    pub usage: Option<Usage>,
}

impl LLMResponse {
    /// 检查是否包含工具调用
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}

/// LLM 提供商抽象接口
#[async_trait]
pub trait LLMProvider: Send + Sync {
    /// 发送聊天完成请求
    async fn chat(
        &self,
        messages: Vec<ChatMessage>,
        tools: Option<Vec<ToolDefinition>>,
        model: Option<&str>,
    ) -> anyhow::Result<LLMResponse>;

    /// 获取默认模型名称（克隆自提供商内部状态，便于热更新凭据）
    fn get_default_model(&self) -> String;
}
