//! 时间相关工具函数

use chrono::{DateTime, Utc, Timelike, Datelike};
use std::fmt;

/// 时间格式化器
pub struct TimeFormatter;

impl TimeFormatter {
    /// 格式化为 ISO 8601 字符串
    pub fn iso8601(dt: &DateTime<Utc>) -> String {
        dt.to_rfc3339()
    }

    /// 格式化为可读字符串
    pub fn readable(dt: &DateTime<Utc>) -> String {
        dt.format("%Y-%m-%d %H:%M:%S UTC").to_string()
    }

    /// 获取今天的日期字符串 (YYYY-MM-DD)
    pub fn today() -> String {
        Utc::now().format("%Y-%m-%d").to_string()
    }

    /// 获取当前时间戳（秒）
    pub fn timestamp() -> i64 {
        Utc::now().timestamp()
    }

    /// 获取当前时间戳（毫秒）
    pub fn timestamp_millis() -> i64 {
        Utc::now().timestamp_millis()
    }

    /// 从时间戳创建 DateTime
    pub fn from_timestamp(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(secs, 0).unwrap_or_default()
    }

    /// 从毫秒时间戳创建 DateTime
    pub fn from_timestamp_millis(millis: i64) -> DateTime<Utc> {
        DateTime::from_timestamp_millis(millis).unwrap_or_default()
    }
}

/// 计算两个时间之间的持续时间
pub fn duration_between(start: &DateTime<Utc>, end: &DateTime<Utc>) -> chrono::Duration {
    *end - *start
}

/// 格式化持续时间为人类可读格式
pub fn format_duration(duration: chrono::Duration) -> String {
    let secs = duration.num_seconds();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    }
}
