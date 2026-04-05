//! Tool 抽象接口

use async_trait::async_trait;
use serde_json::Value;
use std::time::Duration;

/// Tool 抽象接口
///
/// 所有工具都需要实现此 trait
#[async_trait]
pub trait Tool: Send + Sync {
    /// 获取工具名称
    fn name(&self) -> &str;

    /// 获取工具描述
    fn description(&self) -> &str;

    /// 获取工具参数 JSON Schema
    fn parameters(&self) -> Value;

    /// 执行工具
    async fn execute(&self, args: Value) -> anyhow::Result<String>;

    /// 单次调用超时；`None` 表示使用全局默认
    fn execution_timeout(&self) -> Option<Duration> {
        None
    }

    /// 日志脱敏：这些 JSON 键在记录参数时替换为 `[REDACTED]`
    fn sensitive_param_keys(&self) -> &'static [&'static str] {
        &[]
    }

    /// 转换为 OpenAI 函数调用格式
    fn to_schema(&self) -> Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": self.name(),
                "description": self.description(),
                "parameters": self.parameters()
            }
        })
    }
}
