//! 定时任务服务 (Cron Service)
//!
//! 对齐 Python 版本：单定时器 + nextRunAtMs 计算 + jobs.json camelCase 存储格式

use chrono::{TimeZone, Utc};
use cron::Schedule as CronScheduleExpr;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use uuid::Uuid;

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronSchedule {
    pub kind: String, // "at" | "every" | "cron"
    #[serde(rename = "atMs", default)]
    pub at_ms: Option<i64>,
    #[serde(rename = "everyMs", default)]
    pub every_ms: Option<i64>,
    #[serde(default)]
    pub expr: Option<String>,
    #[serde(default)]
    pub tz: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronPayload {
    #[serde(default = "default_payload_kind")]
    pub kind: String,
    pub message: String,
    #[serde(default)]
    pub deliver: bool,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
}

fn default_payload_kind() -> String {
    "agent_turn".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJobState {
    #[serde(rename = "nextRunAtMs", default)]
    pub next_run_at_ms: Option<i64>,
    #[serde(rename = "lastRunAtMs", default)]
    pub last_run_at_ms: Option<i64>,
    #[serde(rename = "lastStatus", default)]
    pub last_status: Option<String>, // "ok" | "error" | "skipped"
    #[serde(rename = "lastError", default)]
    pub last_error: Option<String>,
}

impl Default for CronJobState {
    fn default() -> Self {
        Self {
            next_run_at_ms: None,
            last_run_at_ms: None,
            last_status: None,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    pub id: String,
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub schedule: CronSchedule,
    pub payload: CronPayload,
    #[serde(default)]
    pub state: CronJobState,
    #[serde(rename = "createdAtMs", default)]
    pub created_at_ms: i64,
    #[serde(rename = "updatedAtMs", default)]
    pub updated_at_ms: i64,
    #[serde(rename = "deleteAfterRun", default)]
    pub delete_after_run: bool,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronStore {
    pub version: u32,
    pub jobs: Vec<CronJob>,
}

impl Default for CronStore {
    fn default() -> Self {
        Self {
            version: 1,
            jobs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronServiceStatus {
    pub enabled: bool,
    pub total_jobs: usize,
    pub enabled_jobs: usize,
    pub next_wake_at_ms: Option<i64>,
}

/// 任务执行回调（异步：与 Python 一致）
pub type JobCallback = Arc<
    dyn Fn(&CronJob) -> Pin<Box<dyn Future<Output = anyhow::Result<Option<String>>> + Send>>
        + Send
        + Sync,
>;

pub struct CronService {
    store_path: std::path::PathBuf,
    store: Arc<RwLock<CronStore>>,
    on_job: Arc<RwLock<Option<JobCallback>>>,
    running: Arc<RwLock<bool>>,
    timer: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl CronService {
    pub async fn new(store_path: std::path::PathBuf) -> anyhow::Result<Self> {
        Ok(Self {
            store_path,
            store: Arc::new(RwLock::new(CronStore::default())),
            on_job: Arc::new(RwLock::new(None)),
            running: Arc::new(RwLock::new(false)),
            timer: Arc::new(RwLock::new(None)),
        })
    }

    pub async fn set_callback(&self, callback: JobCallback) {
        *self.on_job.write().await = Some(callback);
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        if *self.running.read().await {
            return Ok(());
        }
        *self.running.write().await = true;

        self.load_store().await?;
        self.recompute_next_runs().await;
        self.save_store().await?;
        self.arm_timer().await;
        Ok(())
    }

    pub async fn stop(&self) -> anyhow::Result<()> {
        *self.running.write().await = false;
        if let Some(h) = self.timer.write().await.take() {
            h.abort();
        }
        Ok(())
    }

    pub async fn status(&self) -> CronServiceStatus {
        let store = self.store.read().await;
        let enabled_jobs = store.jobs.iter().filter(|j| j.enabled).count();
        CronServiceStatus {
            enabled: *self.running.read().await,
            total_jobs: store.jobs.len(),
            enabled_jobs,
            next_wake_at_ms: self.get_next_wake_ms().await,
        }
    }

    pub async fn list_jobs(&self, include_disabled: bool) -> Vec<CronJob> {
        let store = self.store.read().await;
        let mut jobs: Vec<CronJob> = store
            .jobs
            .iter()
            .filter(|j| include_disabled || j.enabled)
            .cloned()
            .collect();
        jobs.sort_by_key(|j| j.state.next_run_at_ms.unwrap_or(i64::MAX));
        jobs
    }

    pub async fn add_job(&self, mut job: CronJob) -> anyhow::Result<CronJob> {
        let now = now_ms();
        if job.created_at_ms == 0 {
            job.created_at_ms = now;
        }
        job.updated_at_ms = now;
        if job.state.next_run_at_ms.is_none() {
            job.state.next_run_at_ms = compute_next_run(&job.schedule, now);
        }

        let mut store = self.store.write().await;
        store.jobs.push(job.clone());
        drop(store);

        self.save_store().await?;
        self.arm_timer().await;
        Ok(job)
    }

    pub async fn remove_job(&self, job_id: &str) -> anyhow::Result<bool> {
        let mut store = self.store.write().await;
        let before = store.jobs.len();
        store.jobs.retain(|j| j.id != job_id);
        let removed = store.jobs.len() != before;
        drop(store);

        if removed {
            self.save_store().await?;
            self.arm_timer().await;
        }
        Ok(removed)
    }

    pub async fn enable_job(&self, job_id: &str, enabled: bool) -> anyhow::Result<Option<CronJob>> {
        let mut store = self.store.write().await;
        for job in &mut store.jobs {
            if job.id == job_id {
                job.enabled = enabled;
                job.updated_at_ms = now_ms();
                job.state.next_run_at_ms = if enabled {
                    compute_next_run(&job.schedule, now_ms())
                } else {
                    None
                };
                let out = job.clone();
                drop(store);
                self.save_store().await?;
                self.arm_timer().await;
                return Ok(Some(out));
            }
        }
        Ok(None)
    }

    pub async fn run_job(&self, job_id: &str, force: bool) -> anyhow::Result<bool> {
        let mut store = self.store.write().await;
        for job in &mut store.jobs {
            if job.id == job_id {
                if !force && !job.enabled {
                    return Ok(false);
                }
                let job_clone = job.clone();
                drop(store);
                self.execute_job(job_clone).await?;
                self.save_store().await?;
                self.arm_timer().await;
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn load_store(&self) -> anyhow::Result<()> {
        if !self.store_path.exists() {
            return Ok(());
        }
        let content = tokio::fs::read_to_string(&self.store_path).await?;
        let parsed: CronStore = serde_json::from_str(&content)?;
        *self.store.write().await = parsed;
        Ok(())
    }

    async fn save_store(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.store_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let store = self.store.read().await;
        let content = serde_json::to_string_pretty(&*store)?;
        tokio::fs::write(&self.store_path, content).await?;
        Ok(())
    }

    async fn recompute_next_runs(&self) {
        let now = now_ms();
        let mut store = self.store.write().await;
        for job in &mut store.jobs {
            if job.enabled {
                job.state.next_run_at_ms = compute_next_run(&job.schedule, now);
            }
        }
    }

    async fn get_next_wake_ms(&self) -> Option<i64> {
        let store = self.store.read().await;
        store
            .jobs
            .iter()
            .filter(|j| j.enabled)
            .filter_map(|j| j.state.next_run_at_ms)
            .min()
    }

    async fn arm_timer(&self) {
        if let Some(h) = self.timer.write().await.take() {
            h.abort();
        }

        if !*self.running.read().await {
            return;
        }

        let this = self.clone_arc();
        let handle = tokio::spawn(async move {
            loop {
                if !*this.running.read().await {
                    break;
                }

                let Some(next) = this.get_next_wake_ms().await else {
                    break;
                };
                let delay_ms = (next - now_ms()).max(0) as u64;
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;

                if *this.running.read().await {
                    let _ = this.on_timer().await;
                }
            }
        });

        *self.timer.write().await = Some(handle);
    }

    async fn on_timer(&self) -> anyhow::Result<()> {
        let now = now_ms();
        let due: Vec<CronJob> = {
            let store = self.store.read().await;
            store
                .jobs
                .iter()
                .filter(|j| j.enabled)
                .filter(|j| j.state.next_run_at_ms.map(|t| now >= t).unwrap_or(false))
                .cloned()
                .collect()
        };

        for job in due {
            self.execute_job(job).await?;
        }

        self.save_store().await?;
        Ok(())
    }

    async fn execute_job(&self, job: CronJob) -> anyhow::Result<()> {
        let start_ms = now_ms();

        let mut status = Some("ok".to_string());
        let mut last_error: Option<String> = None;
        let mut response_text: Option<String> = None;

        if let Some(cb) = self.on_job.read().await.clone() {
            match cb(&job).await {
                Ok(r) => response_text = r,
                Err(e) => {
                    status = Some("error".to_string());
                    last_error = Some(e.to_string());
                }
            }
        }

        let mut store = self.store.write().await;
        let idx = store.jobs.iter().position(|j| j.id == job.id);
        if let Some(idx) = idx {
            // 先复制关键字段，避免 retain 闭包捕获可变借用
            let delete_after_run = store.jobs[idx].delete_after_run;
            let kind = store.jobs[idx].schedule.kind.clone();
            let job_id = store.jobs[idx].id.clone();

            store.jobs[idx].state.last_run_at_ms = Some(start_ms);
            store.jobs[idx].state.last_status = status;
            store.jobs[idx].state.last_error = last_error;
            store.jobs[idx].updated_at_ms = now_ms();

            if kind == "at" {
                if delete_after_run {
                    store.jobs.retain(|x| x.id != job_id);
                } else {
                    store.jobs[idx].enabled = false;
                    store.jobs[idx].state.next_run_at_ms = None;
                }
            } else {
                let next = compute_next_run(&store.jobs[idx].schedule, now_ms());
                store.jobs[idx].state.next_run_at_ms = next;
            }

            // deliver/channel/to 由 Gateway 的回调负责落地（与 Python 一致）
            let _ = response_text;
        }

        Ok(())
    }

    fn clone_arc(&self) -> Arc<CronService> {
        // Used only for timer task; wrap self in Arc by cloning fields.
        Arc::new(CronService {
            store_path: self.store_path.clone(),
            store: self.store.clone(),
            on_job: self.on_job.clone(),
            running: self.running.clone(),
            timer: self.timer.clone(),
        })
    }
}

fn compute_next_run(schedule: &CronSchedule, now_ms: i64) -> Option<i64> {
    match schedule.kind.as_str() {
        "at" => schedule.at_ms.filter(|t| *t > now_ms),
        "every" => {
            let every = schedule.every_ms.unwrap_or(0);
            if every <= 0 {
                None
            } else {
                Some(now_ms + every)
            }
        }
        "cron" => {
            let expr = schedule.expr.as_deref()?;
            let sched = CronScheduleExpr::from_str(expr).ok()?;
            let next = sched.upcoming(Utc).next()?;
            Some(next.timestamp_millis())
        }
        _ => None,
    }
}

// 便捷构造：对齐 Python id 取前 8
pub fn every_job(name: String, message: String, interval_seconds: i64) -> CronJob {
    let schedule = CronSchedule {
        kind: "every".to_string(),
        at_ms: None,
        every_ms: Some(interval_seconds * 1000),
        expr: None,
        tz: None,
    };
    CronJob {
        id: Uuid::new_v4().to_string()[..8].to_string(),
        name,
        enabled: true,
        schedule,
        payload: CronPayload {
            kind: "agent_turn".to_string(),
            message,
            deliver: false,
            channel: None,
            to: None,
        },
        state: CronJobState::default(),
        created_at_ms: now_ms(),
        updated_at_ms: now_ms(),
        delete_after_run: false,
    }
}

#[cfg(test)]
mod serde_tests {
    use super::*;

    #[test]
    fn cron_store_json_roundtrip() {
        let job = every_job("j".into(), "msg".into(), 30);
        let store = CronStore {
            version: 1,
            jobs: vec![job],
        };
        let json = serde_json::to_string(&store).expect("serialize");
        let back: CronStore = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.version, 1);
        assert_eq!(back.jobs.len(), 1);
        assert_eq!(back.jobs[0].name, "j");
        assert_eq!(back.jobs[0].schedule.kind, "every");
        assert_eq!(back.jobs[0].schedule.every_ms, Some(30_000));
    }

    #[test]
    fn cron_job_json_uses_camel_case_keys() {
        let job = every_job("x".into(), "y".into(), 1);
        let v = serde_json::to_value(&job).unwrap();
        assert!(v.get("createdAtMs").is_some());
        assert!(v.get("schedule").is_some());
        let sch = v.get("schedule").unwrap();
        assert!(sch.get("everyMs").is_some(), "{:?}", sch);
    }
}
