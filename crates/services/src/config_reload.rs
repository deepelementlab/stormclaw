//! 配置热重载服务 (Config Hot Reload Service)
//!
//! 监控配置文件变化并自动重载

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use notify::{RecursiveMode, Watcher, EventKind};
use serde::Deserialize;

/// 配置热重载配置
#[derive(Debug, Clone, Deserialize)]
pub struct HotReloadConfig {
    /// 是否启用
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// 检查间隔（毫秒）
    #[serde(default = "default_interval")]
    pub check_interval_ms: u64,

    /// 要忽略的文件
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
}

fn default_enabled() -> bool {
    true
}

fn default_interval() -> u64 {
    1000 // 1 秒
}

/// 配置重载回调
pub type ReloadCallback = Arc<dyn Fn(&PathBuf) -> anyhow::Result<bool> + Send + Sync>;

/// 配置热重载服务
pub struct HotReloadService {
    config: HotReloadConfig,
    watchers: Arc<RwLock<HashMap<PathBuf, ReloadCallback>>>,
    running: Arc<RwLock<bool>>,
}

impl HotReloadService {
    /// 创建新的热重载服务
    pub fn new(config: HotReloadConfig) -> anyhow::Result<Self> {
        Ok(Self {
            config,
            watchers: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
        })
    }

    /// 注册要监视的文件
    pub async fn watch(&self, path: PathBuf, callback: ReloadCallback) {
        let mut watchers = self.watchers.write().await;
        watchers.insert(path, callback);
    }

    /// 启动服务
    pub async fn start(&self) -> anyhow::Result<()> {
        if !self.config.enabled {
            tracing::info!("Hot reload disabled");
            return Ok(());
        }

        let mut running = self.running.write().await;
        if *running {
            return Ok(());
        }

        tracing::info!("Starting hot reload service");
        *running = true;

        // 收集所有要监视的路径
        let paths: Vec<PathBuf> = {
            let watchers = self.watchers.read().await;
            watchers.keys().cloned().collect()
        };

        if paths.is_empty() {
            tracing::warn!("No files to watch");
            return Ok(());
        }

        let running_flag = self.running.clone();
        let watchers_clone = self.watchers.clone();
        let ignore_patterns = self.config.ignore_patterns.clone();

        tokio::spawn(async move {
            let mut targets = Vec::new();

            for path in paths {
                let parent = path.parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."));

                targets.push((parent, path.clone()));
            }

            let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<notify::Result<notify::Event>>();
            let mut watcher = match notify::recommended_watcher(move |res| {
                // notify 的回调是同步的；这里将事件推送到 async 侧处理
                let _ = event_tx.send(res);
            }) {
                Ok(w) => w,
                Err(e) => {
                    tracing::error!("Failed to create watcher: {}", e);
                    return;
                }
            };

            // 配置监视器
            for (parent, _path) in targets {
                if let Err(e) = watcher.watch(&parent, RecursiveMode::NonRecursive) {
                    tracing::warn!("Failed to watch {:?}: {}", parent, e);
                }
            }

            while *running_flag.read().await {
                let maybe = tokio::time::timeout(
                    tokio::time::Duration::from_millis(250),
                    event_rx.recv(),
                )
                .await;

                let res = match maybe {
                    Ok(Some(res)) => res,
                    Ok(None) => break, // sender dropped
                    Err(_) => continue, // timeout
                };

                let event = match res {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::warn!("Watch error: {}", e);
                        continue;
                    }
                };

                // 仅处理修改事件
                if !matches!(event.kind, EventKind::Modify(_)) {
                    continue;
                }

                for changed in event.paths {
                    // 简单忽略（基于路径字符串包含），避免引入 regex/glob 的额外复杂度
                    let changed_s = changed.to_string_lossy();
                    if ignore_patterns.iter().any(|p| !p.is_empty() && changed_s.contains(p)) {
                        continue;
                    }

                    // 检查是否在监视列表中
                    let callback = {
                        let watchers = watchers_clone.read().await;
                        watchers.get(&changed).cloned()
                    };

                    if let Some(callback) = callback {
                        tracing::debug!("Config file changed: {:?}", changed);
                        match callback(&changed) {
                            Ok(true) => tracing::info!("Config reloaded: {:?}", changed),
                            Ok(false) => tracing::debug!("Config reload skipped: {:?}", changed),
                            Err(e) => tracing::error!("Config reload error: {:?}", e),
                        }
                    }
                }
            }

            tracing::info!("Hot reload service stopped");
        });

