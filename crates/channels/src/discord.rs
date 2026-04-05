//! Discord 渠道实现
//!
//! 通过 Discord Bot API 集成

use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::RwLock;
use stormclaw_core::{MessageBus, OutboundMessage, InboundMessage};
use super::{BaseChannel, ChannelState};
use chrono::Utc;

#[cfg(feature = "discord-gateway")]
use serenity::{
    client::{Client, Context, EventHandler},
    model::{channel::Message, gateway::Ready},
    async_trait as serenity_async_trait,
};

/// Discord 事件处理器 (Gateway 模式)
#[cfg(feature = "discord-gateway")]
struct DiscordEventHandler {
    bus: Arc<MessageBus>,
    allow_from: Vec<String>,
}

#[cfg(feature = "discord-gateway")]
#[serenity_async_trait]
impl EventHandler for DiscordEventHandler {
    async fn message(&self, ctx: Context, msg: Message) {
        // 忽略机器人消息
        if msg.author.bot {
            return;
        }

        // 检查权限
        let guild_id = msg.guild_id.map(|g| g.to_string()).unwrap_or_default();
        let user_id = msg.author.id.to_string();

        let allowed = if !self.allow_from.is_empty() {
            self.allow_from.contains(&guild_id) || self.allow_from.contains(&user_id)
        } else {
            true
        };

        if !allowed {
            return;
        }

        // 创建入站消息
        let inbound = InboundMessage {
            channel: "discord".to_string(),
            sender_id: user_id.clone(),
            chat_id: msg.channel_id.to_string(),
            content: msg.content,
            timestamp: Utc::now(),
            media: Vec::new(),
            metadata: serde_json::json!({
                "guild_id": guild_id,
                "username": msg.author.name,
                "discriminator": msg.author.discriminator,
            }),
        };

        if let Err(e) = self.bus.publish_inbound(inbound).await {
            tracing::error!("Failed to publish Discord message: {}", e);
        }
    }

    async fn ready(&self, _ctx: Context, ready: Ready) {
        tracing::info!("Discord Gateway connected as {}", ready.user.name);
    }
}

/// Discord 渠道配置
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct DiscordConfig {
    pub enabled: bool,
    pub token: String,
    pub allow_from: Vec<String>, // Guild IDs or User IDs
    pub command_prefix: String,
}

impl Default for DiscordConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            token: String::new(),
            allow_from: Vec::new(),
            command_prefix: "!".to_string(),
        }
    }
}

/// Discord 渠道
pub struct DiscordChannel {
    config: DiscordConfig,
    bus: Arc<MessageBus>,
    state: Arc<RwLock<ChannelState>>,
    running: Arc<AtomicBool>,
    // Serenity client would be stored here
    // For now, we use a simpler approach with HTTP API
    client: Arc<reqwest::Client>,
    message_count: Arc<AtomicU64>,
}

