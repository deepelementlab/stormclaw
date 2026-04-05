//! LLM 提供商模块
//!
//! 提供 LLM 抽象接口和 OpenAI 兼容实现

pub mod base;
pub mod openai;

pub use base::{LLMProvider, LLMResponse, ToolCall, ChatMessage, ToolDefinition, FunctionDefinition, Usage};
pub use openai::OpenAIProvider;