        Ok(())
    }

    /// 停止服务
    pub async fn stop(&self) -> anyhow::Result<()> {
        tracing::info!("Stopping hot reload service");
        *self.running.write().await = false;
        Ok(())
    }
}

/// 配置重载器
///
/// 提供配置重载的通用逻辑
pub struct ConfigReloader;

impl ConfigReloader {
    /// 创建主配置重载回调
    pub fn main_config_callback() -> ReloadCallback {
        Arc::new(|path: &PathBuf| -> anyhow::Result<bool> {
            tracing::info!("Reloading main config from {:?}", path);

            // 重新加载配置
            let new_config = stormclaw_config::load_config()?;

            let errors = ConfigValidator::validate(&new_config);
            if !errors.is_empty() {
                tracing::error!(
                    "Reloaded config from {:?} failed validation: {:?}",
                    path,
                    errors
                );
                return Ok(false);
            }

            tracing::info!(
                "Config file {:?} was reloaded and validated. \
                 Standalone processes still hold old in-memory state — restart them, or use `stormclaw gateway` which applies changes automatically.",
                path
            );

            Ok(true)
        })
    }

    /// 创建渠道配置重载回调
    pub fn channel_config_callback() -> ReloadCallback {
        Arc::new(|path: &PathBuf| -> anyhow::Result<bool> {
            tracing::info!("Reloading channel config from {:?}", path);

            let new_config = stormclaw_config::load_config()?;
            let errors = ConfigValidator::validate(&new_config);
            if !errors.is_empty() {
                tracing::error!("Channel config reload validation failed: {:?}", errors);
                return Ok(false);
            }
            tracing::info!(
                "Channel-related config was read from {:?}. \
                 Non-gateway processes must restart to pick up changes.",
                path
            );

            Ok(true)
        })
    }

    /// 创建工具配置重载回调
    pub fn tools_config_callback() -> ReloadCallback {
        Arc::new(|path: &PathBuf| -> anyhow::Result<bool> {
            tracing::info!("Reloading tools config from {:?}", path);

            let new_config = stormclaw_config::load_config()?;
            let errors = ConfigValidator::validate(&new_config);
            if !errors.is_empty() {
                tracing::error!("Tools config reload validation failed: {:?}", errors);
                return Ok(false);
            }
            tracing::warn!(
                "Tools config was read from {:?}; Agent tool registry is not updated at runtime — restart to apply.",
                path
            );

            Ok(true)
        })
    }
}

/// 配置验证器
pub struct ConfigValidator;

impl ConfigValidator {
    /// 验证配置是否有效
    pub fn validate(config: &stormclaw_config::Config) -> Vec<String> {
        let mut errors = Vec::new();

        // 检查 API Key
        if config.get_api_key().is_none() {
            errors.push("No API key configured".to_string());
        }

        // 检查模型
        let model = &config.agents.defaults.model;
        if model.is_empty() {
            errors.push("Model name is empty".to_string());
        }

        // 检查启用的渠道
        if config.channels.telegram.enabled && config.channels.telegram.token.is_empty() {
            errors.push("Telegram enabled but token is empty".to_string());
        }

        if config.channels.whatsapp.enabled {
            if config.channels.whatsapp.bridge_url.is_empty() {
                errors.push("WhatsApp enabled but bridge_url is empty".to_string());
            }
        }

        errors
    }

    /// 验证配置并返回结果
    pub fn validate_and_report(config: &stormclaw_config::Config) -> anyhow::Result<()> {
        let errors = Self::validate(config);

        if !errors.is_empty() {
            anyhow::bail!("Config validation failed:\n{}", errors.join("\n"));
        }

        Ok(())
    }
}
