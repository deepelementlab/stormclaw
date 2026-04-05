//! 配置结构定义

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// stormclaw 根配置
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default)]
    pub agents: AgentsConfig,
    #[serde(default)]
    pub channels: ChannelsConfig,
    #[serde(default)]
    pub providers: ProvidersConfig,
    #[serde(default)]
    pub gateway: GatewayConfig,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub security: SecurityConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            agents: AgentsConfig::default(),
            channels: ChannelsConfig::default(),
            providers: ProvidersConfig::default(),
            gateway: GatewayConfig::default(),
            tools: ToolsConfig::default(),
            security: SecurityConfig::default(),
        }
    }
}

impl Config {
    /// 获取 API 密钥（按优先级）
    pub fn get_api_key(&self) -> Option<String> {
        self.providers.openrouter.api_key.clone()
            .or_else(|| self.providers.anthropic.api_key.clone())
            .or_else(|| self.providers.openai.api_key.clone())
    }

    /// 获取 API 基础 URL
    pub fn get_api_base(&self) -> Option<String> {
        if self.providers.openrouter.api_key.is_some() {
            Some(self.providers.openrouter.api_base.clone()
                .unwrap_or_else(|| "https://openrouter.ai/api/v1".to_string()))
        } else {
            None
        }
    }

    /// 获取工作区路径
    pub fn workspace_path(&self) -> PathBuf {
        if let Some(workspace) = &self.agents.defaults.workspace {
            stormclaw_utils::expand_path(workspace)
        } else {
            stormclaw_utils::default_workspace()
        }
    }
}

/// Agent 配置
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentsConfig {
    #[serde(default)]
    pub defaults: AgentDefaults,
}

impl Default for AgentsConfig {
    fn default() -> Self {
        Self {
            defaults: AgentDefaults::default(),
        }
    }
}

/// Agent 默认配置
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDefaults {
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default = "default_max_tool_iterations")]
    pub max_tool_iterations: usize,
}

impl Default for AgentDefaults {
    fn default() -> Self {
        Self {
            workspace: None,
            model: default_model(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            max_tool_iterations: default_max_tool_iterations(),
        }
    }
}

fn default_model() -> String {
    "anthropic/claude-opus-4-5".to_string()
}

fn default_max_tokens() -> usize {
    8192
}

fn default_temperature() -> f64 {
    0.7
}

fn default_max_tool_iterations() -> usize {
    20
}

/// 渠道配置
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ChannelsConfig {
    pub telegram: TelegramConfig,
    pub whatsapp: WhatsAppConfig,
}

impl Default for ChannelsConfig {
    fn default() -> Self {
        Self {
            telegram: TelegramConfig::default(),
            whatsapp: WhatsAppConfig::default(),
        }
    }
}

/// Telegram 渠道配置
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelegramConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub allow_from: Vec<String>,
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            token: String::new(),
            allow_from: Vec::new(),
        }
    }
}

/// WhatsApp 渠道配置
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhatsAppConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_bridge_url")]
    pub bridge_url: String,
    #[serde(default)]
    pub allow_from: Vec<String>,
}

impl Default for WhatsAppConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bridge_url: default_bridge_url(),
            allow_from: Vec::new(),
        }
    }
}

fn default_bridge_url() -> String {
    "ws://localhost:3001".to_string()
}

/// LLM 提供商配置
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ProvidersConfig {
    pub anthropic: ProviderConfig,
    pub openai: ProviderConfig,
    pub openrouter: ProviderConfig,
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            anthropic: ProviderConfig::default(),
            openai: ProviderConfig::default(),
            openrouter: ProviderConfig::default(),
        }
    }
}

/// 提供商配置
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_base: Option<String>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            api_base: None,
        }
    }
}

/// 网关配置
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct GatewayConfig {
    #[serde(default = "default_gateway_host")]
    pub host: String,
    #[serde(default = "default_gateway_port")]
    pub port: u16,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            host: default_gateway_host(),
            port: default_gateway_port(),
        }
    }
}

fn default_gateway_host() -> String {
    "0.0.0.0".to_string()
}

fn default_gateway_port() -> u16 {
    18789
}

/// 工具配置
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ToolsConfig {
    pub web: WebToolsConfig,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            web: WebToolsConfig::default(),
        }
    }
}

