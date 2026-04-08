//! Agent 核心模块
//!
//! 提供 Agent 循环、上下文构建、子代理等功能

pub mod r#loop;
pub mod context;
pub mod subagent;
pub mod tools;
pub mod runtime_state;
pub mod path_policy;
pub mod docker_exec;
pub mod tool_executor;
pub mod tool_policy;

pub use runtime_state::AgentRuntimeState;
pub use r#loop::AgentLoop;
pub use context::ContextBuilder;
pub use subagent::SubagentManager;
pub use tools::{Tool, ToolRegistry};
pub use tool_executor::{ToolExecutor, ToolInvocationContext};
