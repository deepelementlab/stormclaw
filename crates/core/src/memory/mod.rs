//! 记忆系统模块

use std::path::PathBuf;
use stormclaw_utils::{today_date, ensure_dir, read_file, write_file, append_file};

/// 记忆存储
pub struct MemoryStore {
    workspace: PathBuf,
    memory_dir: PathBuf,
    memory_file: PathBuf,
}

impl MemoryStore {
    /// 创建新的记忆存储
    pub fn new(workspace: PathBuf) -> Self {
        let memory_dir = workspace.join("memory");
        let memory_file = memory_dir.join("MEMORY.md");

        Self {
            workspace,
            memory_dir,
            memory_file,
        }
    }

    /// 获取今日记忆文件路径
    pub fn today_file(&self) -> PathBuf {
        self.memory_dir.join(format!("{}.md", today_date()))
    }

    /// 读取今日记忆
    pub fn read_today(&self) -> anyhow::Result<String> {
        let path = self.today_file();
        if path.exists() {
            read_file(&path)
        } else {
            Ok(String::new())
        }
    }

    /// 追加今日记忆
    pub fn append_today(&self, content: &str) -> anyhow::Result<()> {
        let path = self.today_file();

        if path.exists() {
            let existing = read_file(&path)?;
            write_file(&path, &format!("{}\n{}", existing, content))
        } else {
            let header = format!("# {}\n\n", today_date());
            write_file(&path, &format!("{}{}", header, content))
        }
    }

    /// 读取长期记忆
    pub fn read_long_term(&self) -> anyhow::Result<String> {
        if self.memory_file.exists() {
            read_file(&self.memory_file)
        } else {
            Ok(String::new())
        }
    }

    /// 写入长期记忆
    pub fn write_long_term(&self, content: &str) -> anyhow::Result<()> {
        ensure_dir(&self.memory_dir)?;
        write_file(&self.memory_file, content)
    }

    /// 获取最近 N 天的记忆
    pub fn get_recent_memories(&self, days: usize) -> anyhow::Result<String> {
        let mut memories = Vec::new();

        for i in 0..days {
            let date = chrono::Utc::now() - chrono::Duration::days(i as i64);
            let date_str = date.format("%Y-%m-%d").to_string();
            let path = self.memory_dir.join(format!("{}.md", date_str));

            if path.exists() {
                let content = read_file(&path)?;
                memories.push(content);
            }
        }

        Ok(memories.join("\n\n---\n\n"))
    }

    /// 列出所有记忆文件
    pub fn list_memory_files(&self) -> anyhow::Result<Vec<PathBuf>> {
        if !self.memory_dir.exists() {
            return Ok(Vec::new());
        }

        let mut files = Vec::new();
        for entry in std::fs::read_dir(&self.memory_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                files.push(path);
            }
        }

