//! 会话管理器

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::fs;

/// 会话消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,
}

/// 会话
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub key: String,
    #[serde(default)]
    pub messages: Vec<SessionMessage>,
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl Session {
    /// 创建新会话
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            messages: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// 添加消息
    pub fn add_message(&mut self, role: impl Into<String>, content: impl Into<String>) {
        self.messages.push(SessionMessage {
            role: role.into(),
            content: content.into(),
            timestamp: Some(Utc::now()),
        });
        self.updated_at = Utc::now();
    }

    /// 获取历史记录（用于 LLM）
    pub fn get_history(&self, max_messages: usize) -> Vec<HashMap<String, String>> {
        let start = if self.messages.len() > max_messages {
            self.messages.len() - max_messages
        } else {
            0
        };

        self.messages[start..]
            .iter()
            .map(|m| {
                let mut map = HashMap::new();
                map.insert("role".to_string(), m.role.clone());
                map.insert("content".to_string(), m.content.clone());
                map
            })
            .collect()
    }

    /// 清空消息
    pub fn clear(&mut self) {
        self.messages.clear();
        self.updated_at = Utc::now();
    }
}

/// 会话管理器
#[derive(Clone)]
pub struct SessionManager {
    sessions_dir: PathBuf,
    cache: Arc<RwLock<HashMap<String, Session>>>,
}

/// 会话信息（用于列表展示 / Gateway API）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub key: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub path: PathBuf,
    pub message_count: usize,
}

