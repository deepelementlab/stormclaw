//! 网关服务 (Gateway Service)
//!
//! 统一的网关服务，协调 Agent、渠道、定时任务、心跳等所有服务

use std::sync::Arc;
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::RwLock;
use tokio::signal;
use serde::{Deserialize, Serialize};
use stormclaw_core::{MessageBus, AgentLoop};
use stormclaw_core::providers::OpenAIProvider;
use stormclaw_core::LLMProvider;
use stormclaw_channels::ChannelManager;
use stormclaw_config::{Config, get_config_path};
use super::{
    ConfigValidator, CronService, HeartbeatConfig, HeartbeatService, HotReloadConfig,
    HotReloadService, LifecycleEvent, ReloadCallback, ServiceLifecycle,
};
use axum::http::StatusCode;

#[cfg(feature = "gateway")]
use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};

/// 网关配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// 监听地址
    #[serde(default = "default_host")]
    pub host: String,

    /// 监听端口
    #[serde(default = "default_port")]
    pub port: u16,

    /// 是否启用 HTTP API
    #[serde(default = "default_http_enabled")]
    pub http_enabled: bool,

    /// 是否启用 metrics 端点
    #[serde(default = "default_metrics_enabled")]
    pub metrics_enabled: bool,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    18789
}

fn default_http_enabled() -> bool {
    true
}

fn default_metrics_enabled() -> bool {
    true
}

/// 网关服务
///
/// 这是主服务，负责协调所有其他服务
pub struct GatewayService {
    config: GatewayConfig,
    /// 与 `ChannelManager` 共享；文件热重载成功后整份替换
    shared_config: Arc<RwLock<Config>>,
    bus: Arc<MessageBus>,
    agent: Arc<AgentLoop<OpenAIProvider>>,
    channels: Arc<ChannelManager>,
    cron: Arc<CronService>,
    heartbeat: Arc<HeartbeatService>,
    lifecycle: Arc<ServiceLifecycle>,
    running: Arc<RwLock<bool>>,
    hot_reload: Mutex<Option<HotReloadService>>,
}

impl GatewayService {
    /// 创建新的网关服务
    pub async fn new(
        config: Config,
        gateway_config: GatewayConfig,
    ) -> anyhow::Result<Self> {
        let bus = Arc::new(MessageBus::new(1000));
        let shared_config = Arc::new(RwLock::new(config.clone()));

        let workspace = config.workspace_path();
        let model = config.agents.defaults.model.clone();
        let brave_api_key = config.tools.web.search.api_key.clone();

        // 创建 LLM Provider
        let api_key = config.get_api_key()
            .ok_or_else(|| anyhow::anyhow!("No API key configured"))?;
        let api_base = config.get_api_base();

        let provider = Arc::new(OpenAIProvider::new(
            api_key,
            api_base,
            model,
        ));

        // 创建 Agent
        let agent = Arc::new(AgentLoop::new(
            bus.clone(),
            provider.clone(),
            workspace,
            Some(provider.get_default_model()),
            config.agents.defaults.max_tool_iterations,
            Some(brave_api_key),
        ).await?);

        // 创建渠道管理器（共享 `shared_config`）
        let channels = Arc::new(ChannelManager::new(shared_config.clone(), bus.clone()));
        channels.initialize().await?;

        // 创建定时任务服务
        let cron_store_path = stormclaw_utils::data_dir().join("cron").join("jobs.json");
        let cron = Arc::new(CronService::new(cron_store_path).await?);

        // 创建心跳服务
        let heartbeat_config = HeartbeatConfig {
            interval_seconds: 30 * 60,
            enabled: true,
            heartbeat_file: "HEARTBEAT.md".to_string(),
        };
        let heartbeat = Arc::new(HeartbeatService::new(
            config.workspace_path(),
            heartbeat_config,
        ));

        // 创建生命周期管理
        let lifecycle = Arc::new(ServiceLifecycle::new("gateway".to_string()));

        Ok(Self {
            config: gateway_config,
            shared_config,
            bus,
            agent,
            channels,
            cron,
            heartbeat,
            lifecycle,
            running: Arc::new(RwLock::new(false)),
            hot_reload: Mutex::new(None),
        })
    }

