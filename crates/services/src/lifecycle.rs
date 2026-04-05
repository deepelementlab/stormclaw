//! 服务生命周期管理 (Service Lifecycle)
//!
//! 管理服务的启动、停止、重启等生命周期事件

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, broadcast};
use serde::{Deserialize, Serialize};

/// 生命周期阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecyclePhase {
    Initializing,
    Starting,
    Running,
    Stopping,
    Stopped,
    Error,
    Restarting,
}

/// 生命周期事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LifecycleEvent {
    Initializing,
    Starting,
    Started,
    Stopping,
    Stopped,
    Error(String),
    Restarting,
    Shutdown,
}

/// 服务生命周期管理器
pub struct ServiceLifecycle {
    name: String,
    phase: Arc<RwLock<LifecyclePhase>>,
    start_time: Arc<RwLock<Option<Instant>>>,
    stop_time: Arc<RwLock<Option<Instant>>>,
    event_sender: broadcast::Sender<LifecycleEvent>,
    event_receiver: broadcast::Receiver<LifecycleEvent>,
}

impl ServiceLifecycle {
    /// 创建新的生命周期管理器
    pub fn new(name: String) -> Self {
        let (tx, rx) = broadcast::channel(100);

        Self {
            name,
            phase: Arc::new(RwLock::new(LifecyclePhase::Initializing)),
            start_time: Arc::new(RwLock::new(None)),
            stop_time: Arc::new(RwLock::new(None)),
            event_sender: tx,
            event_receiver: rx,
        }
    }

    /// 获取当前阶段
    pub async fn phase(&self) -> LifecyclePhase {
        *self.phase.read().await
    }

    /// 检查是否运行中
    pub async fn is_running(&self) -> bool {
        matches!(*self.phase.read().await, LifecyclePhase::Running)
    }

    /// 获取运行时间（秒）
    pub async fn uptime_seconds(&self) -> u64 {
        if let Some(start) = *self.start_time.read().await {
            start.elapsed().as_secs()
        } else {
            0
        }
    }

    /// 发布事件
    pub async fn publish(&self, event: LifecycleEvent) {
        tracing::debug!("Service '{}': {:?}", self.name, event);

        // 更新阶段
        match &event {
            LifecycleEvent::Initializing => {
                *self.phase.write().await = LifecyclePhase::Initializing;
            }
            LifecycleEvent::Starting => {
                *self.phase.write().await = LifecyclePhase::Starting;
            }
            LifecycleEvent::Started => {
                *self.phase.write().await = LifecyclePhase::Running;
                *self.start_time.write().await = Some(Instant::now());
                *self.stop_time.write().await = None;
            }
            LifecycleEvent::Stopping => {
                *self.phase.write().await = LifecyclePhase::Stopping;
            }
            LifecycleEvent::Stopped => {
                *self.phase.write().await = LifecyclePhase::Stopped;
                *self.stop_time.write().await = Some(Instant::now());
            }
            LifecycleEvent::Error(_) => {
                *self.phase.write().await = LifecyclePhase::Error;
            }
            LifecycleEvent::Restarting => {
                *self.phase.write().await = LifecyclePhase::Restarting;
            }
            LifecycleEvent::Shutdown => {
                *self.phase.write().await = LifecyclePhase::Stopped;
                *self.stop_time.write().await = Some(Instant::now());
            }
        }

        // 广播事件
        let _ = self.event_sender.send(event);
    }

    /// 订阅事件
    pub fn subscribe(&self) -> broadcast::Receiver<LifecycleEvent> {
        self.event_sender.subscribe()
    }

    /// 等待特定阶段
    pub async fn wait_for_phase(&self, target: LifecyclePhase) -> anyhow::Result<()> {
        let mut rx = self.subscribe();

        loop {
            if *self.phase.read().await == target {
                return Ok(());
            }

            match rx.recv().await {
                Ok(_) => continue,
                Err(e) => anyhow::bail!("Lifecycle channel closed: {}", e),
            }
        }
    }

    /// 等待运行状态
    pub async fn wait_until_running(&self) -> anyhow::Result<()> {
        self.wait_for_phase(LifecyclePhase::Running).await
    }

    /// 等待停止状态
    pub async fn wait_until_stopped(&self) -> anyhow::Result<()> {
        if matches!(*self.phase.read().await, LifecyclePhase::Stopped) {
            return Ok(());
        }
        self.wait_for_phase(LifecyclePhase::Stopped).await
    }
}

/// 生命周期管理器
///
/// 管理多个服务的生命周期
pub struct LifecycleManager {
    services: Arc<RwLock<HashMap<String, Arc<ServiceLifecycle>>>>,
}

impl LifecycleManager {
    /// 创建新的生命周期管理器
    pub fn new() -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册服务
    pub async fn register(&self, name: String) -> Arc<ServiceLifecycle> {
        let lifecycle = Arc::new(ServiceLifecycle::new(name.clone()));
        let mut services = self.services.write().await;
        services.insert(name, lifecycle.clone());
        lifecycle
    }

    /// 获取服务生命周期
    pub async fn get(&self, name: &str) -> Option<Arc<ServiceLifecycle>> {
        self.services.read().await.get(name).cloned()
    }

    /// 启动所有服务（按依赖顺序）
    pub async fn start_all(&self) -> anyhow::Result<()> {
        let services = self.services.read().await;
        let mut names: Vec<String> = services.keys().cloned().collect();
        names.sort(); // 确保确定性启动顺序

        for name in names {
            if let Some(lifecycle) = services.get(&name) {
                lifecycle.publish(LifecycleEvent::Starting).await;
                lifecycle.publish(LifecycleEvent::Started).await;
            }
        }

        Ok(())
    }