impl DiscordChannel {
    pub fn new(
        config: DiscordConfig,
        bus: Arc<MessageBus>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            config,
            bus,
            state: Arc::new(RwLock::new(ChannelState::default())),
            running: Arc::new(AtomicBool::new(false)),
            client: Arc::new(
                reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(30))
                    .build()?
            ),
            message_count: Arc::new(AtomicU64::new(0)),
        })
    }

    /// 获取 Discord API 基础 URL
    fn api_base(&self) -> &str {
        "https://discord.com/api/v10"
    }

    /// 发送 HTTP 请求到 Discord API
    async fn discord_request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> anyhow::Result<serde_json::Value> {
        let url = format!("{}{}", self.api_base(), path);

        let mut request = self.client
            .request(method, &url)
            .header("Authorization", format!("Bot {}", self.config.token))
            .header("Content-Type", "application/json");

        if let Some(body) = body {
            request = request.json(&body);
        }

        let response = request.send().await?;

        if response.status().is_success() {
            response.json().await.map_err(Into::into)
        } else {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Discord API error ({}): {}", status, error_text)
        }
    }

    /// 处理收到的消息
    async fn handle_message(&self, channel_id: &str, author_id: &str, content: &str) -> anyhow::Result<()> {
        // 检查权限
        if !self.is_allowed(author_id) {
            tracing::debug!("Discord message from unauthorized user: {}", author_id);
            return Ok(());
        }

        // 忽略机器人消息和命令
        if content.starts_with(&self.config.command_prefix) {
            return Ok(());
        }

        // 创建入站消息
        let inbound = InboundMessage {
            channel: "discord".to_string(),
            sender_id: author_id.to_string(),
            chat_id: channel_id.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            media: Vec::new(),
            metadata: serde_json::json!({}),
        };

        // 发布到消息总线
        self.bus.publish_inbound(inbound).await?;

        // 更新计数
        self.message_count.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// 获取当前用户信息
    pub async fn get_current_user(&self) -> anyhow::Result<DiscordUser> {
        let response = self.discord_request(
            reqwest::Method::GET,
            "/users/@me",
            None,
        ).await?;

        Ok(serde_json::from_value(response)?)
    }

    /// 获取 guilds 信息
    pub async fn get_guilds(&self) -> anyhow::Result<Vec<DiscordGuild>> {
        let response = self.discord_request(
            reqwest::Method::GET,
            "/users/@me/guilds",
            None,
        ).await?;

        Ok(serde_json::from_value(response)?)
    }

    /// 获取频道消息（用于调试）
    pub async fn get_channel_messages(&self, channel_id: &str, limit: u64) -> anyhow::Result<Vec<DiscordMessage>> {
        let response = self.discord_request(
            reqwest::Method::GET,
            &format!("/channels/{}/messages?limit={}", channel_id, limit),
            None,
        ).await?;

        Ok(serde_json::from_value(response)?)
    }

    #[cfg(feature = "discord-gateway")]
    async fn start_gateway(&self) -> anyhow::Result<()> {
        // 验证 token
        match self.get_current_user().await {
            Ok(user) => {
                tracing::info!("Connecting to Discord Gateway as {} ({})", user.username, user.id);
            }
            Err(e) => {
                tracing::error!("Failed to validate Discord token: {}", e);
                return Err(e);
            }
        }

        // 创建 Gateway 客户端
        let handler = DiscordEventHandler {
            bus: self.bus.clone(),
            allow_from: self.config.allow_from.clone(),
        };

        let mut client = Client::builder(&self.config.token, serenity::all())
            .event_handler(handler)
            .await?;

        // 启动 Gateway
        let manager = client.start().await?;

        self.running.store(true, Ordering::Relaxed);
        {
            let mut state = self.state.write().await;
            state.is_running = true;
            state.connected_at = Some(Utc::now());
        }

        tracing::info!("Discord Gateway started successfully");

        // 保存 manager 以便之后关闭
        // 注意：实际实现中应该存储 manager 以便在 stop() 时使用
        std::mem::forget(manager);

        Ok(())
    }

    #[cfg(not(feature = "discord-gateway"))]
    async fn start_http_only(&self) -> anyhow::Result<()> {
        // 验证 token
        match self.get_current_user().await {
            Ok(user) => {
                tracing::info!("Connected to Discord HTTP API as {} ({})", user.username, user.id);
            }
            Err(e) => {
                tracing::error!("Failed to connect to Discord: {}", e);
                return Err(e);
            }
        }

        self.running.store(true, Ordering::Relaxed);
        {
            let mut state = self.state.write().await;
            state.is_running = true;
            state.connected_at = Some(Utc::now());
        }

        // Discord 使用 Gateway 进行实时通信
        // 完整实现需要使用 serenity 库和 WebSocket 连接
        // 这里提供一个基础框架，实际生产环境建议使用 serenity

        tracing::info!("Discord channel started (HTTP-only mode)");
        tracing::warn!("Note: Full Discord integration requires 'discord-gateway' feature. Reception is not available in HTTP-only mode.");

        Ok(())
    }
}

#[async_trait]
impl BaseChannel for DiscordChannel {
    fn name(&self) -> &str {
        "discord"
    }

    async fn start(&self) -> anyhow::Result<()> {
        if self.running.load(Ordering::Relaxed) {
            return Ok(());
        }

        tracing::info!("Starting Discord channel");

        #[cfg(feature = "discord-gateway")]
        {
            // 使用 Gateway 模式
            self.start_gateway().await
        }

        #[cfg(not(feature = "discord-gateway"))]
        {
            // 使用 HTTP API 模式（仅发送，不接收）
            self.start_http_only().await
        }
    }

    async fn stop(&self) -> anyhow::Result<()> {
        tracing::info!("Stopping Discord channel");
        self.running.store(false, Ordering::Relaxed);

        let mut state = self.state.write().await;
        state.is_running = false;

        Ok(())
    }

