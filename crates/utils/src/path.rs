//! 路径处理工具函数

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use anyhow::Result;
use regex::Regex;

static RE_ENV_BRACED: OnceLock<Regex> = OnceLock::new();
static RE_ENV_DOLLAR: OnceLock<Regex> = OnceLock::new();
static RE_ENV_PERCENT: OnceLock<Regex> = OnceLock::new();

/// 展开 `${VAR}`、`$VAR`（标识符）与 `%VAR%`（Windows 风格）；未定义则保留原文。
pub fn expand_env_vars_in_str(s: &str) -> String {
    let mut result = s.to_string();

    let re1 = RE_ENV_BRACED.get_or_init(|| Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").unwrap());
    result = re1
        .replace_all(&result, |caps: &regex::Captures| {
            let name = caps.get(1).unwrap().as_str();
            std::env::var(name).unwrap_or_else(|_| caps.get(0).unwrap().as_str().to_string())
        })
        .to_string();

    let re2 = RE_ENV_DOLLAR.get_or_init(|| Regex::new(r"\$([A-Za-z_][A-Za-z0-9_]*)").unwrap());
    result = re2
        .replace_all(&result, |caps: &regex::Captures| {
            let name = caps.get(1).unwrap().as_str();
            std::env::var(name).unwrap_or_else(|_| caps.get(0).unwrap().as_str().to_string())
        })
        .to_string();

    let re3 = RE_ENV_PERCENT.get_or_init(|| Regex::new(r"%([A-Za-z_][A-Za-z0-9_]*)%").unwrap());
    result = re3
        .replace_all(&result, |caps: &regex::Captures| {
            let name = caps.get(1).unwrap().as_str();
            std::env::var(name).unwrap_or_else(|_| caps.get(0).unwrap().as_str().to_string())
        })
        .to_string();

    result
}

/// 获取用户主目录
pub fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

/// 获取 stormclaw 配置目录
pub fn config_dir() -> PathBuf {
    home_dir()
        .map(|p| p.join(".stormclaw"))
        .unwrap_or_else(|| PathBuf::from(".stormclaw"))
}

/// 获取 stormclaw 数据目录
pub fn data_dir() -> PathBuf {
    config_dir().join("data")
}

/// 获取默认工作区路径
pub fn default_workspace() -> PathBuf {
    config_dir().join("workspace")
}

/// 展开路径中的 ~ 和环境变量
pub fn expand_path(path: &str) -> PathBuf {
    let path = path.trim();
    if path.starts_with('~') {
        if let Some(home) = home_dir() {
            // `~/foo` → home + `foo`；避免 `~/Documents` 在 Windows 上因前导 `/` 被当作绝对路径根
            let tail = path[1..].trim_start_matches(['/', '\\']);
            let tail = expand_env_vars_in_str(tail);
            return if tail.is_empty() {
                home
            } else {
                home.join(tail)
            };
        }
    }
    PathBuf::from(expand_env_vars_in_str(path))
}

/// 确保路径是绝对路径
pub fn ensure_absolute(path: &Path, base: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

/// 获取相对路径
pub fn relative_to(path: &Path, base: &Path) -> Result<PathBuf> {
    path.strip_prefix(base)
        .map(|p| p.to_path_buf())
        .map_err(|_| anyhow::anyhow!("Path is not relative to base"))
}

/// 规范化路径（解析 .. 和 .）
pub fn canonicalize(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .map_err(|e| anyhow::anyhow!("Failed to canonicalize path: {}", e))
}

/// 检查命令是否可用
pub fn command_available(cmd: &str) -> bool {
    which::which(cmd).is_ok()
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn expand_path_plain_relative() {
        let p = expand_path("relative/sub");
        assert_eq!(p, PathBuf::from("relative/sub"));
    }

    #[test]
    fn expand_path_tilde_prefix_uses_home_when_set() {
        if let Some(home) = home_dir() {
            let p = expand_path("~/Documents");
            assert_eq!(p, home.join("Documents"));
        }
    }

    #[test]
    fn expand_path_env_dollar_braced() {
        std::env::set_var("SC_TEST_PATH_VAR", "mydir");
        let p = expand_path("${SC_TEST_PATH_VAR}/sub");
        assert_eq!(p, PathBuf::from("mydir/sub"));
        std::env::remove_var("SC_TEST_PATH_VAR");
    }

    #[test]
    fn expand_env_vars_in_str_percent_form() {
        std::env::set_var("SC_TEST_PCT", "X");
        assert_eq!(expand_env_vars_in_str("pre%SC_TEST_PCT%post"), "preXpost");
        std::env::remove_var("SC_TEST_PCT");
    }
}
