//! 渠道管理器
//!
//! 协调多个聊天渠道，管理消息分发

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use stormclaw_core::MessageBus;
use stormclaw_config::Config;
use super::{BaseChannel, ChannelFactory};

/// 渠道管理器
///
/// 负责初始化和管理所有启用的聊天渠道
#[derive(Clone)]
pub struct ChannelManager {
    channels: Arc<RwLock<HashMap<String, Arc<dyn BaseChannel>>>>,
    bus: Arc<MessageBus>,
    /// 与 Gateway 共享；热重载时 `write` 替换整份配置
    config: Arc<RwLock<Config>>,
    dispatch_task: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl ChannelManager {
    /// 创建新的渠道管理器（`config` 与 Gateway 共用同一 `Arc<RwLock<Config>>`）
    pub fn new(config: Arc<RwLock<Config>>, bus: Arc<MessageBus>) -> Self {
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
            bus,
            config,
            dispatch_task: Arc::new(RwLock::new(None)),
        }
    }

    /// 当前内存中的配置快照（读锁）
    pub async fn channels_config_snapshot(&self) -> stormclaw_config::ChannelsConfig {
        self.config.read().await.channels.clone()
    }

    /// 初始化所有已启用的渠道
    pub async fn initialize(&self) -> anyhow::Result<()> {
        tracing::info!("Initializing channels...");

        let mut channels = self.channels.write().await;
        let cfg = self.config.read().await;

        // Telegram 渠道
        if cfg.channels.telegram.enabled && !cfg.channels.telegram.token.is_empty() {
            match ChannelFactory::create_telegram(&cfg.channels.telegram, self.bus.clone()) {
                Ok(channel) => {
                    tracing::info!("✓ Telegram channel enabled");
                    channels.insert("telegram".to_string(), channel);
                }
                Err(e) => {
                    tracing::warn!("Failed to initialize Telegram channel: {}", e);
                }
            }
        }

        // WhatsApp 渠道
        if cfg.channels.whatsapp.enabled {
            match ChannelFactory::create_whatsapp(&cfg.channels.whatsapp, self.bus.clone()) {
                Ok(channel) => {
                    tracing::info!("✓ WhatsApp channel enabled");
                    channels.insert("whatsapp".to_string(), channel);
                }
                Err(e) => {
                    tracing::warn!("Failed to initialize WhatsApp channel: {}", e);
                }
            }
        }

        if channels.is_empty() {
            tracing::warn!("No channels enabled");
        }

        Ok(())
    }

    /// 配置热重载：停止渠道、清空实例并按当前 `config` 重新 `initialize`（由外层循环再次 `start_all`）
    pub async fn reload_channels(&self) -> anyhow::Result<()> {
        self.stop_all().await?;
        {
            let mut map = self.channels.write().await;
            map.clear();
        }
        self.initialize().await
    }

    /// 已注册渠道数量（用于调度循环在空配置时等待）
    pub async fn registered_channel_count(&self) -> usize {
        self.channels.read().await.len()
    }

    /// 启动所有渠道
    pub async fn start_all(&self) -> anyhow::Result<()> {
        let channels = self.channels.read().await;

        if channels.is_empty() {
            anyhow::bail!("No channels to start");
        }

        tracing::info!("Starting {} channel(s)...", channels.len());

        // 启动出站消息分发器
        let dispatch_handle = self.start_outbound_dispatcher().await?;
        *self.dispatch_task.write().await = Some(dispatch_handle);

        // 启动所有渠道
        let mut handles = Vec::new();

        for (name, channel) in channels.iter() {
            let channel = channel.clone();
            let name = name.clone();

            let handle = tokio::spawn(async move {
                tracing::info!("Starting {} channel...", name);

                if let Err(e) = channel.start().await {
                    tracing::error!("{} channel error: {}", name, e);
                }

                tracing::info!("{} channel stopped", name);
            });

            handles.push(handle);
        }

        // 等待所有渠道完成（它们应该一直运行）
        futures::future::join_all(handles).await;

        Ok(())
    }

    /// 停止所有渠道
    pub async fn stop_all(&self) -> anyhow::Result<()> {
        tracing::info!("Stopping all channels...");

        // 停止分发器
        if let Some(task) = self.dispatch_task.write().await.take() {
            task.abort();
        }

        // 停止所有渠道
        let channels = self.channels.read().await;

        for (name, channel) in channels.iter() {
            if let Err(e) = channel.stop().await {
                tracing::error!("Error stopping {} channel: {}", name, e);
            }
        }

        Ok(())
    }

    /// 启动出站消息分发器
    async fn start_outbound_dispatcher(&self) -> anyhow::Result<tokio::task::JoinHandle<()>> {
        let bus = self.bus.clone();
        let channels = self.channels.clone();

        let handle = tokio::spawn(async move {
            tracing::info!("Outbound dispatcher started");

            while let Some(msg) = bus.consume_outbound().await {
                let channels_guard = channels.read().await;
                if let Some(channel) = channels_guard.get(&msg.channel) {
                    if let Err(e) = channel.send(&msg).await {
                        tracing::error!("Error sending to {}: {}", msg.channel, e);
                    }
                } else {
                    tracing::warn!("Unknown channel: {}", msg.channel);
                }
            }

            tracing::info!("Outbound dispatcher stopped");
        });

        Ok(handle)
    }

    /// 获取指定渠道
    pub async fn get_channel(&self, name: &str) -> Option<Arc<dyn BaseChannel>> {
        let channels = self.channels.read().await;
        channels.get(name).cloned()
    }

    /// 获取所有渠道状态
    pub async fn get_status(&self) -> HashMap<String, ChannelStatus> {
        let channels = self.channels.read().await;
        let ch_cfg = self.config.read().await.channels.clone();
        let mut status = HashMap::new();

        for (name, channel) in channels.iter() {
            status.insert(name.clone(), ChannelStatus {
                enabled: true,
                running: channel.is_running(),
            });
        }

        // 添加未启用的渠道
        if !status.contains_key("telegram") {
            status.insert("telegram".to_string(), ChannelStatus {
                enabled: ch_cfg.telegram.enabled,
                running: false,
            });
        }

        if !status.contains_key("whatsapp") {
            status.insert("whatsapp".to_string(), ChannelStatus {
                enabled: ch_cfg.whatsapp.enabled,
                running: false,
            });
        }

        status
    }

    /// 获取已启用的渠道列表
    pub async fn enabled_channels(&self) -> Vec<String> {
        let channels = self.channels.read().await;
        channels.keys().cloned().collect()
    }
}

/// 渠道状态
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChannelStatus {
    pub enabled: bool,
    pub running: bool,
}
