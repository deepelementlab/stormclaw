//! 统一工具执行管线：参数校验、超时、日志脱敏、输出安全处理。

use std::sync::Arc;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use serde_json::{json, Value};
use stormclaw_config::{SecurityConfig, SessionPolicyMode, ToolPolicyConfig};
use stormclaw_safety::{SafetyConfig, SafetyLayer};

use super::tool_policy::{
    category_for_tool, resolve_effective_security, resolve_tool_policy, risk_for_tool,
    timeout_category_key, RiskLevel, ToolCategory,
};
use super::tools::ToolRegistry;

/// 单次工具调用上下文（审计 / 日志）
#[derive(Debug, Clone, Default)]
pub struct ToolInvocationContext {
    pub session_key: String,
    pub channel: String,
    pub subagent_id: Option<String>,
    pub session_policy_mode: Option<SessionPolicyMode>,
}

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
        let started = Instant::now();
        let (safety, base_security) = {
            let st = self.state.read().await;
            (st.safety.clone(), st.security.clone())
        };
        let security = resolve_effective_security(&base_security, &ctx.channel, ctx.session_policy_mode);
        let category = category_for_tool(tool_name);
        let has_tool = registry.has(tool_name).await;
        let policy = resolve_tool_policy(&security, tool_name);

        if let Err(err) = preflight_check(tool_name, category, has_tool, &security, &policy) {
            let reason = err.to_string();
            emit_audit_log(
                &security,
                ctx,
                tool_name,
                category,
                "policy_blocked",
                started.elapsed().as_millis(),
                0,
                Some(&reason),
            );
            return Err(err);
        }

        if security.validator_strict {
            let v = safety.validator().validate_tool_params(&args);
            if !v.is_valid {
                let details = v
                    .errors
                    .iter()
                    .map(|e| format!("{}: {}", e.field, e.message))
                    .collect::<Vec<_>>()
                    .join("; ");
                let err = policy_error("ValidationFailed", format!("Invalid tool parameters: {}", details));
                emit_audit_log(
                    &security,
                    ctx,
                    tool_name,
                    category,
                    "validation_failed",
                    started.elapsed().as_millis(),
                    0,
                    Some(&err.to_string()),
                );
                return Err(err);
            }
        }

        if let Some(max_args_bytes) = policy.max_args_bytes {
            let args_bytes = serde_json::to_vec(&args)
                .map(|v| v.len())
                .unwrap_or_else(|_| args.to_string().len());
            if args_bytes > max_args_bytes {
                let err = policy_error(
                    "ToolBlockedByPolicy",
                    format!(
                        "tool '{}' arguments exceed maxArgsBytes: {} > {}",
                        tool_name, args_bytes, max_args_bytes
                    ),
                );
                emit_audit_log(
                    &security,
                    ctx,
                    tool_name,
                    category,
                    "policy_blocked",
                    started.elapsed().as_millis(),
                    0,
                    Some(&err.to_string()),
                );
                return Err(err);
            }
        }
        if !policy.deny_patterns.is_empty() {
            let args_s = args.to_string();
            if let Some(hit) = policy.deny_patterns.iter().find(|p| !p.is_empty() && args_s.contains(p.as_str())) {
                let err = policy_error(
                    "ToolBlockedByPolicy",
                    format!("tool '{}' arguments matched deny pattern '{}'", tool_name, hit),
                );
                emit_audit_log(
                    &security,
                    ctx,
                    tool_name,
                    category,
                    "policy_blocked",
                    started.elapsed().as_millis(),
                    0,
                    Some(&err.to_string()),
                );
                return Err(err);
            }
        }

        let redacted = redact_tool_args(registry, tool_name, &args).await;
        tracing::debug!(
            tool = %tool_name,
            category = ?category,
            risk = ?risk_for_tool(tool_name),
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
            .or_else(|| {
                let key = timeout_category_key(category);
                security.category_timeout_secs.get(key).copied()
            })
            .or_else(|| tool.execution_timeout().map(|d| d.as_secs()))
            .unwrap_or(security.default_tool_timeout_secs);

        let timeout = Duration::from_secs(timeout_secs.max(1));

        let exec_fut = registry.execute(tool_name, args);
        let raw = match tokio::time::timeout(timeout, exec_fut).await {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                let err = policy_error("ToolRuntimeError", e.to_string());
                emit_audit_log(
                    &security,
                    ctx,
                    tool_name,
                    category,
                    "tool_runtime_error",
                    started.elapsed().as_millis(),
                    0,
                    Some(&err.to_string()),
                );
                return Err(err);
            }
            Err(_) => {
                let err = policy_error("ToolTimeout", format!("tool '{}' timed out after {:?}", tool_name, timeout));
                emit_audit_log(
                    &security,
                    ctx,
                    tool_name,
                    category,
                    "timeout",
                    started.elapsed().as_millis(),
                    0,
                    Some(&err.to_string()),
                );
                return Err(err);
            }
        };

        let max_output = policy
            .max_output_bytes
            .unwrap_or(security.max_tool_output_bytes);
        let raw_truncated = truncate_by_bytes(&raw, max_output);

        let final_output = if security.tool_output_sanitize {
            let sanitized = safety.sanitize_tool_output(tool_name, &raw_truncated);
            sanitized.content
        } else {
            raw_truncated
        };

        emit_audit_log(
            &security,
            ctx,
            tool_name,
            category,
            "success",
            started.elapsed().as_millis(),
            raw.len().saturating_sub(final_output.len()),
            None,
        );

        Ok(final_output)
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

