//! Agent 运行时参数（可被 Gateway 热重载更新）

use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct AgentRuntimeState {
    pub workspace: PathBuf,
    pub model: Option<String>,
    pub max_iterations: usize,
    pub brave_api_key: Option<String>,
}