    /// 启动网关服务
    pub async fn start(&self) -> anyhow::Result<()> {
        let mut running = self.running.write().await;
        if *running {
            return Ok(());
        }

        tracing::info!("🐈 Starting stormclaw gateway on {}:{}", self.config.host, self.config.port);
        *running = true;

        // 发布启动事件
        self.lifecycle.publish(LifecycleEvent::Starting).await;

        // 设置 Cron 回调
        let agent_clone = self.agent.clone();
        let bus_clone = self.bus.clone();
        let cron_callback = Arc::new(move |job: &super::CronJob| {
            let agent = agent_clone.clone();
            let bus = bus_clone.clone();
            // 从借用的 job 提前拷贝出所有需要的数据，避免 Future 捕获引用导致生命周期问题
            let job_name = job.name.clone();
            let job_id = job.id.clone();
            let message = job.payload.message.clone();
            let deliver = job.payload.deliver;
            let channel = job
                .payload
                .channel
                .clone()
                .unwrap_or_else(|| "whatsapp".to_string());
            let to = job.payload.to.clone();
            let session_key = format!("cron:{}", job_id);

            let fut: std::pin::Pin<
                Box<
                    dyn std::future::Future<Output = anyhow::Result<Option<String>>> + Send,
                >,
            > = Box::pin(async move {
                tracing::info!("Executing cron job: {}", job_name);
                let response = agent.process_direct(&message, &session_key).await?;

                if deliver {
                    if let Some(to_chat_id) = to {
                        bus.publish_outbound(stormclaw_core::OutboundMessage::new(
                            channel,
                            to_chat_id,
                            response.clone(),
                        ))
                        .await?;
                    }
                }

                Ok(Some(response))
            });

            fut
        });
        self.cron.set_callback(cron_callback).await;

        // 设置 Heartbeat 回调
        let agent_clone = self.agent.clone();
        let heartbeat_callback = Arc::new(move |prompt: &str| {
            let agent = agent_clone.clone();
            let prompt = prompt.to_string();
            let fut: std::pin::Pin<
                Box<
                    dyn std::future::Future<Output = anyhow::Result<String>> + Send,
                >,
            > = Box::pin(async move {
                tracing::info!("Heartbeat triggered");
                // Python：session_key="heartbeat"
                agent.process_direct(&prompt, "heartbeat").await
            });

            fut
        });
        self.heartbeat.set_callback(heartbeat_callback).await;

        // 配置文件监视：校验通过后更新共享 Config、刷新 Agent 凭据/工具并重启渠道任务
        match HotReloadService::new(HotReloadConfig {
            enabled: true,
            check_interval_ms: 1000,
            ignore_patterns: vec![],
        }) {
            Ok(hot) => {
                let shared = self.shared_config.clone();
                let ch = self.channels.clone();
                let ag = self.agent.clone();
                let cb: ReloadCallback = Arc::new(move |_path: &std::path::PathBuf| {
                    let new_c = match stormclaw_config::load_config() {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::error!("Hot reload: load_config failed: {}", e);
                            return Ok(false);
                        }
                    };
                    let errors = ConfigValidator::validate(&new_c);
                    if !errors.is_empty() {
                        tracing::error!("Hot reload: validation failed: {:?}", errors);
                        return Ok(false);
                    }
                    let shared = shared.clone();
                    let ch = ch.clone();
                    let ag = ag.clone();
                    match tokio::runtime::Handle::try_current() {
                        Ok(h) => {
                            h.spawn(async move {
                                if let Err(e) =
                                    apply_gateway_config_hot_reload(shared, ch, ag, new_c).await
                                {
                                    tracing::error!("Hot reload apply: {}", e);
                                }
                            });
                        }
                        Err(_) => tracing::warn!("Hot reload: no tokio runtime, apply skipped"),
                    }
                    Ok(true)
                });
                hot.watch(get_config_path(), cb).await;
                match hot.start().await {
                    Ok(()) => {
                        tracing::info!("Watching config file for hot reload: {:?}", get_config_path());
                        *self.hot_reload.lock().expect("hot_reload mutex poisoned") = Some(hot);
                    }
                    Err(e) => tracing::warn!("Hot reload service failed to start: {}", e),
                }
            }
            Err(e) => tracing::warn!("Hot reload service init failed: {}", e),
        }

