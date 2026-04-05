//! 工具注册表

use std::collections::HashMap;
use std::sync::Arc;
use serde_json::Value;
use super::base::Tool;
use crate::providers::ToolDefinition;

/// 工具注册表
///
/// 管理所有可用的工具
#[derive(Clone)]
pub struct ToolRegistry {
    tools: Arc<tokio::sync::RwLock<HashMap<String, Arc<dyn Tool>>>>,
}

impl ToolRegistry {
    /// 创建新的工具注册表
    pub fn new() -> Self {
        Self {
            tools: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }

    /// 注册工具
    pub async fn register(&self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        self.tools.write().await.insert(name, tool);
    }

    /// 获取工具
    pub async fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.read().await.get(name).cloned()
    }

    /// 检查工具是否存在
    pub async fn has(&self, name: &str) -> bool {
        self.tools.read().await.contains_key(name)
    }

    /// 获取所有工具定义
    pub async fn get_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.read()
            .await
            .values()
            .filter_map(|t| {
                let schema = t.to_schema();
                serde_json::from_value(schema).ok()
            })
            .collect()
    }

    /// 执行工具
    pub async fn execute(&self, name: &str, args: Value) -> anyhow::Result<String> {
        let tool = self.get(name)
            .await
            .ok_or_else(|| anyhow::anyhow!("Tool '{}' not found", name))?;

        tool.execute(args).await
    }

    /// 获取所有工具名称
    pub async fn tool_names(&self) -> Vec<String> {
        self.tools.read().await.keys().cloned().collect()
    }

    /// 获取工具数量
    pub async fn len(&self) -> usize {
        self.tools.read().await.len()
    }

    /// 检查是否为空
    pub async fn is_empty(&self) -> bool {
        self.tools.read().await.is_empty()
    }

    /// 清空已注册工具（配置热重载后重新 `register_default_tools`）
    pub async fn clear(&self) {
        self.tools.write().await.clear();
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // 创建一个简单的测试工具
    struct TestTool {
        name: &'static str,
    }

    impl TestTool {
        fn new(name: &'static str) -> Self {
            Self { name }
        }
    }

    #[async_trait::async_trait]
    impl Tool for TestTool {
        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            "A test tool"
        }

        fn parameters(&self) -> Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "input": {
                        "type": "string",
                        "description": "Input value"
                    }
                },
                "required": ["input"]
            })
        }

        async fn execute(&self, args: Value) -> anyhow::Result<String> {
            let input = args["input"].as_str().ok_or_else(|| anyhow::anyhow!("Missing input"))?;
            Ok(format!("Processed: {}", input))
        }

        fn to_schema(&self) -> Value {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": self.name,
                    "description": self.description(),
                    "parameters": self.parameters()
                }
            })
        }
    }

    #[tokio::test]
    async fn test_registry_creation() {
        let registry = ToolRegistry::new();
        assert!(registry.is_empty().await);
        assert_eq!(registry.len().await, 0);
    }

    #[tokio::test]
    async fn test_registry_default() {
        let registry = ToolRegistry::default();
        assert!(registry.is_empty().await);
    }

    #[tokio::test]
    async fn test_register_tool() {
        let registry = ToolRegistry::new();
        let tool = Arc::new(TestTool::new("test_tool"));

        registry.register(tool).await;

        assert!(!registry.is_empty().await);
        assert_eq!(registry.len().await, 1);
        assert!(registry.has("test_tool").await);
    }

    #[tokio::test]
    async fn test_get_tool() {
        let registry = ToolRegistry::new();
        let tool = Arc::new(TestTool::new("test_tool"));

        registry.register(tool.clone()).await;

        let retrieved = registry.get("test_tool").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name(), "test_tool");
    }

    #[tokio::test]
    async fn test_get_nonexistent_tool() {
        let registry = ToolRegistry::new();

        let retrieved = registry.get("nonexistent").await;
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_execute_tool() {
        let registry = ToolRegistry::new();
        let tool = Arc::new(TestTool::new("test_tool"));

        registry.register(tool).await;

        let args = serde_json::json!({"input": "test"});
        let result = registry.execute("test_tool", args).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Processed: test");
    }

    #[tokio::test]
    async fn test_execute_nonexistent_tool() {
        let registry = ToolRegistry::new();

        let args = serde_json::json!({"input": "test"});
        let result = registry.execute("nonexistent", args).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_definitions() {
        let registry = ToolRegistry::new();
        let tool1 = Arc::new(TestTool::new("tool1"));
        let tool2 = Arc::new(TestTool::new("tool2"));

        registry.register(tool1).await;
        registry.register(tool2).await;

        let definitions: Vec<_> = registry.get_definitions().await;
        assert_eq!(definitions.len(), 2);
    }

    #[tokio::test]
    async fn test_tool_names() {
        let registry = ToolRegistry::new();
        let tool1 = Arc::new(TestTool::new("tool1"));
        let tool2 = Arc::new(TestTool::new("tool2"));

        registry.register(tool1).await;
        registry.register(tool2).await;

        let names = registry.tool_names().await;
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"tool1".to_string()));
        assert!(names.contains(&"tool2".to_string()));
    }

    #[tokio::test]
    async fn test_multiple_registries() {
        let registry1 = ToolRegistry::new();
        let registry2 = ToolRegistry::new();

        let tool1 = Arc::new(TestTool::new("tool1"));
        let tool2 = Arc::new(TestTool::new("tool2"));

        registry1.register(tool1).await;
        registry2.register(tool2).await;

        assert_eq!(registry1.len().await, 1);
        assert_eq!(registry2.len().await, 1);

        assert!(registry1.has("tool1").await);
        assert!(registry2.has("tool2").await);

        assert!(!registry1.has("tool2").await);
        assert!(!registry2.has("tool1").await);
    }

    #[tokio::test]
    async fn test_registry_clone() {
        let registry1 = ToolRegistry::new();
        let tool = Arc::new(TestTool::new("test_tool"));

        registry1.register(tool).await;

        let registry2 = registry1.clone();

        assert_eq!(registry2.len().await, 1);
        assert!(registry2.has("test_tool").await);
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        let registry = Arc::new(ToolRegistry::new());
        let tool = Arc::new(TestTool::new("test_tool"));

        registry.register(tool).await;

        let mut handles = vec![];

        // 并发读取
        for _ in 0..10 {
            let registry_clone = registry.clone();
            handles.push(tokio::spawn(async move {
                registry_clone.has("test_tool").await
            }));
        }

        // 所有操作都应该成功
        for handle in handles {
            assert!(handle.await.unwrap());
        }
    }
}
