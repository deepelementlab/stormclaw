//! 将 LLM 提供的相对路径限制在工作区内，禁止 `..` 与绝对路径逃逸。

use std::path::{Path, PathBuf};

/// 解析用户路径：必须为相对路径、不含 `..`，结果位于 `workspace_root` 下。
pub fn resolve_under_workspace(workspace_root: &Path, user_path: &str) -> anyhow::Result<PathBuf> {
    if user_path.contains("..") {
        anyhow::bail!("path must not contain '..'");
    }
    let p = Path::new(user_path);
    if p.is_absolute() {
        anyhow::bail!("path must be relative to the workspace root");
    }
    Ok(workspace_root.join(p))
}