fn preflight_check(
    tool_name: &str,
    category: ToolCategory,
    has_tool: bool,
    security: &SecurityConfig,
    policy: &ToolPolicyConfig,
) -> anyhow::Result<()> {
    if !has_tool {
        return Err(policy_error(
            "ToolNotRegistered",
            format!("tool '{}' is not registered in current runtime", tool_name),
        ));
    }
    if !policy.enabled {
        return Err(policy_error(
            "ToolBlockedByPolicy",
            format!("tool '{}' is disabled by security.toolPolicies", tool_name),
        ));
    }
    if policy.require_docker && !security.docker.enabled {
        return Err(policy_error(
            "ToolRequiresIsolation",
            format!("tool '{}' requires Docker isolation", tool_name),
        ));
    }
    if !policy.allow_network && category == ToolCategory::Network {
        return Err(policy_error(
            "ToolBlockedByPolicy",
            format!("network tool '{}' is disabled by tool policy", tool_name),
        ));
    }
    // 未知工具在严格模式下默认拒绝，平衡兼容性与收敛风险。
    if security.validator_strict && risk_for_tool(tool_name) == RiskLevel::High && category == ToolCategory::Meta {
        return Err(policy_error(
            "ToolBlockedByPolicy",
            format!("meta/high-risk tool '{}' is blocked in strict mode", tool_name),
        ));
    }

    Ok(())
}

fn policy_error(code: &str, message: String) -> anyhow::Error {
    anyhow::anyhow!("{}: {}", code, message)
}

fn truncate_by_bytes(input: &str, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input.to_string();
    }
    let mut cut = max_bytes.min(input.len());
    while !input.is_char_boundary(cut) && cut > 0 {
        cut -= 1;
    }
    input[..cut].to_string()
}

fn emit_audit_log(
    security: &SecurityConfig,
    ctx: &ToolInvocationContext,
    tool_name: &str,
    category: ToolCategory,
    decision: &str,
    latency_ms: u128,
    sanitized_bytes: usize,
    reason: Option<&str>,
) {
    if !security.security_audit.enabled {
        return;
    }
    if !should_sample(&ctx.session_key, tool_name, security.security_audit.sample_rate) {
        return;
    }
    tracing::info!(
        target: "stormclaw_security_audit",
        session_key = %ctx.session_key,
        channel = %ctx.channel,
        subagent = ?ctx.subagent_id,
        tool = %tool_name,
        category = ?category,
        decision = %decision,
        latency_ms = latency_ms,
        sanitized_bytes = sanitized_bytes,
        reason = %reason.unwrap_or(""),
        "security audit"
    );
    record_security_metric(decision, tool_name, category, latency_ms as u64, sanitized_bytes as u64);
}

