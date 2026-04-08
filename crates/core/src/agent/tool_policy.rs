//! 内置工具分级（用于日志与策略扩展；执行路由由 `SecurityConfig.docker` 等控制）。
use stormclaw_config::{IngressMode, SecurityConfig, SessionPolicyMode, ToolPolicyConfig};

/// 工具风险/能力域分类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolCategory {
    /// 只读文件系统
    FilesystemRead,
    /// 写或修改工作区内文件
    FilesystemWrite,
    /// 出站网络
    Network,
    /// 任意 shell（宿主或 Docker 由 `exec` 实现决定）
    Shell,
    /// 消息、子代理等元能力
    Meta,
}

/// 工具风险等级，用于默认策略与审计
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

/// 按工具名返回分类；未知工具归为 `Meta`。
pub fn category_for_tool(name: &str) -> ToolCategory {
    match name {
        "read_file" | "list_dir" => ToolCategory::FilesystemRead,
        "write_file" | "edit_file" => ToolCategory::FilesystemWrite,
        "web_search" | "web_fetch" => ToolCategory::Network,
        "exec" => ToolCategory::Shell,
        "message" | "spawn" => ToolCategory::Meta,
        "wasm_eval_demo" => ToolCategory::Meta,
        _ => ToolCategory::Meta,
    }
}

/// 工具风险等级映射；未知工具默认高风险。
pub fn risk_for_tool(name: &str) -> RiskLevel {
    match category_for_tool(name) {
        ToolCategory::FilesystemRead => RiskLevel::Low,
        ToolCategory::FilesystemWrite => RiskLevel::Medium,
        ToolCategory::Network => RiskLevel::High,
        ToolCategory::Shell => RiskLevel::High,
        ToolCategory::Meta => {
            if name == "message" {
                RiskLevel::Low
            } else {
                RiskLevel::High
            }
        }
    }
}

/// 工具类别对应的配置键名（用于 `security.category_timeout_secs`）。
pub fn timeout_category_key(category: ToolCategory) -> &'static str {
    match category {
        ToolCategory::FilesystemRead => "filesystem_read",
        ToolCategory::FilesystemWrite => "filesystem_write",
        ToolCategory::Network => "network",
        ToolCategory::Shell => "shell",
        ToolCategory::Meta => "meta",
    }
}

/// 解析最终安全配置（全局 + channel 覆盖 + session 模式）。
pub fn resolve_effective_security(
    base: &SecurityConfig,
    channel: &str,
    session_mode: Option<SessionPolicyMode>,
) -> SecurityConfig {
    let mut sec = base.clone();
    if let Some(override_cfg) = base.channel_policies.get(channel) {
        if let Some(v) = override_cfg.ingress_mode {
            sec.ingress_mode = v;
        }
        if let Some(v) = override_cfg.validator_strict {
            sec.validator_strict = v;
        }
        if let Some(v) = override_cfg.default_tool_timeout_secs {
            sec.default_tool_timeout_secs = v;
        }
        sec.category_timeout_secs
            .extend(override_cfg.category_timeout_secs.clone());
        sec.tool_timeout_secs
            .extend(override_cfg.tool_timeout_secs.clone());
        sec.tool_policies.extend(override_cfg.tool_policies.clone());
    }
    match session_mode.unwrap_or(base.session_policy_mode) {
        SessionPolicyMode::Inherit => {}
        SessionPolicyMode::Baseline => {
            sec.ingress_mode = IngressMode::Warn;
            sec.validator_strict = false;
        }
        SessionPolicyMode::Strict => {
            sec.ingress_mode = IngressMode::Enforce;
            sec.validator_strict = true;
        }
    }
    sec
}

/// 合并全局/覆盖后得到工具策略（若无则返回默认）。
pub fn resolve_tool_policy(sec: &SecurityConfig, tool_name: &str) -> ToolPolicyConfig {
    sec.tool_policies
        .get(tool_name)
        .cloned()
        .unwrap_or_default()
}