/// Web 工具配置
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct WebToolsConfig {
    pub search: WebSearchConfig,
}

impl Default for WebToolsConfig {
    fn default() -> Self {
        Self {
            search: WebSearchConfig::default(),
        }
    }
}

/// Web 搜索配置
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchConfig {
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_max_results")]
    pub max_results: usize,
}

impl Default for WebSearchConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            max_results: default_max_results(),
        }
    }
}

fn default_max_results() -> usize {
    5
}

/// 用户消息入站安全策略
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IngressMode {
    /// 不做入站校验（兼容旧行为）
    #[default]
    Off,
    /// 校验失败或疑似密钥时记录日志，仍送入 LLM
    Warn,
    /// 校验失败或疑似密钥时拒绝处理该条消息
    Enforce,
}

/// Docker 隔离执行 `exec` 的配置
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerSecurityConfig {
    /// 为 true 时优先在容器内执行 shell（不可用时按 fallback 策略）
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_docker_image")]
    pub image: String,
    /// 是否将工作区挂载为可写（默认只读，更安全）
    #[serde(default)]
    pub workspace_writable: bool,
    #[serde(default = "default_docker_memory_mb")]
    pub memory_mb: u32,
    /// 使用 `--network none` 隔离出站
    #[serde(default = "default_true")]
    pub network_isolated: bool,
    /// Docker 不可用且 `enabled` 时是否回退到宿主执行（默认 false：拒绝执行）
    #[serde(default)]
    pub fallback_to_host: bool,
}

fn default_docker_image() -> String {
    "alpine:3.20".to_string()
}

fn default_docker_memory_mb() -> u32 {
    512
}

impl Default for DockerSecurityConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            image: default_docker_image(),
            workspace_writable: false,
            memory_mb: default_docker_memory_mb(),
            network_isolated: true,
            fallback_to_host: false,
        }
    }
}

/// 纵深安全与工具策略（对齐 IronClaw safety 管线思路）
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityConfig {
    #[serde(default)]
    pub ingress_mode: IngressMode,
    /// 工具返回写入 LLM 前是否走 SafetyLayer（泄露扫描、策略、可选注入清理）
    #[serde(default = "default_true")]
    pub tool_output_sanitize: bool,
    #[serde(default = "default_max_tool_output_bytes")]
    pub max_tool_output_bytes: usize,
    /// 工具输出是否启用注入类清理（依赖 stormclaw_safety sanitizer）
    #[serde(default = "default_true")]
    pub injection_check_on_tool_output: bool,
    /// 是否对工具参数做递归字符串校验（过严可能误杀，默认关闭）
    #[serde(default)]
    pub validator_strict: bool,
    #[serde(default = "default_tool_timeout_secs")]
    pub default_tool_timeout_secs: u64,
    /// 按工具名覆盖超时（秒）
    #[serde(default)]
    pub tool_timeout_secs: HashMap<String, u64>,
    #[serde(default)]
    pub docker: DockerSecurityConfig,
    /// 为 true 且编译启用 `wasm-tools` 特性时注册 WASM 演示工具
    #[serde(default)]
    pub wasm_tools_enabled: bool,
}

fn default_true() -> bool {
    true
}

fn default_max_tool_output_bytes() -> usize {
    100_000
}

fn default_tool_timeout_secs() -> u64 {
    120
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            ingress_mode: IngressMode::Off,
            tool_output_sanitize: true,
            max_tool_output_bytes: default_max_tool_output_bytes(),
            injection_check_on_tool_output: true,
            validator_strict: false,
            default_tool_timeout_secs: default_tool_timeout_secs(),
            tool_timeout_secs: HashMap::new(),
            docker: DockerSecurityConfig::default(),
            wasm_tools_enabled: false,
        }
    }
}

#[cfg(test)]
mod schema_tests {
    use super::*;

    #[test]
    fn minimal_config_json_camel_case() {
        let j = r#"{"agents":{"defaults":{"model":"openrouter/test"}}}"#;
        let c: Config = serde_json::from_str(j).expect("parse");
        assert_eq!(c.agents.defaults.model, "openrouter/test");
    }

    #[test]
    fn config_default_serializes_key_fields() {
        let c = Config::default();
        let v = serde_json::to_value(&c).unwrap();
        assert!(v.get("agents").is_some());
        assert!(v.get("providers").is_some());
    }
}