fn should_sample(session_key: &str, tool_name: &str, sample_rate: f64) -> bool {
    if sample_rate >= 1.0 {
        return true;
    }
    if sample_rate <= 0.0 {
        return false;
    }
    let mut hash: u64 = 1469598103934665603;
    for b in session_key.as_bytes().iter().chain(tool_name.as_bytes()) {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    let bucket = (hash % 10_000) as f64 / 10_000.0;
    bucket < sample_rate
}

fn record_security_metric(
    decision: &str,
    tool_name: &str,
    category: ToolCategory,
    latency_ms: u64,
    sanitized_bytes: u64,
) {
    static COUNTERS: OnceLock<Mutex<std::collections::HashMap<String, u64>>> = OnceLock::new();
    let counters = COUNTERS.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    if let Ok(mut g) = counters.lock() {
        *g.entry(format!(
            "security_tool_decision_total|decision={}|tool={}|category={:?}",
            decision, tool_name, category
        ))
        .or_insert(0) += 1;
        *g.entry(format!(
            "security_tool_sanitized_bytes_total|tool={}",
            tool_name
        ))
        .or_insert(0) += sanitized_bytes;
        *g.entry(format!(
            "security_tool_latency_ms_total|tool={}|category={:?}",
            tool_name, category
        ))
        .or_insert(0) += latency_ms;
    }
    tracing::debug!(
        target: "stormclaw_security_metric",
        decision = %decision,
        tool = %tool_name,
        category = ?category,
        latency_ms = latency_ms,
        sanitized_bytes = sanitized_bytes,
        "security metric"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::base::Tool;
    use async_trait::async_trait;
    use tokio::time::Duration as TokioDuration;

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

    #[tokio::test]
    async fn executor_blocks_disabled_tool_by_policy() {
        let reg = ToolRegistry::new();
        reg.register(std::sync::Arc::new(EchoTool)).await;
        let mut sec = SecurityConfig::default();
        sec.tool_policies
            .insert("echo".to_string(), stormclaw_config::ToolPolicyConfig {
                enabled: false,
                ..Default::default()
            });
        let ex = ToolExecutor::new(sec);
        let err = ex
            .execute(&reg, "echo", json!({"msg":"x"}), &ToolInvocationContext::default())
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("ToolBlockedByPolicy"));
    }

    struct SleepTool;

    #[async_trait]
    impl Tool for SleepTool {
        fn name(&self) -> &str {
            "sleep_tool"
        }
        fn description(&self) -> &str {
            "sleep"
        }
        fn parameters(&self) -> Value {
            json!({})
        }
        async fn execute(&self, _args: Value) -> anyhow::Result<String> {
            tokio::time::sleep(TokioDuration::from_secs(2)).await;
            Ok("done".to_string())
        }
    }

    #[tokio::test]
    async fn executor_applies_category_timeout() {
        let reg = ToolRegistry::new();
        reg.register(std::sync::Arc::new(SleepTool)).await;
        let mut sec = SecurityConfig::default();
        sec.default_tool_timeout_secs = 10;
        sec.category_timeout_secs.insert("meta".to_string(), 1);
        let ex = ToolExecutor::new(sec);
        let err = ex
            .execute(&reg, "sleep_tool", json!({}), &ToolInvocationContext::default())
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("ToolTimeout"));
    }

    #[tokio::test]
    async fn executor_blocks_by_deny_pattern() {
        let reg = ToolRegistry::new();
        reg.register(std::sync::Arc::new(EchoTool)).await;
        let mut sec = SecurityConfig::default();
        sec.tool_policies.insert(
            "echo".to_string(),
            stormclaw_config::ToolPolicyConfig {
                deny_patterns: vec!["DROP TABLE".to_string()],
                ..Default::default()
            },
        );
        let ex = ToolExecutor::new(sec);
        let err = ex
            .execute(
                &reg,
                "echo",
                json!({"msg":"DROP TABLE users"}),
                &ToolInvocationContext::default(),
            )
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("ToolBlockedByPolicy"));
    }

    #[tokio::test]
    async fn executor_applies_channel_override() {
        let reg = ToolRegistry::new();
        reg.register(std::sync::Arc::new(SleepTool)).await;
        let mut sec = SecurityConfig::default();
        sec.default_tool_timeout_secs = 10;
        let mut override_cfg = stormclaw_config::SecurityPolicyOverride::default();
        override_cfg.category_timeout_secs.insert("meta".to_string(), 1);
        sec.channel_policies.insert("telegram".to_string(), override_cfg);
        let ex = ToolExecutor::new(sec);
        let ctx = ToolInvocationContext {
            channel: "telegram".to_string(),
            ..Default::default()
        };
        let err = ex
            .execute(&reg, "sleep_tool", json!({}), &ctx)
            .await;
        assert!(err.unwrap_err().to_string().contains("ToolTimeout"));
    }

    #[test]
    fn truncate_keeps_utf8_boundary() {
        let s = "你好abc";
        let out = truncate_by_bytes(s, 4);
        assert_eq!(out, "你");
    }
}