        // 启动所有服务
        let mut tasks = Vec::new();

        // 1. 启动 Agent 循环
        let agent = self.agent.clone();
        let running_flag = self.running.clone();
        tasks.push(tokio::spawn(async move {
            while *running_flag.read().await {
                if let Err(e) = agent.run().await {
                    tracing::error!("Agent loop error: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
        }));

        // 2. 启动定时任务服务
        let cron = self.cron.clone();
        tasks.push(tokio::spawn(async move {
            if let Err(e) = cron.start().await {
                tracing::error!("Cron service error: {}", e);
            }
        }));

        // 3. 启动心跳服务
        let heartbeat = self.heartbeat.clone();
        tasks.push(tokio::spawn(async move {
            if let Err(e) = heartbeat.start().await {
                tracing::error!("Heartbeat service error: {}", e);
            }
        }));

        // 4. 渠道：`stop_all`（如热重载）会使 `start_all` 返回，此处循环以便自动拉起
        let channels = self.channels.clone();
        let running_ch = self.running.clone();
        tasks.push(tokio::spawn(async move {
            while *running_ch.read().await {
                let n = channels.registered_channel_count().await;
                if n == 0 {
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    continue;
                }
                match channels.start_all().await {
                    Ok(()) => {
                        tracing::info!("Channel runner iteration finished; retrying after delay");
                    }
                    Err(e) => {
                        tracing::error!("Channels start_all error: {}", e);
                    }
                }
                if !*running_ch.read().await {
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }
        }));

        // 5. 启动 HTTP API (如果启用)
        if self.config.http_enabled {
            let http_task = self.start_http_api().await?;
            tasks.push(http_task);
        }

        // 发布启动完成事件
        self.lifecycle.publish(LifecycleEvent::Started).await;

        // 等待关闭信号
        let handle = self.shutdown_signal().await?;
        handle.await?;

        // 清理
        self.shutdown().await?;

        // 等待所有任务完成
        futures_util::future::join_all(tasks).await;

        Ok(())
    }

    /// 停止网关服务
    pub async fn shutdown(&self) -> anyhow::Result<()> {
        tracing::info!("Shutting down gateway...");

        *self.running.write().await = false;

        let hot_opt = {
            let mut g = self.hot_reload.lock().expect("hot_reload mutex poisoned");
            g.take()
        };
        if let Some(hot) = hot_opt {
            let _ = hot.stop().await;
        }

        // 停止各个服务
        self.cron.stop().await?;
        self.heartbeat.stop().await?;
        self.channels.stop_all().await?;
        self.agent.stop().await;

        self.lifecycle.publish(LifecycleEvent::Stopped).await;

        tracing::info!("Gateway shutdown complete");
        Ok(())
    }

    /// 获取状态
    pub async fn status(&self) -> GatewayStatus {
        let channel_status = self.channels.get_status().await;
        let cron_status = self.cron.status().await;
        let heartbeat_status = self.heartbeat.status().await;

        GatewayStatus {
            running: *self.running.read().await,
            uptime_seconds: self.lifecycle.uptime_seconds().await,
            channels: channel_status,
            cron: cron_status,
            heartbeat: heartbeat_status,
        }
    }

    /// 启动 HTTP API
    async fn start_http_api(&self) -> anyhow::Result<tokio::task::JoinHandle<()>> {
        // 创建应用状态
        let app_state = AppState {
            running: self.running.clone(),
            lifecycle: self.lifecycle.clone(),
            agent: self.agent.clone(),
            channels: self.channels.clone(),
            cron: self.cron.clone(),
            heartbeat: self.heartbeat.clone(),
        };

        let app = build_gateway_router(app_state);

        let addr = format!("{}:{}", self.config.host, self.config.port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;

        tracing::info!("HTTP API listening on {}", addr);

        Ok(tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        }))
    }

    /// 等待关闭信号
    async fn shutdown_signal(&self) -> anyhow::Result<tokio::task::JoinHandle<()>> {
        let running = self.running.clone();
        let lifecycle = self.lifecycle.clone();

        Ok(tokio::spawn(async move {
            // 等待 Ctrl+C
            let _ = signal::ctrl_c().await;

            tracing::info!("Received shutdown signal");

            *running.write().await = false;
            lifecycle.publish(LifecycleEvent::Stopping).await;
        }))
    }
}

async fn apply_gateway_config_hot_reload(
    shared: Arc<RwLock<Config>>,
    channels: Arc<ChannelManager>,
    agent: Arc<AgentLoop<OpenAIProvider>>,
    new_config: Config,
) -> anyhow::Result<()> {
    let api_key = new_config
        .get_api_key()
        .ok_or_else(|| anyhow::anyhow!("Reloaded config has no API key"))?;
    let api_base = new_config.get_api_base();
    let default_model = new_config.agents.defaults.model.clone();
    let max_it = new_config.agents.defaults.max_tool_iterations;
    let brave = Some(new_config.tools.web.search.api_key.clone())
        .filter(|s| !s.is_empty());
    let ws = new_config.workspace_path();

    *shared.write().await = new_config;

    agent
        .apply_gateway_hot_reload(api_key, api_base, default_model, max_it, brave, ws)
        .await?;

    channels.reload_channels().await?;

    tracing::info!("Configuration hot reload applied (channels restarting)");
    Ok(())
}

/// 应用状态
#[derive(Clone)]
struct AppState {
    running: Arc<RwLock<bool>>,
    lifecycle: Arc<ServiceLifecycle>,
    agent: Arc<AgentLoop<OpenAIProvider>>,
    channels: Arc<ChannelManager>,
    cron: Arc<CronService>,
    heartbeat: Arc<HeartbeatService>,
}

/// 组装网关 HTTP 路由（`with_state` 后为 `Router<()>`，可供 `serve` / oneshot 使用）。
#[cfg(feature = "gateway")]
fn build_gateway_router(app_state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/status", get(get_status))
        .route("/heartbeat/trigger", post(trigger_heartbeat))
        .route("/sessions", get(list_sessions))
        .route("/sessions/:id", get(get_session))
        .route("/sessions/:id/clear", post(clear_session))
        .route("/channels", get(list_channels))
        .route("/channels/:name/start", post(start_channel))
        .route("/channels/:name/stop", post(stop_channel))
        .route("/cron/jobs", get(list_cron_jobs))
        .route("/metrics", get(get_metrics))
        .with_state(app_state)
}

/// HTTP 处理器
async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    if *state.running.read().await {
        (StatusCode::OK, "OK").into_response()
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "Shutting down").into_response()
    }
}

