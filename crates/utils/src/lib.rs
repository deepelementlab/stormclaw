//! stormclaw 工具函数库
//!
//! 提供文件系统、时间、路径等通用工具函数

use std::path::Path;
use anyhow::Result;

pub mod fs;
pub mod time;
pub mod path;
pub mod metrics;

// 重新导出常用函数
pub use path::{config_dir, data_dir, default_workspace, expand_env_vars_in_str, expand_path};
pub use fs::{read_file, write_file, append_file};

/// 确保目录存在，不存在则创建
pub fn ensure_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

/// 将文件名转换为安全格式
///
/// 将不安全的字符替换为下划线
pub fn safe_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            ':' | '/' | '\\' | '<' | '>' | '|' | '"' | '?' | '*' => '_',
            _ => c
        })
        .collect()
}

/// 获取当前日期字符串 (YYYY-MM-DD 格式)
pub fn today_date() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

/// 获取当前时间戳（毫秒）
pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_filename() {
        assert_eq!(safe_filename("test:file"), "test_file");
        assert_eq!(safe_filename("path/to/file"), "path_to_file");
        assert_eq!(safe_filename("normal.txt"), "normal.txt");
    }

    #[test]
    fn test_today_date() {
        let date = today_date();
        assert!(date.len() == 10); // YYYY-MM-DD
        assert!(date.contains('-'));
    }
}
