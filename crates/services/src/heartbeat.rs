//! 心跳服务 (Heartbeat Service)
//!
//! 定期唤醒 Agent 检查待处理任务

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::future::Future;
use std::pin::Pin;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

const HEARTBEAT_PROMPT: &str = "Read HEARTBEAT.md in your workspace (if it exists).\nFollow any instructions or tasks listed there.\nIf nothing needs attention, reply with just: HEARTBEAT_OK";

/// 心跳服务配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatConfig {
    /// 心跳间隔（秒）
    #[serde(default = "default_interval")]
    pub interval_seconds: u64,

    /// 是否启用
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// 心跳文件路径（相对于工作区）
    #[serde(default = "default_heartbeat_file")]
    pub heartbeat_file: String,
}

fn default_interval() -> u64 {
    30 * 60 // 30 分钟
}

fn default_enabled() -> bool {
    true
}

fn default_heartbeat_file() -> String {
    "HEARTBEAT.md".to_string()
}

/// 心跳状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatStatus {
    pub enabled: bool,
    pub last_check_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_action_at: Option<chrono::DateTime<chrono::Utc>>,
    pub checks_performed: u64,
    pub actions_taken: u64,
}

/// 心跳服务
///
/// 定期检查 HEARTBEAT.md 文件并执行其中的任务
pub struct HeartbeatService {
    workspace: PathBuf,
    config: HeartbeatConfig,
    callback: Arc<RwLock<Option<HeartbeatCallback>>>,
    status: Arc<RwLock<HeartbeatStatus>>,
    running: Arc<RwLock<bool>>,
    task_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

/// 心跳回调类型（异步：与 Python 一致）
pub type HeartbeatCallback = Arc<
    dyn Fn(&str) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>>
        + Send
        + Sync,
>;

impl HeartbeatService {
    /// 创建新的心跳服务
    pub fn new(
        workspace: PathBuf,
        config: HeartbeatConfig,
    ) -> Self {
        let enabled = config.enabled;
        Self {
            workspace,
            config,
            callback: Arc::new(RwLock::new(None)),
            status: Arc::new(RwLock::new(HeartbeatStatus {
                enabled,
                last_check_at: None,
                last_action_at: None,
                checks_performed: 0,
                actions_taken: 0,
            })),
            running: Arc::new(RwLock::new(false)),
            task_handle: Arc::new(RwLock::new(None)),
        }
    }

    /// 获取心跳文件路径
    pub fn heartbeat_file(&self) -> PathBuf {
        self.workspace.join(&self.config.heartbeat_file)
    }

    /// 设置回调
    pub async fn set_callback(&self, callback: HeartbeatCallback) {
        let mut cb = self.callback.write().await;
        *cb = Some(callback);
    }

    /// 启动心跳服务
    pub async fn start(&self) -> anyhow::Result<()> {
        if !self.config.enabled {
            tracing::info!("Heartbeat disabled");
            return Ok(());
        }

        let mut running = self.running.write().await;
        if *running {
            return Ok(());
        }

        tracing::info!("Starting heartbeat service (interval: {}s)", self.config.interval_seconds);
        *running = true;

        let interval = Duration::from_secs(self.config.interval_seconds);
        let workspace = self.workspace.clone();
        let heartbeat_file = self.config.heartbeat_file.clone();
        let callback = self.callback.clone();
        let status = self.status.clone();
        let running_flag = self.running.clone();

        let handle = tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);

            loop {
                interval_timer.tick().await;

                if !*running_flag.read().await {
                    break;
                }

                if let Err(e) = Self::perform_heartbeat(
                    &workspace,
                    &heartbeat_file,
                    &callback.read().await.clone(),
                    &status,
                ).await {
                    tracing::error!("Heartbeat error: {}", e);
                }
            }

