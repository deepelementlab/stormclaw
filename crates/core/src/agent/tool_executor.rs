//! 统一工具执行管线：参数校验、超时、日志脱敏、输出安全处理。

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use serde_json::{json, Value};
use stormclaw_config::SecurityConfig;
use stormclaw_safety::{SafetyConfig, SafetyLayer};

use super::tools::{Tool, ToolRegistry};

/// 单次工具调用上下文（审计 / 日志）
#[derive(Debug, Clone, Default)]
pub struct ToolInvocationContext {
    pub session_key: String,
    pub channel: String,
    pub subagent_id: Option<String>,
}

#[derive(Debug)]
struct ToolExecutorState {
    safety: Arc<SafetyLayer>,
    security: SecurityConfig,
}

/// 与 `AgentLoop` / `SubagentManager` 共享的执行器；支持热重载安全参数。
pub struct ToolExecutor {
    state: Arc<RwLock<ToolExecutorState>>,
}

impl ToolExecutor {
    pub fn new(security: SecurityConfig) -> Self {
        let safety = Arc::new(SafetyLayer::new(&safety_config_from_security(&security)));
        Self {
            state: Arc::new(RwLock::new(ToolExecutorState { safety, security })),
        }
    }

    /// 热重载安全策略（网关配置重载时调用）
    pub async fn refresh_security(&self, security: SecurityConfig) {
        let safety_cfg = safety_config_from_security(&security);
        let safety = Arc::new(SafetyLayer::new(&safety_cfg));
        let mut st = self.state.write().await;
        st.safety = safety;
        st.security = security;
    }

    pub async fn security_snapshot(&self) -> SecurityConfig {
        self.state.read().await.security.clone()
    }

    pub async fn scan_inbound_secrets(&self, input: &str) -> Option<String> {
        self.state.read().await.safety.scan_inbound_for_secrets(input)
    }

    pub async fn validate_user_text(&self, input: &str) -> stormclaw_safety::ValidationResult {
        self.state.read().await.safety.validate_input(input)
    }

    /// 执行工具：校验 → 超时 → 执行 → 输出处理
    pub async fn execute(
        &self,
        registry: &ToolRegistry,
        tool_name: &str,
        args: Value,
        ctx: &ToolInvocationContext,
    ) -> anyhow::Result<String> {
        let st = self.state.read().await;
        let security = &st.security;

        if security.validator_strict {
            let v = st.safety.validator().validate_tool_params(&args);
            if !v.is_valid {
                let details = v
                    .errors
                    .iter()
                    .map(|e| format!("{}: {}", e.field, e.message))
                    .collect::<Vec<_>>()
                    .join("; ");
                anyhow::bail!("Invalid tool parameters: {}", details);
            }
        }

        let redacted = redact_tool_args(registry, tool_name, &args).await;
        tracing::debug!(
            tool = %tool_name,
            session = %ctx.session_key,
            channel = %ctx.channel,
            subagent = ?ctx.subagent_id,
            params = %redacted,
            "tool call"
        );

        let tool = registry
            .get(tool_name)
            .await
            .ok_or_else(|| anyhow::anyhow!("Tool '{}' not found", tool_name))?;

        let timeout_secs = security
            .tool_timeout_secs
            .get(tool_name)
            .copied()
            .or_else(|| tool.execution_timeout().map(|d| d.as_secs()))
            .unwrap_or(security.default_tool_timeout_secs);

        let timeout = Duration::from_secs(timeout_secs.max(1));

        let exec_fut = registry.execute(tool_name, args);
        let raw = tokio::time::timeout(timeout, exec_fut)
            .await
            .map_err(|_| anyhow::anyhow!("tool '{}' timed out after {:?}", tool_name, timeout))??;

        drop(st);

        let st = self.state.read().await;
        if security.tool_output_sanitize {
            let sanitized = st.safety.sanitize_tool_output(tool_name, &raw);
            Ok(sanitized.content)
        } else {
            Ok(raw)
        }
    }
}

fn safety_config_from_security(sec: &SecurityConfig) -> SafetyConfig {
    SafetyConfig {
        max_output_length: sec.max_tool_output_bytes,
        injection_check_enabled: sec.tool_output_sanitize && sec.injection_check_on_tool_output,
    }
}

async fn redact_tool_args(registry: &ToolRegistry, tool_name: &str, args: &Value) -> String {
    let Some(tool) = registry.get(tool_name).await else {
        return args.to_string();
    };
    let keys = tool.sensitive_param_keys();
    if keys.is_empty() {
        return args.to_string();
    }
    redact_json_keys(args, keys)
}

fn redact_json_keys(v: &Value, keys: &[&str]) -> String {
    match v {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, val) in map {
                if keys.iter().any(|sk| sk == k) {
                    out.insert(k.clone(), json!("[REDACTED]"));
                } else {
                    out.insert(k.clone(), val.clone());
                }
            }
            Value::Object(out).to_string()
        }
        _ => v.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::base::Tool;
    use async_trait::async_trait;

    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "echo"
        }
        fn parameters(&self) -> Value {
            json!({})
        }
        fn sensitive_param_keys(&self) -> &'static [&'static str] {
            &["secret"]
        }
        async fn execute(&self, args: Value) -> anyhow::Result<String> {
            Ok(args.get("msg").and_then(|v| v.as_str()).unwrap_or("").to_string())
        }
    }

    #[tokio::test]
    async fn executor_runs_with_timeout() {
        let reg = ToolRegistry::new();
        reg.register(std::sync::Arc::new(EchoTool)).await;
        let mut sec = SecurityConfig::default();
        sec.default_tool_timeout_secs = 30;
        let ex = ToolExecutor::new(sec);
        let ctx = ToolInvocationContext::default();
        let out = ex
            .execute(
                &reg,
                "echo",
                json!({"msg": "hi", "secret": "x"}),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(out, "hi");
    }
}
