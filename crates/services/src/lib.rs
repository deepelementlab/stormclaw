//! 业务服务模块
//!
//! 提供定时任务、心跳、网关等业务服务

pub mod cron;
pub mod heartbeat;
pub mod gateway;
pub mod supervisor;
pub mod lifecycle;
pub mod logging;
pub mod config_reload;
pub mod metrics;

pub use cron::{CronService, CronJob, CronSchedule, CronPayload, CronJobState, CronStore, CronServiceStatus, every_job};
pub use heartbeat::{HeartbeatService, HeartbeatConfig, HeartbeatStatus};
pub use gateway::{GatewayService, GatewayConfig, GatewayBuilder, GatewayStatus, GatewayStats, GatewayEvent};
pub use supervisor::{
    ServiceSupervisor, ServiceStatus, ServiceStartFn, ServiceStopFn, RestartPolicy, ServiceTopology,
    ServiceMetrics,
};
pub use lifecycle::{ServiceLifecycle, LifecyclePhase, LifecycleEvent, LifecycleManager, ServiceLauncher, GracefulShutdown, HealthChecker, HealthStatus};
pub use logging::{LoggingService, LoggingConfig, LogLevel, ServiceLogger};
pub use config_reload::{HotReloadService, HotReloadConfig, ReloadCallback, ConfigReloader, ConfigValidator};
pub use metrics::{MetricsCollector, Metric, MetricType, ServiceMetrics as MetricsServiceMetrics, Counter, Gauge, Histogram};
