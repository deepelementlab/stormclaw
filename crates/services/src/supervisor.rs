//! 服务监管器 (Service Supervisor)
//!
//! 监控和管理所有子服务的生命周期

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, mpsc};
use serde::{Deserialize, Serialize};

/// 服务状态
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServiceStatus {
    Starting,
    Running,
    Stopping,
    Stopped,
    Error { message: String },
    Degraded { reason: String },
}

/// 服务监管器
///
/// 管理多个子服务的生命周期
pub struct ServiceSupervisor {
    services: Arc<RwLock<HashMap<String, ServiceState>>>,
    event_sender: mpsc::UnboundedSender<SupervisorEvent>,
    event_receiver: Arc<RwLock<Option<mpsc::UnboundedReceiver<SupervisorEvent>>>>,
    restart_policy: RestartPolicy,
    running: Arc<RwLock<bool>>,
}

/// 服务状态信息
#[derive(Clone)]
pub struct ServiceState {
    pub name: String,
    pub status: ServiceStatus,
    pub pid: Option<u32>,
    pub start_time: Option<Instant>,
    pub restart_count: u32,
    pub last_error: Option<String>,
    pub metadata: HashMap<String, String>,
    pub start_fn: Option<ServiceStartFn>,
    pub stop_fn: Option<ServiceStopFn>,
}

impl std::fmt::Debug for ServiceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceState")
            .field("name", &self.name)
            .field("status", &self.status)
            .field("pid", &self.pid)
            .field("start_time", &self.start_time)
            .field("restart_count", &self.restart_count)
            .field("last_error", &self.last_error)
            .field("metadata", &self.metadata)
            .field("start_fn", &self.start_fn.as_ref().map(|_| "<fn>"))
            .field("stop_fn", &self.stop_fn.as_ref().map(|_| "<fn>"))
            .finish()
    }
}

/// 监管事件
#[derive(Debug, Clone)]
pub enum SupervisorEvent {
    ServiceStarted { name: String },
    ServiceStopped { name: String, exit_code: Option<i32> },
    ServiceError { name: String, error: String },
    ServiceRestarting { name: String },
    CheckFailed { name: String, reason: String },
}

/// 重启策略
#[derive(Debug, Clone, Copy)]
pub enum RestartPolicy {
    /// 不自动重启
    Never,
    /// 总是重启
    Always,
    /// 失败时重启
    OnFailure,
    /// 指定次数后停止
    Limit { max_restarts: u32 },
}

impl ServiceSupervisor {
    /// 创建新的服务监管器
    pub fn new(restart_policy: RestartPolicy) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
            event_sender: tx,
            event_receiver: Arc::new(RwLock::new(Some(rx))),
            restart_policy,
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// 启动监管器
    pub async fn start(&self) -> anyhow::Result<()> {
        *self.running.write().await = true;
        tracing::info!("Service supervisor started");

        // 启动事件处理循环
        let receiver = self.event_receiver.clone();
        let services = self.services.clone();
        let policy = self.restart_policy;
        let running = self.running.clone();

        tokio::spawn(async move {
            let mut rx = receiver.write().await.take();
            while *running.read().await {
                if let Some(ref mut rx) = rx {
                    if let Some(event) = rx.recv().await {
                        Self::handle_event(&services, &policy, event).await;
                    }
                } else {
                    break;
                }
            }
        });

        Ok(())
    }

    /// 停止监管器
    pub async fn stop(&self) -> anyhow::Result<()> {
        tracing::info!("Stopping service supervisor");
        *self.running.write().await = false;

        // 停止所有服务
        let services = self.services.read().await;
        for (name, _) in services.iter() {
            if let Err(e) = self.stop_service(name).await {
                tracing::warn!("Error stopping service {}: {}", name, e);
            }
        }

        Ok(())
    }

    /// 注册服务（可选随后调用 `set_stop_fn` 注册停止回调）
    pub async fn register_service(&self, name: String, start_fn: ServiceStartFn) {
        let mut services = self.services.write().await;
        let entry = services.entry(name.clone()).or_insert_with(|| ServiceState {
            name: name.clone(),
            status: ServiceStatus::Stopped,
            pid: None,
            start_time: None,
            restart_count: 0,
            last_error: None,
            metadata: HashMap::new(),
            start_fn: None,
            stop_fn: None,
        });
        entry.start_fn = Some(start_fn);
    }

