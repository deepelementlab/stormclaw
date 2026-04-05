//! stormclaw 核心库
//!
//! 提供消息总线、Agent 引擎、工具系统、会话管理等核心功能

pub mod bus;
pub mod agent;
pub mod providers;
pub mod session;
pub mod memory;
pub mod skills;

cfg_if::cfg_if! {
    if #[cfg(any(test, feature = "test-utils"))] {
        pub mod testing;
    }
}

pub use bus::{MessageBus, InboundMessage, OutboundMessage};
pub use agent::{AgentLoop, ContextBuilder, SubagentManager};
pub use providers::{LLMProvider, LLMResponse, ToolCall, ChatMessage, ToolDefinition, FunctionDefinition};
pub use session::{Session, SessionManager};
pub use memory::{MemoryStore};
pub use skills::{SkillsLoader};