async fn get_status(State(state): State<AppState>) -> impl IntoResponse {
    let channel_status = state.channels.get_status().await;
    let cron_status = state.cron.status().await;
    let heartbeat_status = state.heartbeat.status().await;

    let gateway_status = GatewayStatus {
        running: *state.running.read().await,
        uptime_seconds: state.lifecycle.uptime_seconds().await,
        channels: channel_status,
        cron: cron_status,
        heartbeat: heartbeat_status,
    };

    Json(gateway_status).into_response()
}

async fn trigger_heartbeat(State(state): State<AppState>) -> impl IntoResponse {
    match state.heartbeat.trigger_now().await {
        Ok(response) => {
            (StatusCode::OK, response).into_response()
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

// 会话管理
async fn list_sessions(State(state): State<AppState>) -> impl IntoResponse {
    let mgr = state.agent.session_manager();
    match mgr.list_sessions().await {
        Ok(list) => {
            let result: Vec<SessionInfo> = list.into_iter().map(|s| SessionInfo {
                id: s.key,
                created_at: s.created_at,
                updated_at: s.updated_at,
                message_count: s.message_count,
            }).collect();
            Json(result).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let mgr = state.agent.session_manager();
    match mgr.get(&id).await {
        Ok(Some(session)) => Json(session).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "session not found").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn clear_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let mgr = state.agent.session_manager();
    match mgr.get_or_create(&id).await {
        Ok(mut session) => {
            session.clear();
            match mgr.save(&session).await {
                Ok(_) => (StatusCode::OK, "OK").into_response(),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            }
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// 渠道管理
async fn list_channels(State(state): State<AppState>) -> impl IntoResponse {
    let status = state.channels.get_status().await;
    Json(status).into_response()
}

async fn start_channel(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match state.channels.get_channel(&name).await {
        Some(ch) => match ch.start().await {
            Ok(_) => (StatusCode::OK, "OK").into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        },
        None => (StatusCode::NOT_FOUND, format!("Unknown channel '{}'", name)).into_response(),
    }
}

async fn stop_channel(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match state.channels.get_channel(&name).await {
        Some(ch) => match ch.stop().await {
            Ok(_) => (StatusCode::OK, "OK").into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        },
        None => (StatusCode::NOT_FOUND, format!("Unknown channel '{}'", name)).into_response(),
    }
}

// 定时任务
async fn list_cron_jobs(State(state): State<AppState>) -> impl IntoResponse {
    let jobs = state.cron.list_jobs(true).await;
    Json(jobs).into_response()
}

fn format_gateway_prometheus_metrics(status: &GatewayStatus) -> String {
    let uptime = status.uptime_seconds as f64;
    format!(
        "# HELP stormclaw_up Whether the gateway is running\n\
         # TYPE stormclaw_up gauge\n\
         stormclaw_up {}\n\
         # HELP stormclaw_uptime_seconds Gateway uptime in seconds\n\
         # TYPE stormclaw_uptime_seconds gauge\n\
         stormclaw_uptime_seconds {}\n\
         # HELP stormclaw_channels_total Total number of channels\n\
         # TYPE stormclaw_channels_total gauge\n\
         stormclaw_channels_total {}\n\
         # HELP stormclaw_cron_jobs_total Total number of cron jobs\n\
         # TYPE stormclaw_cron_jobs_total gauge\n\
         stormclaw_cron_jobs_total {}\n\
         # HELP stormclaw_cron_jobs_enabled Number of enabled cron jobs\n\
         # TYPE stormclaw_cron_jobs_enabled gauge\n\
         stormclaw_cron_jobs_enabled {}\n",
        if status.running { 1 } else { 0 },
        uptime,
        status.channels.len(),
        status.cron.total_jobs,
        status.cron.enabled_jobs,
    )
}

// 指标 (Prometheus 格式)
async fn get_metrics(State(state): State<AppState>) -> impl IntoResponse {
    let channel_status = state.channels.get_status().await;
    let cron_status = state.cron.status().await;
    let heartbeat_status = state.heartbeat.status().await;
    let status = GatewayStatus {
        running: *state.running.read().await,
        uptime_seconds: state.lifecycle.uptime_seconds().await,
        channels: channel_status,
        cron: cron_status,
        heartbeat: heartbeat_status,
    };
    let metrics = format_gateway_prometheus_metrics(&status);

    (StatusCode::OK, [("content-type", "text/plain")], metrics).into_response()
}

/// 网关状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayStatus {
    pub running: bool,
    pub uptime_seconds: u64,
    pub channels: HashMap<String, stormclaw_channels::ChannelStatus>,
    pub cron: super::CronServiceStatus,
    pub heartbeat: super::HeartbeatStatus,
}

/// 会话信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub message_count: usize,
}

/// 网关事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GatewayEvent {
    ClientConnected {
        client_id: String,
        channel: String,
    },
    ClientDisconnected {
        client_id: String,
    },
    MessageReceived {
        channel: String,
        chat_id: String,
    },
    MessageSent {
        channel: String,
        chat_id: String,
    },
}

/// 网关统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayStats {
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub messages_processed: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub active_channels: Vec<String>,
    pub uptime_seconds: u64,
}

impl GatewayStats {
    pub fn new() -> Self {
        Self {
            start_time: chrono::Utc::now(),
            messages_processed: 0,
            messages_sent: 0,
            messages_received: 0,
            active_channels: Vec::new(),
            uptime_seconds: 0,
        }
    }
}

impl Default for GatewayStats {
    fn default() -> Self {
        Self::new()
    }
}

/// 网关 builder
///
/// 提供链式 API 构建网关
pub struct GatewayBuilder {
    config: Option<Config>,
    gateway_config: Option<GatewayConfig>,
}

impl GatewayBuilder {
    pub fn new() -> Self {
        Self {
            config: None,
            gateway_config: None,
        }
    }

    pub fn with_config(mut self, config: Config) -> Self {
        self.config = Some(config);
        self
    }

    pub fn with_gateway_config(mut self, config: GatewayConfig) -> Self {
        self.gateway_config = Some(config);
        self
    }

    pub async fn build(self) -> anyhow::Result<GatewayService> {
        let config = self.config.unwrap_or_default();
        let gateway_config = self.gateway_config.unwrap_or_default();

        GatewayService::new(config, gateway_config).await
    }
}

impl Default for GatewayBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            http_enabled: default_http_enabled(),
            metrics_enabled: default_metrics_enabled(),
        }
    }
}

#[cfg(all(test, feature = "gateway"))]
mod gateway_http_smoke_tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use stormclaw_config::Config;
    use tower::ServiceExt;
    use crate::{CronService, CronServiceStatus, HeartbeatConfig, HeartbeatService, HeartbeatStatus};

    async fn smoke_test_router() -> Router {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.agents.defaults.workspace = Some(dir.path().to_string_lossy().into_owned());
        config.providers.openai.api_key = Some("sk-test".into());

        let bus = Arc::new(MessageBus::new(1000));
        let workspace = config.workspace_path();
        let provider = Arc::new(OpenAIProvider::new(
            "sk-test".into(),
            None,
            "gpt-4".into(),
        ));
        let agent = Arc::new(
            AgentLoop::new(
                bus.clone(),
                provider.clone(),
                workspace.clone(),
                Some("gpt-4".into()),
                4,
                None,
            )
            .await
            .expect("agent"),
        );

        let shared = Arc::new(RwLock::new(config.clone()));
        let channels = Arc::new(ChannelManager::new(shared, bus.clone()));
        channels.initialize().await.expect("channels");

        let cron_path = dir.path().join("cron").join("jobs.json");
        std::fs::create_dir_all(cron_path.parent().unwrap()).unwrap();
        let cron = Arc::new(CronService::new(cron_path).await.unwrap());

        let heartbeat_config = HeartbeatConfig {
            interval_seconds: 30 * 60,
            enabled: true,
            heartbeat_file: "HEARTBEAT.md".into(),
        };
        let heartbeat = Arc::new(HeartbeatService::new(workspace, heartbeat_config));

        let app_state = AppState {
            running: Arc::new(RwLock::new(true)),
            lifecycle: Arc::new(ServiceLifecycle::new("gateway".into())),
            agent,
            channels,
            cron,
            heartbeat,
        };

        build_gateway_router(app_state)
    }

    #[tokio::test]
    async fn health_oneshot_returns_ok() {
        let app = smoke_test_router().await;
        let req = Request::builder().uri("/health").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), b"OK");
    }

    #[tokio::test]
    async fn metrics_oneshot_contains_stormclaw_up() {
        let app = smoke_test_router().await;
        let req = Request::builder().uri("/metrics").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let s = String::from_utf8(body.to_vec()).unwrap();
        assert!(s.contains("stormclaw_up"));
    }

    #[test]
    fn format_prometheus_metrics_contains_stormclaw_up_line() {
        let status = GatewayStatus {
            running: true,
            uptime_seconds: 42,
            channels: HashMap::new(),
            cron: CronServiceStatus {
                enabled: false,
                total_jobs: 2,
                enabled_jobs: 1,
                next_wake_at_ms: None,
            },
            heartbeat: HeartbeatStatus {
                enabled: false,
                last_check_at: None,
                last_action_at: None,
                checks_performed: 0,
                actions_taken: 0,
            },
        };
        let s = format_gateway_prometheus_metrics(&status);
        assert!(s.contains("stormclaw_up 1"));
        assert!(s.contains("stormclaw_uptime_seconds 42"));
        assert!(s.contains("stormclaw_cron_jobs_total 2"));
    }
}
