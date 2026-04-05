//! 测试辅助工具模块
//!
//! `MockLLMProvider` 在 `test-utils` feature 或单元测试下可用；夹具与 `TestEnvironment` 仅在 crate 自身 `cfg(test)` 下编译。

#[cfg(any(test, feature = "test-utils"))]
pub mod mock_provider;

#[cfg(any(test, feature = "test-utils"))]
pub use mock_provider::MockLLMProvider;

#[cfg(test)]
pub mod fixtures;

#[cfg(test)]
pub use fixtures::{
    create_test_config,
    create_test_message,
    create_test_workspace,
};

#[cfg(test)]
use std::path::PathBuf;
#[cfg(test)]
use tempfile::TempDir;

#[cfg(test)]
pub struct TestEnvironment {
    pub workspace: PathBuf,
    pub temp_dir: TempDir,
}

#[cfg(test)]
impl TestEnvironment {
    pub async fn new() -> anyhow::Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let workspace = temp_dir.path().join("workspace");
        tokio::fs::create_dir_all(&workspace).await?;
        Ok(Self { workspace, temp_dir })
    }

    pub async fn create_file(&self, path: &str, content: &str) -> anyhow::Result<()> {
        let file_path = self.workspace.join(path);
        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&file_path, content).await?;
        Ok(())
    }

    pub async fn read_file(&self, path: &str) -> anyhow::Result<String> {
        let file_path = self.workspace.join(path);
        Ok(tokio::fs::read_to_string(&file_path).await?)
    }
}

#[cfg(test)]
#[macro_export]
macro_rules! async_test {
    ($($name:ident $body:block)*) => {
        $(
            #[tokio::test]
            async fn $name() {
                $body
            }
        )*
    }
}
