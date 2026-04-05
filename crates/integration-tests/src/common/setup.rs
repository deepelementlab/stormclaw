//! 测试环境设置

use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

/// 测试环境配置
pub struct TestEnv {
    pub workspace: PathBuf,
    pub temp_dir: TempDir,
    pub config_path: PathBuf,
}

impl TestEnv {
    pub async fn new() -> anyhow::Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let workspace = temp_dir.path().join("workspace");
        let config_path = temp_dir.path().join("config.json");

        tokio::fs::create_dir_all(&workspace).await?;
        tokio::fs::create_dir_all(workspace.join("sessions")).await?;
        tokio::fs::create_dir_all(workspace.join("memory")).await?;
        tokio::fs::create_dir_all(workspace.join("skills")).await?;

        let default_config = serde_json::json!({
            "agents": {
                "defaults": {
                    "model": "gpt-4",
                    "maxIterations": 5
                }
            },
            "providers": {
                "openrouter": {
                    "apiKey": "test-key"
                }
            },
            "workspace": workspace.to_string_lossy().to_string()
        });

        tokio::fs::write(
            &config_path,
            serde_json::to_string_pretty(&default_config)?,
        )
        .await?;

        Ok(Self {
            workspace,
            temp_dir,
            config_path,
        })
    }

    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    pub async fn create_file(&self, relative_path: &str, content: &str) -> anyhow::Result<()> {
        let file_path = self.workspace.join(relative_path);
        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&file_path, content).await?;
        Ok(())
    }

    pub async fn read_file(&self, relative_path: &str) -> anyhow::Result<String> {
        let file_path = self.workspace.join(relative_path);
        Ok(tokio::fs::read_to_string(&file_path).await?)
    }

    pub async fn file_exists(&self, relative_path: &str) -> bool {
        self.workspace.join(relative_path).exists()
    }
}

pub async fn cleanup_test_workspace(path: PathBuf) -> anyhow::Result<()> {
    if path.exists() {
        tokio::fs::remove_dir_all(path).await?;
    }
    Ok(())
}

pub async fn wait_for_condition<F, Fut>(condition: F, timeout_ms: u64) -> anyhow::Result<bool>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let result = timeout(Duration::from_millis(timeout_ms), condition()).await?;
    Ok(result)
}

pub async fn retry_async<F, Fut, T>(
    mut operation: F,
    max_retries: usize,
    delay_ms: u64,
) -> anyhow::Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    let mut last_error = None;

    for _ in 0..max_retries {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                last_error = Some(e);
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Max retries exceeded")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_test_env_creation() {
        let env = TestEnv::new().await.unwrap();
        assert!(env.workspace.exists());
        assert!(env.config_path.exists());
        assert!(env.workspace.join("sessions").exists());
        assert!(env.workspace.join("memory").exists());
    }

    #[tokio::test]
    async fn test_create_and_read_file() {
        let env = TestEnv::new().await.unwrap();
        env.create_file("test.txt", "Hello, world!").await.unwrap();
        assert!(env.file_exists("test.txt").await);

        let content = env.read_file("test.txt").await.unwrap();
        assert_eq!(content, "Hello, world!");
    }

    #[tokio::test]
    async fn test_wait_for_condition() {
        let result = wait_for_condition(|| async { true }, 100).await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_retry_async() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let attempts = std::sync::Arc::new(AtomicUsize::new(0));
        let a = attempts.clone();
        let result = retry_async(
            move || {
                let a = a.clone();
                async move {
                    let n = a.fetch_add(1, Ordering::SeqCst) + 1;
                    if n < 3 {
                        Err(anyhow::anyhow!("Not yet"))
                    } else {
                        Ok("success")
                    }
                }
            },
            5,
            10,
        )
        .await
        .unwrap();
        assert_eq!(result, "success");
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }
}