        files.sort();
        files.reverse();
        Ok(files)
    }

    /// 获取记忆上下文（用于 Agent）
    pub fn get_memory_context(&self) -> anyhow::Result<String> {
        let mut parts = Vec::new();

        // 长期记忆
        if let Ok(long_term) = self.read_long_term() {
            if !long_term.is_empty() {
                parts.push(format!("## Long-term Memory\n{}", long_term));
            }
        }

        // 今日记忆
        if let Ok(today) = self.read_today() {
            if !today.is_empty() {
                parts.push(format!("## Today's Notes\n{}", today));
            }
        }

        Ok(parts.join("\n\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    async fn create_test_store() -> anyhow::Result<(MemoryStore, TempDir)> {
        let temp_dir = tempfile::tempdir()?;
        let workspace = temp_dir.path().join("workspace");
        tokio::fs::create_dir_all(&workspace).await?;

        let memory_dir = workspace.join("memory");
        tokio::fs::create_dir_all(&memory_dir).await?;

        let store = MemoryStore {
            workspace: workspace.clone(),
            memory_dir,
            memory_file: workspace.join("memory").join("MEMORY.md"),
        };

        Ok((store, temp_dir))
    }

    #[tokio::test]
    async fn test_memory_store_new() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workspace = temp_dir.path().join("workspace");
        tokio::fs::create_dir_all(&workspace).await.unwrap();

        let store = MemoryStore::new(workspace.clone());

        assert_eq!(store.workspace, workspace);
        assert_eq!(store.memory_dir, workspace.join("memory"));
        assert_eq!(store.memory_file, workspace.join("memory").join("MEMORY.md"));
    }

    #[tokio::test]
    async fn test_read_today_empty() {
        let (store, _temp_dir) = create_test_store().await.unwrap();

        let content = store.read_today().unwrap();
        assert!(content.is_empty());
    }

    #[tokio::test]
    async fn test_append_today() {
        let (store, _temp_dir) = create_test_store().await.unwrap();

        store.append_today("Test memory").unwrap();

        let content = store.read_today().unwrap();
        assert!(content.contains("Test memory"));
        assert!(content.contains(&format!("# {}", chrono::Utc::now().format("%Y-%m-%d"))));
    }

    #[tokio::test]
    async fn test_append_multiple() {
        let (store, _temp_dir) = create_test_store().await.unwrap();

        store.append_today("First memory").unwrap();
        store.append_today("Second memory").unwrap();

        let content = store.read_today().unwrap();
        assert!(content.contains("First memory"));
        assert!(content.contains("Second memory"));
    }

    #[tokio::test]
    async fn test_read_long_term_empty() {
        let (store, _temp_dir) = create_test_store().await.unwrap();

        let content = store.read_long_term().unwrap();
        assert!(content.is_empty());
    }

    #[tokio::test]
    async fn test_write_long_term() {
        let (store, _temp_dir) = create_test_store().await.unwrap();

        store.write_long_term("Important information").unwrap();

        let content = store.read_long_term().unwrap();
        assert_eq!(content, "Important information");
    }

    #[tokio::test]
    async fn test_get_recent_memories() {
        let (store, _temp_dir) = create_test_store().await.unwrap();

        // 创建今天和昨天的记忆文件
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let yesterday = (chrono::Utc::now() - chrono::Duration::days(1))
            .format("%Y-%m-%d").to_string();

        let today_path = store.memory_dir.join(format!("{}.md", today));
        let yesterday_path = store.memory_dir.join(format!("{}.md", yesterday));

        fs::write(&today_path, "Today's notes").unwrap();
        fs::write(&yesterday_path, "Yesterday's notes").unwrap();

        let recent = store.get_recent_memories(2).unwrap();
        assert!(recent.contains("Today's notes"));
        assert!(recent.contains("Yesterday's notes"));
    }

    #[tokio::test]
    async fn test_list_memory_files() {
        let (store, _temp_dir) = create_test_store().await.unwrap();

        // 创建一些测试文件
        fs::write(store.memory_dir.join("2024-01-01.md"), "Memory 1").unwrap();
        fs::write(store.memory_dir.join("2024-01-02.md"), "Memory 2").unwrap();
        fs::write(store.memory_dir.join("MEMORY.md"), "Long term").unwrap();

        let files = store.list_memory_files().unwrap();
        assert_eq!(files.len(), 3);
    }

    #[tokio::test]
    async fn test_get_memory_context() {
        let (store, _temp_dir) = create_test_store().await.unwrap();

        store.write_long_term("Long term info").unwrap();
        store.append_today("Today's info").unwrap();

        let context = store.get_memory_context().unwrap();
        assert!(context.contains("Long-term Memory"));
        assert!(context.contains("Long term info"));
        assert!(context.contains("Today's Notes"));
        assert!(context.contains("Today's info"));
    }

    #[tokio::test]
    async fn test_get_memory_context_empty() {
        let (store, _temp_dir) = create_test_store().await.unwrap();

        let context = store.get_memory_context().unwrap();
        assert!(context.is_empty());
    }

    #[tokio::test]
    async fn test_today_file_path() {
        let (store, _temp_dir) = create_test_store().await.unwrap();

        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let expected = store.memory_dir.join(format!("{}.md", today));

        assert_eq!(store.today_file(), expected);
    }

    #[tokio::test]
    async fn test_overwrite_long_term() {
        let (store, _temp_dir) = create_test_store().await.unwrap();

        store.write_long_term("First version").unwrap();
        store.write_long_term("Second version").unwrap();

        let content = store.read_long_term().unwrap();
        assert_eq!(content, "Second version");
    }
}