impl SessionManager {
    /// 创建新的会话管理器
    pub async fn new(workspace: PathBuf) -> anyhow::Result<Self> {
        // 对齐 Python：会话存储在 ~/.stormclaw/sessions（不依赖 workspace）
        let _ = workspace;
        let sessions_dir = stormclaw_utils::config_dir().join("sessions");
        tokio::fs::create_dir_all(&sessions_dir).await?;

        Ok(Self {
            sessions_dir,
            cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// 获取会话文件路径
    fn session_path(&self, key: &str) -> PathBuf {
        let safe_key = stormclaw_utils::safe_filename(key).replace(":", "_");
        self.sessions_dir.join(format!("{}.jsonl", safe_key))
    }

    /// 获取或创建会话
    pub async fn get_or_create(&self, key: &str) -> anyhow::Result<Session> {
        // 检查缓存
        {
            let cache = self.cache.read().await;
            if let Some(session) = cache.get(key) {
                return Ok(session.clone());
            }
        }

        // 从磁盘加载
        let session = if let Ok(Some(loaded)) = self.load(key).await {
            loaded
        } else {
            Session::new(key)
        };

        // 更新缓存
        {
            let mut cache = self.cache.write().await;
            cache.insert(key.to_string(), session.clone());
        }

        Ok(session)
    }

    /// 从磁盘加载会话
    async fn load(&self, key: &str) -> anyhow::Result<Option<Session>> {
        let path = self.session_path(key);

        if !path.exists() {
            return Ok(None);
        }

        let content = tokio::fs::read_to_string(&path).await?;
        let mut messages = Vec::new();
        let mut created_at = None;
        let mut updated_at = None;
        let mut metadata = HashMap::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(_type) = value.get("_type").and_then(|v| v.as_str()) {
                    if _type == "metadata" {
                        created_at = value.get("created_at")
                            .and_then(|v| v.as_str())
                            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                            .map(|dt| dt.with_timezone(&Utc));
                        updated_at = value.get("updated_at")
                            .and_then(|v| v.as_str())
                            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                            .map(|dt| dt.with_timezone(&Utc));

                        if let Some(meta) = value.get("metadata").and_then(|v| v.as_object()) {
                            for (k, v) in meta {
                                if let Some(s) = v.as_str() {
                                    metadata.insert(k.clone(), s.to_string());
                                }
                            }
                        }
                    }
                } else if let Ok(msg) = serde_json::from_value::<SessionMessage>(value) {
                    messages.push(msg);
                }
            }
        }

        Ok(Some(Session {
            key: key.to_string(),
            messages,
            created_at: created_at.unwrap_or_else(Utc::now),
            updated_at: updated_at.unwrap_or_else(Utc::now),
            metadata,
        }))
    }

    /// 获取会话（如果不存在返回 None）
    pub async fn get(&self, key: &str) -> anyhow::Result<Option<Session>> {
        self.load(key).await
    }

    /// 列出磁盘上的所有会话（按 updated_at 降序）
    pub async fn list_sessions(&self) -> anyhow::Result<Vec<SessionInfo>> {
        let mut result = Vec::new();
        let mut rd = fs::read_dir(&self.sessions_dir).await?;

        while let Some(entry) = rd.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                continue;
            }

            let content = match fs::read_to_string(&path).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            let mut created_at = Utc::now();
            let mut updated_at = Utc::now();
            let mut message_count = 0usize;

            for (idx, line) in content.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
                    if idx == 0 && value.get("_type").and_then(|v| v.as_str()) == Some("metadata") {
                        if let Some(s) = value.get("created_at").and_then(|v| v.as_str()) {
                            if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
                                created_at = dt.with_timezone(&Utc);
                            }
                        }
                        if let Some(s) = value.get("updated_at").and_then(|v| v.as_str()) {
                            if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
                                updated_at = dt.with_timezone(&Utc);
                            }
                        }
                    } else if value.get("role").is_some() && value.get("content").is_some() {
                        message_count += 1;
                    }
                }
            }

            // best-effort key: file stem "_" -> ":"（与 Python 一致）
            let key = path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .replace('_', ":");

            result.push(SessionInfo { key, created_at, updated_at, path: path.clone(), message_count });
        }

        result.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(result)
    }

    /// 保存会话
    pub async fn save(&self, session: &Session) -> anyhow::Result<()> {
        let path = self.session_path(&session.key);

        let mut lines = Vec::new();

        // 写入元数据
        let metadata_line = serde_json::json!({
            "_type": "metadata",
            "created_at": session.created_at.to_rfc3339(),
            "updated_at": session.updated_at.to_rfc3339(),
            "metadata": session.metadata
        });
        lines.push(metadata_line.to_string());

        // 写入消息
        for msg in &session.messages {
            lines.push(serde_json::to_string(msg)?);
        }

        let content = lines.join("\n");
        tokio::fs::write(&path, content).await?;

        // 更新缓存
        {
            let mut cache = self.cache.write().await;
            cache.insert(session.key.clone(), session.clone());
        }

        Ok(())
    }

    /// 删除会话
    pub async fn delete(&self, key: &str) -> anyhow::Result<bool> {
        // 从缓存移除
        {
            let mut cache = self.cache.write().await;
            cache.remove(key);
        }

        // 删除文件
        let path = self.session_path(key);
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_manager() -> anyhow::Result<(SessionManager, TempDir)> {
        let temp_dir = tempfile::tempdir()?;
        let workspace = temp_dir.path().join("workspace");
        tokio::fs::create_dir_all(&workspace).await?;

        let sessions_dir = workspace.join("sessions");
        tokio::fs::create_dir_all(&sessions_dir).await?;

        let manager = SessionManager {
            sessions_dir,
            cache: Arc::new(RwLock::new(HashMap::new())),
        };

        Ok((manager, temp_dir))
    }

    #[tokio::test]
    async fn test_session_new() {
        let session = Session::new("test_key");
        assert_eq!(session.key, "test_key");
        assert!(session.messages.is_empty());
        assert!(session.metadata.is_empty());
    }

    #[tokio::test]
    async fn test_session_add_message() {
        let mut session = Session::new("test_key");
        session.add_message("user", "Hello!");
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].role, "user");
        assert_eq!(session.messages[0].content, "Hello!");
    }

    #[tokio::test]
    async fn test_session_get_history() {
        let mut session = Session::new("test_key");
        session.add_message("user", "Message 1");
        session.add_message("assistant", "Response 1");
        session.add_message("user", "Message 2");
        session.add_message("assistant", "Response 2");

        // 获取最后 2 条消息
        let history = session.get_history(2);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0]["role"], "user");
        assert_eq!(history[0]["content"], "Message 2");
    }

    #[tokio::test]
    async fn test_session_clear() {
        let mut session = Session::new("test_key");
        session.add_message("user", "Message 1");
        session.add_message("assistant", "Response 1");

        session.clear();
        assert!(session.messages.is_empty());
    }

    #[tokio::test]
    async fn test_manager_get_or_create() {
        let (manager, _temp_dir) = create_test_manager().await.unwrap();

        let session = manager.get_or_create("test_key").await.unwrap();
        assert_eq!(session.key, "test_key");
        assert!(session.messages.is_empty());
    }

    #[tokio::test]
    async fn test_manager_save_and_load() {
        let (manager, _temp_dir) = create_test_manager().await.unwrap();

        // 创建并保存会话
        let mut session = Session::new("test_key");
        session.add_message("user", "Hello!");
        session.add_message("assistant", "Hi there!");

        manager.save(&session).await.unwrap();

        // 重新加载
        let loaded = manager.get_or_create("test_key").await.unwrap();
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[0].content, "Hello!");
        assert_eq!(loaded.messages[1].content, "Hi there!");
    }

    #[tokio::test]
    async fn test_manager_delete() {
        let (manager, _temp_dir) = create_test_manager().await.unwrap();

        // 创建并保存会话
        let session = Session::new("test_key");
        manager.save(&session).await.unwrap();

        // 删除会话
        let deleted = manager.delete("test_key").await.unwrap();
        assert!(deleted);

        // 再次删除应该返回 false
        let deleted_again = manager.delete("test_key").await.unwrap();
        assert!(!deleted_again);
    }

    #[tokio::test]
    async fn test_manager_cache() {
        let (manager, _temp_dir) = create_test_manager().await.unwrap();

        // 创建会话
        let session1 = manager.get_or_create("test_key").await.unwrap();

        // 修改并保存
        let mut session1_mut = session1.clone();
        session1_mut.add_message("user", "Test");
        manager.save(&session1_mut).await.unwrap();

        // 从缓存获取（不重新加载）
        let session2 = manager.get_or_create("test_key").await.unwrap();

        // 对齐 Python：缓存应返回已保存版本
        assert_eq!(session2.messages.len(), 1);
    }

    #[tokio::test]
    async fn test_session_metadata() {
        let mut session = Session::new("test_key");
        session.metadata.insert("user_id".to_string(), "123".to_string());
        session.metadata.insert("channel".to_string(), "telegram".to_string());

        assert_eq!(session.metadata.get("user_id"), Some(&"123".to_string()));
        assert_eq!(session.metadata.len(), 2);
    }

    #[tokio::test]
    async fn test_session_path_safety() {
        let (manager, _temp_dir) = create_test_manager().await.unwrap();

        // 测试特殊字符被正确处理
        let path1 = manager.session_path("test:key");
        let path2 = manager.session_path("test/key");

        // 路径应该包含下划线而不是特殊字符
        assert!(path1.to_string_lossy().contains("test_key"));
        assert!(path2.to_string_lossy().contains("test_key"));
    }

    #[tokio::test]
    async fn test_multiple_sessions() {
        let (manager, _temp_dir) = create_test_manager().await.unwrap();

        // 创建多个会话
        for i in 0..5 {
            let mut session = Session::new(format!("key_{}", i));
            session.add_message("user", format!("Message {}", i));
            manager.save(&session).await.unwrap();
        }

        // 验证所有会话都能加载
        for i in 0..5 {
            let session = manager.get_or_create(&format!("key_{}", i)).await.unwrap();
            assert_eq!(session.messages.len(), 1);
            assert_eq!(session.messages[0].content, format!("Message {}", i));
        }
    }
}
