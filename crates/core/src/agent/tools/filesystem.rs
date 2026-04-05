//! 文件系统工具（路径限制在工作区内）

use async_trait::async_trait;
use anyhow::Context;
use serde_json::{json, Value};
use std::path::PathBuf;

use super::base::Tool;
use crate::agent::path_policy::resolve_under_workspace;

/// 文件读取工具
pub struct ReadFileTool {
    pub workspace_root: PathBuf,
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file under the workspace (path must be relative, no '..')"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative path under the workspace"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' argument"))?;

        let full = resolve_under_workspace(&self.workspace_root, path)?;
        let content = tokio::fs::read_to_string(&full)
            .await
            .with_context(|| format!("Failed to read file: {}", full.display()))?;

        Ok(content)
    }
}

/// 文件写入工具
pub struct WriteFileTool {
    pub workspace_root: PathBuf,
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to a file under the workspace (path must be relative, no '..')"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative path under the workspace"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' argument"))?;

        let content = args["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'content' argument"))?;

        let full = resolve_under_workspace(&self.workspace_root, path)?;

        if let Some(parent) = full.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .context("Failed to create parent directory")?;
        }

        tokio::fs::write(&full, content)
            .await
            .with_context(|| format!("Failed to write file: {}", full.display()))?;

        Ok(format!(
            "Successfully wrote {} bytes to {}",
            content.len(),
            full.display()
        ))
    }
}

/// 文件编辑工具
pub struct EditFileTool {
    pub workspace_root: PathBuf,
}

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn description(&self) -> &str {
        "Edit a file under the workspace by replacing exact text matches"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative path under the workspace"
                },
                "old_text": {
                    "type": "string",
                    "description": "Exact text to replace"
                },
                "new_text": {
                    "type": "string",
                    "description": "New text to insert"
                }
            },
            "required": ["path", "old_text", "new_text"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' argument"))?;

        let old_text = args["old_text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'old_text' argument"))?;

        let new_text = args["new_text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'new_text' argument"))?;

        let full = resolve_under_workspace(&self.workspace_root, path)?;

        let content = tokio::fs::read_to_string(&full).await?;

        if !content.contains(old_text) {
            anyhow::bail!("Old text not found in file");
        }

        let new_content = content.replace(old_text, new_text);
        tokio::fs::write(&full, new_content).await?;

        Ok(format!("Successfully edited {}", full.display()))
    }
}

/// 目录列表工具
pub struct ListDirTool {
    pub workspace_root: PathBuf,
}

#[async_trait]
impl Tool for ListDirTool {
    fn name(&self) -> &str {
        "list_dir"
    }

    fn description(&self) -> &str {
        "List files and directories in a path under the workspace (relative path, use '.' for workspace root)"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative directory under the workspace (e.g. '.' or 'src')"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' argument"))?;

        let full = resolve_under_workspace(&self.workspace_root, path)?;

        let mut dir = tokio::fs::read_dir(&full).await?;
        let mut entries = Vec::new();

        while let Some(entry) = dir.next_entry().await? {
            entries.push(entry);
        }

        entries.sort_by_key(|e| e.file_name());

        let mut result = String::new();
        for entry in entries {
            let name = entry.file_name().to_string_lossy().to_string();
            let metadata = entry.metadata().await?;
            let kind = if metadata.is_dir() { "DIR" } else { "FILE" };
            result.push_str(&format!("[{}] {}\n", kind, name));
        }

        Ok(result)
    }
}