            tracing::info!("Heartbeat service stopped");
        });

        let mut task_handle = self.task_handle.write().await;
        *task_handle = Some(handle);

        Ok(())
    }

    /// 停止心跳服务
    pub async fn stop(&self) -> anyhow::Result<()> {
        tracing::info!("Stopping heartbeat service");

        *self.running.write().await = false;

        let mut task_handle = self.task_handle.write().await;
        if let Some(handle) = task_handle.take() {
            handle.abort();
        }

        Ok(())
    }

    /// 手动触发心跳
    pub async fn trigger_now(&self) -> anyhow::Result<String> {
        let callback = self.callback.read().await;
        let callback = callback.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No callback set"))?;

        callback(HEARTBEAT_PROMPT).await
    }

    /// 获取状态
    pub async fn status(&self) -> HeartbeatStatus {
        self.status.read().await.clone()
    }

    /// 执行心跳检查
    async fn perform_heartbeat(
        workspace: &PathBuf,
        heartbeat_file: &str,
        callback: &Option<HeartbeatCallback>,
        status: &Arc<RwLock<HeartbeatStatus>>,
    ) -> anyhow::Result<()> {
        let heartbeat_path = workspace.join(heartbeat_file);

        // 读取心跳文件
        let content = if heartbeat_path.exists() {
            tokio::fs::read_to_string(&heartbeat_path).await.ok()
        } else {
            None
        };

        // 检查是否有可执行内容
        if is_empty_heartbeat(content.as_deref()) {
            tracing::debug!("Heartbeat file is empty, no action needed");
            return Ok(());
        }

        tracing::info!("Heartbeat: checking for tasks...");

        // 更新状态
        {
            let mut s = status.write().await;
            s.last_check_at = Some(chrono::Utc::now());
            s.checks_performed += 1;
        }

        // 执行回调
        if let Some(cb) = callback.as_ref() {
            // 对齐 Python：只发送固定 HEARTBEAT_PROMPT（agent 自己读取 HEARTBEAT.md）
            let response = cb(HEARTBEAT_PROMPT).await?;

            // 检查是否需要执行操作
            if !is_heartbeat_ok_response(&response) {
                {
                    let mut s = status.write().await;
                    s.last_action_at = Some(chrono::Utc::now());
                    s.actions_taken += 1;
                }
                tracing::info!("Heartbeat: action completed");
            } else {
                tracing::info!("Heartbeat: OK (no action needed)");
            }
        }

        Ok(())
    }
}

/// 检查心跳内容是否为空
fn is_empty_heartbeat(content: Option<&str>) -> bool {
    let Some(content) = content else {
        return true;
    };
    if content.trim().is_empty() {
        return true;
    }

    // 跳过空行、标题、HTML 注释、空复选框
    let skip_exact = ["- [ ]", "* [ ]", "- [x]", "* [x]"];

    for line in content.lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') || line.starts_with("<!--") {
            continue;
        }

        if skip_exact.contains(&line) {
            continue;
        }

        return false; // 找到可执行内容
    }

    true
}

/// 检查响应是否表示 OK
fn is_heartbeat_ok_response(response: &str) -> bool {
    // 对齐 Python：response.upper().replace("_","") contains HEARTBEATOK
    let normalized = response.to_uppercase().replace('_', "");
    normalized.contains("HEARTBEATOK")
}

/// 心跳文件模板
pub fn heartbeat_file_template() -> &'static str {
    r#"# Heartbeat Tasks

This file contains tasks that should be checked periodically by the agent.

## Daily Tasks

- [ ] Check daily schedule
- [ ] Review pending items
- [ ] Send daily summary if requested

## Weekly Tasks

- [ ] Weekly review
- [ ] Clean up old files

## One-time Tasks

- [ ] Example one-time task

---
Instructions:
- Check this file periodically (every 30 minutes by default)
- Execute any checked tasks
- Remove completed tasks to avoid repetition
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_empty_heartbeat() {
        assert!(is_empty_heartbeat(None));
        assert!(is_empty_heartbeat(Some("")));
        assert!(is_empty_heartbeat(Some("# Just a header\n")));
        assert!(is_empty_heartbeat(Some("- [ ]\n")));
        assert!(!is_empty_heartbeat(Some("- [x] Some task\n")));
    }

    #[test]
    fn test_is_heartbeat_ok_response() {
        assert!(is_heartbeat_ok_response("HEARTBEAT_OK"));
        assert!(is_heartbeat_ok_response("heartbeat_ok"));
        assert!(!is_heartbeat_ok_response("I did some work"));
    }
}
