//! 渠道监控模块
//!
//! 提供渠道状态监控和健康检查功能

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use stormclaw_core::{MessageBus, InboundMessage, OutboundMessage};

/// 渠道健康状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelHealth {
    pub name: String,
    pub status: HealthStatus,
    pub latency_ms: Option<u64>,
    pub last_message_at: Option<chrono::DateTime<chrono::Utc>>,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub error_count: u64,
    pub last_error: Option<String>,
}

/// 健康状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

/// 渠道监控器
pub struct ChannelMonitor {
    channels: Arc<RwLock<HashMap<String, ChannelHealth>>>,
    bus: Arc<MessageBus>,
}

impl ChannelMonitor {
    pub fn new(bus: Arc<MessageBus>) -> Self {
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
            bus,
        }
    }

    /// 注册要监控的渠道
    pub async fn register_channel(&self, name: String) {
        let mut channels = self.channels.write().await;
        channels.entry(name.clone()).or_insert_with(|| ChannelHealth {
            name,
            status: HealthStatus::Unknown,
            latency_ms: None,
            last_message_at: None,
            messages_sent: 0,
            messages_received: 0,
            error_count: 0,
            last_error: None,
        });
    }

    /// 记录入站消息
    pub async fn log_inbound(&self, channel: &str, msg: &InboundMessage) {
        let mut channels = self.channels.write().await;
        if let Some(health) = channels.get_mut(channel) {
            health.messages_received += 1;
            health.last_message_at = Some(msg.timestamp);
            health.status = HealthStatus::Healthy;
            health.error_count = 0;
            health.last_error = None;
        }
    }

    /// 记录出站消息
    pub async fn log_outbound(&self, channel: &str, msg: &OutboundMessage) {
        let mut channels = self.channels.write().await;
        if let Some(health) = channels.get_mut(channel) {
            health.messages_sent += 1;
        }
    }

    /// 记录错误
    pub async fn log_error(&self, channel: &str, error: &str) {
        let mut channels = self.channels.write().await;
        if let Some(health) = channels.get_mut(channel) {
            health.error_count += 1;
            health.last_error = Some(error.to_string());

            // 根据错误数量确定状态
            health.status = match health.error_count {
                0 => HealthStatus::Healthy,
                1..=5 => HealthStatus::Degraded,
                _ => HealthStatus::Unhealthy,
            };
        }
    }

    /// 更新延迟
    pub async fn update_latency(&self, channel: &str, latency: Duration) {
        let mut channels = self.channels.write().await;
        if let Some(health) = channels.get_mut(channel) {
            health.latency_ms = Some(latency.as_millis() as u64);
        }
    }

    /// 获取所有渠道健康状态
    pub async fn get_all_health(&self) -> HashMap<String, ChannelHealth> {
        self.channels.read().await.clone()
    }

    /// 获取特定渠道健康状态
    pub async fn get_health(&self, channel: &str) -> Option<ChannelHealth> {
        self.channels.read().await.get(channel).cloned()
    }

    /// 检查所有渠道是否健康
    pub async fn is_healthy(&self) -> bool {
        let channels = self.channels.read().await;
        channels.values().all(|h| matches!(h.status, HealthStatus::Healthy))
    }

    /// 获取不健康的渠道列表
    pub async fn get_unhealthy_channels(&self) -> Vec<String> {
        let channels = self.channels.read().await;
        channels
            .iter()
            .filter(|(_, h)| !matches!(h.status, HealthStatus::Healthy))
            .map(|(name, _)| name.clone())
            .collect()
    }
}

/// 渠道统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelStats {
    pub name: String,
    pub uptime_seconds: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub average_latency_ms: f64,
    pub error_rate: f64,
}

/// 渠道统计收集器
pub struct ChannelStatsCollector {
    start_times: Arc<RwLock<HashMap<String, Instant>>>,
    latencies: Arc<RwLock<HashMap<String, Vec<Duration>>>>,
    errors: Arc<RwLock<HashMap<String, u64>>>,
}

impl ChannelStatsCollector {
    pub fn new() -> Self {
        Self {
            start_times: Arc::new(RwLock::new(HashMap::new())),
            latencies: Arc::new(RwLock::new(HashMap::new())),
            errors: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 记录渠道启动
    pub async fn record_start(&self, channel: &str) {
        let mut start_times = self.start_times.write().await;
        start_times.insert(channel.to_string(), Instant::now());
    }

    /// 记录延迟
    pub async fn record_latency(&self, channel: &str, latency: Duration) {
        let mut latencies = self.latencies.write().await;
        latencies
            .entry(channel.to_string())
            .or_insert_with(Vec::new)
            .push(latency);

        // 保留最近 100 个样本
        if let Some(samples) = latencies.get_mut(channel) {
            if samples.len() > 100 {
                samples.remove(0);
            }
        }
    }

    /// 记录错误
    pub async fn record_error(&self, channel: &str) {
        let mut errors = self.errors.write().await;
        *errors.entry(channel.to_string()).or_insert(0) += 1;
    }

    /// 获取渠道统计
    pub async fn get_stats(&self, channel: &str, health: &ChannelHealth) -> ChannelStats {
        let start_times = self.start_times.read().await;
        let latencies = self.latencies.read().await;
        let errors = self.errors.read().await;

        let uptime = start_times
            .get(channel)
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0);

        let avg_latency = latencies
            .get(channel)
            .map(|samples| {
                if samples.is_empty() {
                    0.0
                } else {
                    let total: u64 = samples.iter().map(|d| d.as_millis() as u64).sum();
                    total as f64 / samples.len() as f64
                }
            })
            .unwrap_or(0.0);

        let total_messages = health.messages_sent + health.messages_received;
        let error_count = *errors.get(channel).unwrap_or(&0);
        let error_rate = if total_messages > 0 {
            error_count as f64 / total_messages as f64
        } else {
            0.0
        };

        ChannelStats {
            name: channel.to_string(),
            uptime_seconds: uptime,
            messages_sent: health.messages_sent,
            messages_received: health.messages_received,
            average_latency_ms: avg_latency,
            error_rate,
        }
    }
}

impl Default for ChannelStatsCollector {
    fn default() -> Self {
        Self::new()
    }
}