    /// 为已注册服务设置停止回调
    pub async fn set_stop_fn(&self, name: &str, stop_fn: ServiceStopFn) {
        let mut services = self.services.write().await;
        if let Some(entry) = services.get_mut(name) {
            entry.stop_fn = Some(stop_fn);
        }
    }

    /// 启动服务
    pub async fn start_service(&self, name: &str) -> anyhow::Result<()> {
        let name = name.to_string();
        let event_sender = self.event_sender.clone();

        let start_fn = {
            let mut services = self.services.write().await;
            let state = services
                .get_mut(&name)
                .ok_or_else(|| anyhow::anyhow!("unknown service: {}", name))?;
            state.status = ServiceStatus::Starting;
            state.start_fn.clone()
        };

        let Some(start_fn) = start_fn else {
            let mut services = self.services.write().await;
            if let Some(state) = services.get_mut(&name) {
                state.status = ServiceStatus::Error {
                    message: "no start_fn registered".to_string(),
                };
                state.last_error = Some("no start_fn registered".to_string());
            }
            let err = "no start_fn registered".to_string();
            let _ = event_sender.send(SupervisorEvent::ServiceError {
                name: name.clone(),
                error: err.clone(),
            });
            anyhow::bail!(err);
        };

        match start_fn() {
            Ok(()) => {
                let mut services = self.services.write().await;
                if let Some(state) = services.get_mut(&name) {
                    state.status = ServiceStatus::Running;
                    state.start_time = Some(Instant::now());
                }
                event_sender.send(SupervisorEvent::ServiceStarted { name: name.clone() })?;
                tracing::info!("Service '{}' started", name);
                Ok(())
            }
            Err(e) => {
                let msg = e.to_string();
                let mut services = self.services.write().await;
                if let Some(state) = services.get_mut(&name) {
                    state.status = ServiceStatus::Error {
                        message: msg.clone(),
                    };
                    state.last_error = Some(msg.clone());
                }
                event_sender.send(SupervisorEvent::ServiceError {
                    name: name.clone(),
                    error: msg.clone(),
                })?;
                Err(e)
            }
        }
    }

    /// 停止服务
    pub async fn stop_service(&self, name: &str) -> anyhow::Result<()> {
        let services = self.services.clone();
        let name = name.to_string();
        let event_sender = self.event_sender.clone();

        let stop_fn = {
            let mut services = services.write().await;
            if let Some(state) = services.get_mut(&name) {
                state.status = ServiceStatus::Stopping;
                state.stop_fn.clone()
            } else {
                None
            }
        };

        if let Some(stop_fn) = stop_fn {
            if let Err(e) = stop_fn() {
                tracing::warn!("Service '{}' stop_fn error: {}", name, e);
            }
        }

        event_sender.send(SupervisorEvent::ServiceStopped {
            name: name.clone(),
            exit_code: None,
        })?;

        {
            let mut services = services.write().await;
            if let Some(state) = services.get_mut(&name) {
                state.status = ServiceStatus::Stopped;
                state.start_time = None;
            }
        }

        tracing::info!("Service '{}' stopped", name);
        Ok(())
    }

    /// 获取服务状态
    pub async fn get_service_status(&self, name: &str) -> Option<ServiceState> {
        let services = self.services.read().await;
        services.get(name).cloned()
    }

    /// 获取所有服务状态
    pub async fn get_all_status(&self) -> HashMap<String, ServiceState> {
        self.services.read().await.clone()
    }

    /// 重启服务
    pub async fn restart_service(&self, name: &str) -> anyhow::Result<()> {
        tracing::info!("Restarting service '{}'", name);

        self.stop_service(name).await?;
        self.start_service(name).await?;

        {
            let mut services = self.services.write().await;
            if let Some(state) = services.get_mut(name) {
                state.restart_count += 1;
            }
        }

        Ok(())
    }

    /// 健康检查所有服务
    pub async fn health_check(&self) -> HashMap<String, bool> {
        let services = self.services.read().await;
        let mut health = HashMap::new();

        for (name, state) in services.iter() {
            let is_healthy = matches!(state.status, ServiceStatus::Running);
            health.insert(name.clone(), is_healthy);
        }

        health
    }