    async fn send(&self, msg: &OutboundMessage) -> anyhow::Result<()> {
        tracing::debug!("Sending Discord message to {}: {}", msg.chat_id, msg.content);

        // Discord 消息格式
        let discord_msg = serde_json::json!({
            "content": msg.content,
        });

        self.discord_request(
            reqwest::Method::POST,
            &format!("/channels/{}/messages", msg.chat_id),
            Some(discord_msg),
        ).await?;

        // 更新统计
        let mut state = self.state.write().await;
        state.messages_sent += 1;

        Ok(())
    }

    fn is_allowed(&self, sender_id: &str) -> bool {
        if self.config.allow_from.is_empty() {
            return true;
        }
        self.config.allow_from.iter().any(|a| a == sender_id)
    }

    fn bus(&self) -> &MessageBus {
        &self.bus
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

/// Discord 用户信息
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DiscordUser {
    pub id: String,
    pub username: String,
    pub discriminator: String,
    pub avatar: Option<String>,
    pub bot: Option<bool>,
    pub system: Option<bool>,
    pub mfa_enabled: Option<bool>,
    pub locale: Option<String>,
    pub verified: Option<bool>,
    pub email: Option<String>,
    pub flags: Option<u64>,
    pub premium_type: Option<u64>,
    pub public_flags: Option<u64>,
}

/// Discord Guild 信息
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DiscordGuild {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub owner: bool,
    pub permissions: String,
    pub features: Vec<String>,
}

/// Discord 消息
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DiscordMessage {
    pub id: String,
    pub channel_id: String,
    pub author: DiscordUser,
    pub content: String,
    pub timestamp: String,
    pub edited_timestamp: Option<String>,
    pub tts: bool,
    pub mention_everyone: bool,
    pub mentions: Vec<DiscordUser>,
    pub mention_roles: Vec<String>,
    pub attachments: Vec<DiscordAttachment>,
    pub embeds: Vec<DiscordEmbed>,
    pub pinned: bool,
    pub kind: u64, // message type
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DiscordAttachment {
    pub id: String,
    pub filename: String,
    pub size: u64,
    pub url: String,
    pub proxy_url: String,
    pub height: Option<u64>,
    pub width: Option<u64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DiscordEmbed {
    pub title: Option<String>,
    #[serde(rename = "type")]
    pub embed_type: Option<String>,
    pub description: Option<String>,
    pub url: Option<String>,
    pub timestamp: Option<String>,
    pub color: Option<u64>,
    pub footer: Option<DiscordEmbedFooter>,
    pub image: Option<DiscordEmbedImage>,
    pub thumbnail: Option<DiscordEmbedThumbnail>,
    pub video: Option<DiscordEmbedVideo>,
    pub provider: Option<DiscordEmbedProvider>,
    pub author: Option<DiscordEmbedAuthor>,
    pub fields: Option<Vec<DiscordEmbedField>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DiscordEmbedFooter {
    pub text: String,
    pub icon_url: Option<String>,
    pub proxy_icon_url: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DiscordEmbedImage {
    pub url: String,
    pub proxy_url: String,
    pub height: Option<u64>,
    pub width: Option<u64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DiscordEmbedThumbnail {
    pub url: String,
    pub proxy_url: String,
    pub height: Option<u64>,
    pub width: Option<u64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DiscordEmbedVideo {
    pub url: Option<String>,
    pub proxy_url: Option<String>,
    pub height: Option<u64>,
    pub width: Option<u64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DiscordEmbedProvider {
    pub name: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DiscordEmbedAuthor {
    pub name: String,
    pub url: Option<String>,
    pub icon_url: Option<String>,
    pub proxy_icon_url: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DiscordEmbedField {
    pub name: String,
    pub value: String,
    pub inline: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discord_config_default() {
        let config = DiscordConfig::default();
        assert!(!config.enabled);
        assert!(config.token.is_empty());
        assert_eq!(config.command_prefix, "!");
    }

    #[test]
    fn test_allow_from() {
        let config = DiscordConfig {
            allow_from: vec!["123".to_string(), "456".to_string()],
            ..Default::default()
        };

        let bus = Arc::new(MessageBus::new(10));
        let channel = DiscordChannel::new(config, bus).unwrap();

        assert!(channel.is_allowed("123"));
        assert!(channel.is_allowed("456"));
        assert!(!channel.is_allowed("789"));
    }

    #[test]
    fn test_allow_from_empty() {
        let config = DiscordConfig {
            allow_from: vec![],
            ..Default::default()
        };

        let bus = Arc::new(MessageBus::new(10));
        let channel = DiscordChannel::new(config, bus).unwrap();

        // 空列表意味着允许所有人
        assert!(channel.is_allowed("anyone"));
    }
}
