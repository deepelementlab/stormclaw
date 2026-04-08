//! 将 LLM 提供的相对路径限制在工作区内，禁止 `..` 与绝对路径逃逸。

use std::path::{Component, Path, PathBuf};

/// 解析用户路径：必须为相对路径、不含 `..`，结果位于 `workspace_root` 下。
pub fn resolve_under_workspace(workspace_root: &Path, user_path: &str) -> anyhow::Result<PathBuf> {
    let p = Path::new(user_path);
    if p.is_absolute() {
        anyhow::bail!("path must be relative to the workspace root");
    }

    let mut normalized = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::CurDir => {}
            Component::Normal(seg) => normalized.push(seg),
            Component::ParentDir => anyhow::bail!("path traversal is not allowed"),
            Component::RootDir | Component::Prefix(_) => {
                anyhow::bail!("path must be relative to the workspace root")
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        anyhow::bail!("path must not be empty");
    }

    let joined = workspace_root.join(&normalized);
    if !joined.starts_with(workspace_root) {
        anyhow::bail!("resolved path escaped workspace root");
    }
    Ok(joined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn rejects_dotdot() {
        let ws = PathBuf::from("/tmp/ws");
        assert!(resolve_under_workspace(&ws, "../etc/passwd").is_err());
        assert!(resolve_under_workspace(&ws, "a/../b.txt").is_err());
    }

    #[test]
    fn rejects_absolute() {
        let ws = PathBuf::from("/tmp/ws");
        assert!(resolve_under_workspace(&ws, "/etc/passwd").is_err());
    }

    #[test]
    fn joins_relative() {
        let ws = PathBuf::from("/tmp/ws");
        let p = resolve_under_workspace(&ws, "src/main.rs").unwrap();
        assert!(p.ends_with("src/main.rs"));
    }

    #[test]
    fn rejects_empty_path() {
        let ws = PathBuf::from("/tmp/ws");
        assert!(resolve_under_workspace(&ws, ".").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn rejects_windows_prefix_and_unc() {
        let ws = PathBuf::from("C:\\ws");
        assert!(resolve_under_workspace(&ws, "C:\\Windows\\System32").is_err());
        assert!(resolve_under_workspace(&ws, "\\\\server\\share\\x.txt").is_err());
    }
}
