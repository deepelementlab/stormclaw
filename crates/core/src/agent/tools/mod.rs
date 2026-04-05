//! 工具系统模块
//!
//! 提供 Tool trait 和 ToolRegistry

use serde_json::Value;

pub mod base;
pub mod registry;
pub mod filesystem;
pub mod shell;
pub mod web;
pub mod message;
pub mod spawn;

#[cfg(feature = "wasm-tools")]
pub mod wasm_guest;

pub use base::Tool;
pub use registry::ToolRegistry;
pub use filesystem::{ReadFileTool, WriteFileTool, EditFileTool, ListDirTool};
pub use shell::ExecTool;
pub use web::{WebSearchTool, WebFetchTool};
pub use message::MessageTool;
pub use spawn::SpawnTool;

#[cfg(feature = "wasm-tools")]
pub use wasm_guest::WasmEvalTool;

pub(crate) fn normalize_tool_arguments(args: &Value) -> anyhow::Result<Value> {
    match args {
        Value::String(s) => Ok(serde_json::from_str::<Value>(s).unwrap_or(Value::String(s.clone()))),
        other => Ok(other.clone()),
    }
}
