//! 日志服务 (Logging Service)
//!
//! 集中化的日志管理和日志流处理

use std::sync::Arc;
use std::io::{self, Write};
use tokio::sync::mpsc;
use tokio::fs::OpenOptions;
use chrono::{DateTime, Utc};
use tokio::io::AsyncWriteExt;

/// 日志级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Trace => "TRACE",
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "TRACE" => Some(LogLevel::Trace),
            "DEBUG" => Some(LogLevel::Debug),
            "INFO" => Some(LogLevel::Info),
            "WARN" | "WARNING" => Some(LogLevel::Warn),
            "ERROR" => Some(LogLevel::Error),
            _ => None,
        }
    }
}

/// 日志条目
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub target: String,
    pub message: String,
    pub metadata: LogMetadata,
}

/// 日志元数据
#[derive(Debug, Clone, Default)]
pub struct LogMetadata {
    pub service: Option<String>,
    pub request_id: Option<String>,
    pub user_id: Option<String>,
    pub channel: Option<String>,
    pub extra: std::collections::HashMap<String, String>,
}

/// 日志服务配置
#[derive(Debug, Clone)]
pub struct LoggingConfig {
    pub min_level: LogLevel,
    pub console_enabled: bool,
    pub file_enabled: bool,
    pub file_path: Option<String>,
    pub max_file_size: u64,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            min_level: LogLevel::Info,
            console_enabled: true,
            file_enabled: false,
            file_path: None,
            max_file_size: 100 * 1024 * 1024, // 100MB
        }
    }
}

/// 日志服务
pub struct LoggingService {
    config: LoggingConfig,
    sender: mpsc::UnboundedSender<LogEntry>,
    running: Arc<tokio::sync::RwLock<bool>>,
}

impl LoggingService {
    pub fn new(config: LoggingConfig) -> Self {
        let (sender, mut receiver) = mpsc::unbounded_channel();

        let service = Self {
            config,
            sender,
            running: Arc::new(tokio::sync::RwLock::new(false)),
        };

        // 启动日志处理任务
        let running = service.running.clone();
        let console_enabled = service.config.console_enabled;
        let file_enabled = service.config.file_enabled;
        let file_path = service.config.file_path.clone();
        let max_file_size = service.config.max_file_size;
        let min_level = service.config.min_level;

        tokio::spawn(async move {
            let mut current_file = None;
            let mut current_size = 0u64;

            while *running.read().await || !receiver.is_empty() {
                match receiver.recv().await {
                    Some(entry) => {
                        let level = entry.level;
                        if level < min_level {
                            continue;
                        }

                        let formatted = format_log_entry(&entry);

                        // 控制台输出
                        if console_enabled {
                            match level {
                                LogLevel::Error => eprintln!("{}", formatted),
                                LogLevel::Warn => eprintln!("{}", formatted),
                                _ => println!("{}", formatted),
                            }
                        }

                        // 文件输出
                        if file_enabled {
                            if let Some(path) = &file_path {
                                let file_size = formatted.len() as u64;

                                // 检查是否需要轮换文件
                                if current_size + file_size > max_file_size || current_file.is_none() {
                                    current_file = None; // 触发重新打开

                                    // 轮换文件
                                    let old_path = format!("{}.old", path);
                                    if std::path::Path::new(path).exists() {
                                        let _ = std::fs::rename(path, &old_path);
                                    }
                                }

                                // 打开文件
                                if current_file.is_none() {
                                    if let Ok(mut f) = OpenOptions::new()
                                        .create(true)
                                        .append(true)
                                        .open(path)
                                        .await
                                    {
                                        current_size = f.metadata().await.map(|m| m.len()).unwrap_or(0);
                                        current_file = Some(f);

                                        // 写入条目
                                        if let Some(ref mut f) = current_file {
                                            let _ = f.write_all(formatted.as_bytes()).await;
                                            let _ = f.write_all(b"\n").await;
                                            current_size += file_size + 1;
                                        }
                                    }
                                } else if let Some(ref mut f) = current_file {
                                    let _ = f.write_all(formatted.as_bytes()).await;
                                    let _ = f.write_all(b"\n").await;
                                    current_size += file_size + 1;
                                }
                            }
                        }
                    }
                    None => break,
                }
            }
        });

        service
    }

    /// 记录日志
    pub fn log(&self, level: LogLevel, target: String, message: String) {
        let entry = LogEntry {
            timestamp: Utc::now(),
            level,
            target,
            message,
            metadata: LogMetadata::default(),
        };

        let _ = self.sender.send(entry);
    }

    /// 创建子日志器
    pub fn logger(&self, target: String) -> ServiceLogger {
        ServiceLogger {
            sender: self.sender.clone(),
            target,
            metadata: LogMetadata::default(),
        }
    }

    /// 启动服务
    pub async fn start(&self) -> anyhow::Result<()> {
        *self.running.write().await = true;
        tracing::info!("Logging service started");
        Ok(())
    }

    /// 停止服务
    pub async fn stop(&self) -> anyhow::Result<()> {
        *self.running.write().await = false;
        tracing::info!("Logging service stopped");
        Ok(())
    }
}

/// 服务日志器
#[derive(Clone)]
pub struct ServiceLogger {
    sender: mpsc::UnboundedSender<LogEntry>,
    target: String,
    metadata: LogMetadata,
}

impl ServiceLogger {
    pub fn with_service(mut self, service: String) -> Self {
        self.metadata.service = Some(service);
        self
    }

    pub fn with_request(mut self, request_id: String) -> Self {
        self.metadata.request_id = Some(request_id);
        self
    }

    pub fn trace(&self, message: String) {
        self.log(LogLevel::Trace, message);
    }

    pub fn debug(&self, message: String) {
        self.log(LogLevel::Debug, message);
    }

    pub fn info(&self, message: String) {
        self.log(LogLevel::Info, message);
    }

    pub fn warn(&self, message: String) {
        self.log(LogLevel::Warn, message);
    }

    pub fn error(&self, message: String) {
        self.log(LogLevel::Error, message);
    }

    fn log(&self, level: LogLevel, message: String) {
        let _ = self.sender.send(LogEntry {
            timestamp: Utc::now(),
            level,
            target: self.target.clone(),
            message,
            metadata: self.metadata.clone(),
        });
    }
}

/// 格式化日志条目
fn format_log_entry(entry: &LogEntry) -> String {
    format!(
        "{} [{:5}] {}: {}",
        entry.timestamp.format("%Y-%m-%d %H:%M:%S%.3f"),
        entry.level.as_str(),
        entry.target,
        entry.message
    )
}