    /// 停止所有服务（反向顺序）
    pub async fn stop_all(&self) -> anyhow::Result<()> {
        let services = self.services.read().await;
        let mut names: Vec<String> = services.keys().cloned().collect();
        names.sort();
        names.reverse(); // 反向停止

        for name in names {
            if let Some(lifecycle) = services.get(&name) {
                lifecycle.publish(LifecycleEvent::Stopping).await;
                lifecycle.publish(LifecycleEvent::Stopped).await;
            }
        }

        Ok(())
    }

    /// 获取所有服务状态
    pub async fn get_all_status(&self) -> HashMap<String, LifecyclePhase> {
        let services = self.services.read().await;
        let mut status = HashMap::new();

        for (name, lifecycle) in services.iter() {
            status.insert(name.clone(), *lifecycle.phase.read().await);
        }

        status
    }
}

/// 服务启动器
///
/// 提供服务启动的通用模式
pub struct ServiceLauncher {
    lifecycle: Arc<ServiceLifecycle>,
    shutdown_signal: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl ServiceLauncher {
    /// 创建新的启动器
    pub fn new(name: String) -> Self {
        Self {
            lifecycle: Arc::new(ServiceLifecycle::new(name)),
            shutdown_signal: Arc::new(RwLock::new(None)),
        }
    }

    /// 启动服务
    pub async fn launch<Fut>(
        &self,
        start_fn: impl FnOnce(Arc<ServiceLifecycle>) -> Fut + Send + 'static,
    ) -> anyhow::Result<()>
    where
        Fut: std::future::Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        self.lifecycle.publish(LifecycleEvent::Initializing).await;
        self.lifecycle.publish(LifecycleEvent::Starting).await;

        let lifecycle = self.lifecycle.clone();
        let running_flag = Arc::new(RwLock::new(true));

        // 启动服务任务
        let handle = tokio::spawn(async move {
            let result = start_fn(lifecycle.clone()).await;

            // 服务完成或出错
            if let Err(e) = result {
                lifecycle.publish(LifecycleEvent::Error(e.to_string())).await;
            } else {
                lifecycle.publish(LifecycleEvent::Stopped).await;
            }

            *running_flag.write().await = false;
        });

        // 设置关闭信号处理
        let mut shutdown = self.shutdown_signal.write().await;
        *shutdown = Some(handle);

        Ok(())
    }

    /// 优雅关闭
    pub async fn shutdown(self) -> anyhow::Result<()> {
        self.lifecycle.publish(LifecycleEvent::Stopping).await;

        if let Some(handle) = self.shutdown_signal.write().await.take() {
            handle.abort();
        }

        self.lifecycle.publish(LifecycleEvent::Stopped).await;
        Ok(())
    }
}

/// 优雅关闭
///
/// 等待所有服务完成当前任务后关闭。未注册任何 `stop_signal` 时，`shutdown` 不阻塞等待。
pub struct GracefulShutdown {
    timeout: Duration,
    services: Vec<String>,
    stop_signals: Vec<Arc<AtomicBool>>,
}

impl GracefulShutdown {
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            services: Vec::new(),
            stop_signals: Vec::new(),
        }
    }

    /// 添加需要等待的服务（用于日志与排查）
    pub fn add_service(&mut self, name: String) {
        self.services.push(name);
    }

    /// 注册停止完成信号：各服务退出前将对应 `AtomicBool` 置为 `true`
    pub fn register_stop_signal(&mut self, flag: Arc<AtomicBool>) {
        self.stop_signals.push(flag);
    }

    /// 执行关闭
    pub async fn shutdown(self) -> anyhow::Result<()> {
        tracing::info!("Starting graceful shutdown (timeout: {:?})", self.timeout);
        if !self.services.is_empty() {
            tracing::info!(
                "GracefulShutdown awaiting signals for services: {:?}",
                self.services
            );
        }

        let start = Instant::now();

        if self.stop_signals.is_empty() {
            tracing::debug!("No stop signals registered; skipping wait");
            return Ok(());
        }

        while start.elapsed() < self.timeout {
            let all_stopped = self
                .stop_signals
                .iter()
                .all(|f| f.load(Ordering::SeqCst));

            if all_stopped {
                tracing::info!("All stop signals set; graceful shutdown complete");
                break;
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        Ok(())
    }
}

/// 健康检查
pub struct HealthChecker {
    services: Arc<RwLock<HashMap<String, Arc<dyn HealthCheck>>>>,
}

/// 健康检查接口
#[async_trait::async_trait]
pub trait HealthCheck: Send + Sync {
    async fn check_health(&self) -> HealthStatus;
    async fn check_readiness(&self) -> bool {
        matches!(self.check_health().await, HealthStatus::Healthy)
    }
}

/// 健康状态
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded { reason: String },
    Unhealthy { reason: String },
}

impl HealthChecker {
    pub fn new() -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register(&self, name: String, checker: Arc<dyn HealthCheck>) {
        let mut services = self.services.write().await;
        services.insert(name, checker);
    }

    pub async fn check_all(&self) -> HashMap<String, HealthStatus> {
        let services = self.services.read().await;
        let mut results = HashMap::new();

        for (name, checker) in services.iter() {
            let status = checker.check_health().await;
            results.insert(name.clone(), status);
        }

        results
    }

    pub async fn is_healthy(&self) -> bool {
        let results = self.check_all().await;
        results.values().all(|s| matches!(s, HealthStatus::Healthy))
    }
}
