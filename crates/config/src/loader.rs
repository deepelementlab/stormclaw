//! 配置加载和保存

use crate::schema::Config;
use anyhow::{Context, Result};
use regex::Regex;
use std::sync::OnceLock;
use stormclaw_utils::{config_dir, ensure_dir};
use std::path::PathBuf;

/// 获取配置文件路径
pub fn get_config_path() -> PathBuf {
    config_dir().join("config.json")
}

/// 加载配置
pub fn load_config() -> Result<Config> {
    let config_path = get_config_path();

    if !config_path.exists() {
        tracing::warn!("Config file not found, using defaults");
        return Ok(Config::default());
    }

    let content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read config from: {}", config_path.display()))?;

    // 支持环境变量替换
    let content = expand_env_vars(&content);

    let config: Config = serde_json::from_str(&content)
        .with_context(|| "Failed to parse config JSON")?;

    Ok(config)
}

/// 保存配置
pub fn save_config(config: &Config) -> Result<()> {
    let config_path = get_config_path();

    // 确保目录存在
    if let Some(parent) = config_path.parent() {
        ensure_dir(parent)?;
    }

    let content = serde_json::to_string_pretty(config)
        .context("Failed to serialize config")?;

    std::fs::write(&config_path, content)
        .with_context(|| format!("Failed to write config to: {}", config_path.display()))?;

    Ok(())
}

/// 获取数据目录
pub fn get_data_dir() -> PathBuf {
    let data_dir = config_dir().join("data");
    let _ = ensure_dir(&data_dir);
    data_dir
}

static RE_CONFIG_ENV: OnceLock<Regex> = OnceLock::new();

/// 展开 `${VAR}` 与 `$VAR`（C 风格标识符）；未定义则保留占位原文。
fn expand_env_vars(content: &str) -> String {
    let re = RE_CONFIG_ENV.get_or_init(|| {
        Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}|\$([A-Za-z_][A-Za-z0-9_]*)").unwrap()
    });
    re.replace_all(content, |caps: &regex::Captures| {
        let name = caps
            .get(1)
            .or_else(|| caps.get(2))
            .unwrap()
            .as_str();
        std::env::var(name).unwrap_or_else(|_| caps.get(0).unwrap().as_str().to_string())
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.agents.defaults.model, "anthropic/claude-opus-4-5");
        assert!(!config.channels.telegram.enabled);
    }

    #[test]
    fn test_get_config_path() {
        let path = get_config_path();
        assert!(path.ends_with("config.json"));
    }

    /// 临时改写 `HOME`；`#[serial]` 降低竞态。  
    /// Windows 上 `dirs::home_dir` 来自 Known Folder API，不随 `USERPROFILE` 覆盖变化，故仅在 Unix 断言。
    #[cfg(unix)]
    #[serial_test::serial]
    #[test]
    fn get_config_path_uses_isolated_home() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path();

        let saved = std::env::var("HOME").ok();
        std::env::set_var("HOME", home.as_os_str());

        let base = stormclaw_utils::config_dir();
        assert_eq!(base, home.join(".stormclaw"));
        assert_eq!(get_config_path(), base.join("config.json"));

        match &saved {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
    }

    #[test]
    fn expand_env_vars_braced_and_adjacent() {
        std::env::set_var("SC_CFG_A", "hello");
        std::env::set_var("SC_CFG_B", "world");
        let s = r#"{"x": "${SC_CFG_A}${SC_CFG_B}", "y": "$SC_CFG_A"}"#;
        let out = expand_env_vars(s);
        assert!(out.contains("helloworld"));
        assert!(out.contains("\"hello\""));
        std::env::remove_var("SC_CFG_A");
        std::env::remove_var("SC_CFG_B");
    }

    #[test]
    fn expand_env_vars_undefined_keeps_literal() {
        let s = "${SC_CFG_UNDEFINED_XYZ}";
        assert_eq!(expand_env_vars(s), s);
    }
}