    /// 处理事件
    async fn handle_event(
        services: &Arc<RwLock<HashMap<String, ServiceState>>>,
        policy: &RestartPolicy,
        event: SupervisorEvent,
    ) {
        match event {
            SupervisorEvent::ServiceError { name, error } => {
                tracing::error!("Service '{}' error: {}", name, error);

                {
                    let mut s = services.write().await;
                    if let Some(state) = s.get_mut(&name) {
                        state.status = ServiceStatus::Error {
                            message: error.clone(),
                        };
                        state.last_error = Some(error);
                    }
                }

                // 根据重启策略决定是否重启
                match policy {
                    RestartPolicy::Always => {
                        Self::schedule_restart(&name, services).await;
                    }
                    RestartPolicy::OnFailure => {
                        Self::schedule_restart(&name, services).await;
                    }
                    RestartPolicy::Limit { max_restarts } => {
                        let restart_count = {
                            let s = services.read().await;
                            s.get(&name).map(|s| s.restart_count).unwrap_or(0)
                        };

                        if restart_count < *max_restarts {
                            Self::schedule_restart(&name, services).await;
                        }
                    }
                    RestartPolicy::Never => {}
                }
            }
            SupervisorEvent::CheckFailed { name, reason } => {
                tracing::warn!("Service '{}' check failed: {}", name, reason);

                {
                    let mut s = services.write().await;
                    if let Some(state) = s.get_mut(&name) {
                        state.status = ServiceStatus::Degraded {
                            reason: reason.clone(),
                        };
                    }
                }
            }
            _ => {}
        }
    }

    /// 安排重启
    async fn schedule_restart(
        name: &str,
        services: &Arc<RwLock<HashMap<String, ServiceState>>>,
    ) {
        {
            let mut s = services.write().await;
            if let Some(state) = s.get_mut(name) {
                state.status = ServiceStatus::Stopping;
            }
        }

        tracing::info!("Scheduling restart for service '{}'", name);

        // 延迟重启
        let name = name.to_string();
        let services = services.clone();
        // NOTE: 这里不尝试从 metadata 反序列化 sender（不可行）。
        // 由上层在真正实现 restart 调度时注入通知机制。

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;

            {
                let mut s = services.write().await;
                if let Some(state) = s.get_mut(&name) {
                    state.status = ServiceStatus::Starting;
                }
            }
        });
    }
}

/// 服务启动函数类型
pub type ServiceStartFn = Arc<dyn Fn() -> anyhow::Result<()> + Send + Sync>;

/// 服务停止函数类型
pub type ServiceStopFn = Arc<dyn Fn() -> anyhow::Result<()> + Send + Sync>;

/// 服务监控数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceMetrics {
    pub name: String,
    pub uptime_seconds: u64,
    pub cpu_percent: f32,
    pub memory_mb: f32,
    pub requests_per_second: f32,
    pub error_rate: f32,
}

/// 服务依赖关系
#[derive(Debug, Clone)]
pub struct ServiceDependency {
    pub service: String,
    pub depends_on: Vec<String>,
}

/// 服务拓扑
pub struct ServiceTopology {
    services: Vec<String>,
    dependencies: Vec<ServiceDependency>,
}

impl ServiceTopology {
    pub fn new() -> Self {
        Self {
            services: Vec::new(),
            dependencies: Vec::new(),
        }
    }

    pub fn add_service(&mut self, name: String) {
        self.services.push(name);
    }

    pub fn add_dependency(&mut self, service: String, depends_on: String) {
        if let Some(dep) = self.dependencies.iter_mut().find(|d| d.service == service) {
            dep.depends_on.push(depends_on);
        } else {
            self.dependencies.push(ServiceDependency {
                service,
                depends_on: vec![depends_on],
            });
        }
    }

    /// 获取启动顺序
    pub fn startup_order(&self) -> Vec<String> {
        let mut order = Vec::new();
        let mut started = std::collections::HashSet::new();

        loop {
            let mut progress = false;

            for service in &self.services {
                if started.contains(service) {
                    continue;
                }

                let deps = self.dependencies
                    .iter()
                    .find(|d| &d.service == service)
                    .map(|d| d.depends_on.as_slice())
                    .unwrap_or(&[]);

                if deps.iter().all(|d| started.contains(d)) {
                    order.push(service.clone());
                    started.insert(service.clone());
                    progress = true;
                }
            }

            if !progress {
                break;
            }
        }

        order
    }
}
