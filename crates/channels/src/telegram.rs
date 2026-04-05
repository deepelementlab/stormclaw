//! Telegram Bot 渠道实现

use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::RwLock;
use teloxide::{
    prelude::*,
    types::{ChatId, Message, ParseMode, User},
};
use stormclaw_core::{MessageBus, OutboundMessage};
use super::{BaseChannel, ChannelState};
use regex::Regex;
use std::path::PathBuf;

/// Telegram Bot 渠道
pub struct TelegramChannel {
    bot: AutoSend<Bot>,
    token: String,
    allow_from: Vec<String>,
    bus: Arc<MessageBus>,
    state: Arc<RwLock<ChannelState>>,
    running: Arc<AtomicBool>,
    chat_context: Arc<RwLock<std::collections::HashMap<String, String>>>,
}

impl TelegramChannel {
    /// 创建新的 Telegram 渠道
    pub fn new(
        token: String,
        allow_from: Vec<String>,
        bus: Arc<MessageBus>,
    ) -> anyhow::Result<Self> {
        if token.is_empty() {
            anyhow::bail!("Telegram bot token is required");
        }

        let bot = Bot::new(token.clone()).auto_send();

        Ok(Self {
            bot,
            token,
            allow_from,
            bus,
            state: Arc::new(RwLock::new(ChannelState::default())),
            running: Arc::new(AtomicBool::new(false)),
            chat_context: Arc::new(RwLock::new(std::collections::HashMap::new())),
        })
    }

    /// 通过 Bot API file path 下载到 `~/.stormclaw/media`（与 Python `download_to_drive` 等价路径语义）
    async fn download_tg_file_to_media(
        &self,
        id_prefix: &str,
        ext: &str,
        remote_path: &str,
    ) -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        let media_dir = home.join(".stormclaw").join("media");
        tokio::fs::create_dir_all(&media_dir).await.ok()?;
        let local_path = media_dir.join(format!("{}{}", id_prefix, ext));
        let url = format!(
            "https://api.telegram.org/file/bot{}/{}",
            self.token, remote_path
        );
        let resp = reqwest::get(url).await.ok()?;
        let bytes = resp.bytes().await.ok()?;
        tokio::fs::write(&local_path, bytes).await.ok()?;
        Some(local_path)
    }

    /// 处理传入的消息
    async fn handle_incoming_message(&self, msg: Message) -> anyhow::Result<()> {
        let chat_id = msg.chat.id.to_string();
        let sender_id = msg
            .from()
            .map(|u| telegram_sender_id(u))
            .unwrap_or_else(|| chat_id.clone());

        // 对齐 base._handle_message：先校验 allowlist，再处理媒体与入总线
        if !self.is_allowed(&sender_id) {
            tracing::debug!(
                "Rejected Telegram message from unauthorized sender: {}",
                sender_id
            );
            return Ok(());
        }

        // 构建 content + 下载媒体（对齐 Python：落盘到 ~/.stormclaw/media）
        let mut content_parts: Vec<String> = Vec::new();
        let mut media_paths: Vec<String> = Vec::new();

        if let Some(text) = msg.text() {
            if !text.is_empty() {
                content_parts.push(text.to_string());
            }
        }
        if let Some(caption) = msg.caption() {
            if !caption.is_empty() {
                content_parts.push(caption.to_string());
            }
        }

        // 与 Python 一致：photo / voice / audio / document 互斥，只处理一种附件
        if let Some(photos) = msg.photo() {
            if let Some(best) = photos.last() {
                if let Ok(file_meta) = self.bot.get_file(best.file.id.clone()).await {
                    let prefix: String = best.file.id.to_string().chars().take(16).collect();
                    let ext = media_extension("image", None);
                    if let Some(path) = self
                        .download_tg_file_to_media(&prefix, ext, &file_meta.path)
                        .await
                    {
                        media_paths.push(path.to_string_lossy().to_string());
                        content_parts.push(format!("[image: {}]", path.display()));
                    }
                }
            }
        } else if let Some(voice) = msg.voice() {
            if let Ok(file_meta) = self.bot.get_file(voice.file.id.clone()).await {
                let prefix: String = voice.file.id.to_string().chars().take(16).collect();
                let mime = voice.mime_type.as_ref().map(|m| m.essence_str());
                let ext = media_extension("voice", mime);
                if let Some(path) = self
                    .download_tg_file_to_media(&prefix, ext, &file_meta.path)
                    .await
                {
                    media_paths.push(path.to_string_lossy().to_string());
                    content_parts.push(format!("[voice: {}]", path.display()));
                }
            }
        } else if let Some(audio) = msg.audio() {
            if let Ok(file_meta) = self.bot.get_file(audio.file.id.clone()).await {
                let prefix: String = audio.file.id.to_string().chars().take(16).collect();
                let mime = audio.mime_type.as_ref().map(|m| m.essence_str());
                let ext = media_extension("audio", mime);
                if let Some(path) = self
                    .download_tg_file_to_media(&prefix, ext, &file_meta.path)
                    .await
                {
                    media_paths.push(path.to_string_lossy().to_string());
                    content_parts.push(format!("[audio: {}]", path.display()));
                }
            }
        } else if let Some(doc) = msg.document() {
            if let Ok(file_meta) = self.bot.get_file(doc.file.id.clone()).await {
                let prefix: String = doc.file.id.to_string().chars().take(16).collect();
                let mime = doc.mime_type.as_ref().map(|m| m.essence_str());
                let ext = media_extension("file", mime);
                if let Some(path) = self
                    .download_tg_file_to_media(&prefix, ext, &file_meta.path)
                    .await
                {
                    media_paths.push(path.to_string_lossy().to_string());
                    content_parts.push(format!("[file: {}]", path.display()));
                }
            }
        }

        let content = if content_parts.is_empty() {
            "[empty message]".to_string()
        } else {
            content_parts.join("\n")
        };

        tracing::debug!(
            "Telegram message from {}: {}",
            sender_id,
            content
        );

        // 更新统计
        {
            let mut state = self.state.write().await;
            state.messages_received += 1;
        }

        let from = msg.from();
        let inbound = stormclaw_core::InboundMessage {
            channel: self.name().to_string(),
            sender_id,
            chat_id,
            content,
            timestamp: chrono::Utc::now(),
            media: media_paths,
            metadata: serde_json::json!({
                "message_id": msg.id,
                "user_id": from.map(|u| u.id),
                "username": from.and_then(|u| u.username.clone()),
                "first_name": from.map(|u| u.first_name.clone()),
                "last_name": from.and_then(|u| u.last_name.clone()),
            }),
        };

        self.bus.publish_inbound(inbound).await?;
        Ok(())
    }
}

/// 与 Python `str(user.username or user.id)` 一致（空字符串用户名视为无用户名）
fn telegram_sender_id(user: &User) -> String {
    user.username
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| user.id.to_string())
}

/// 根据 MIME 类型与媒体大类推断 Telegram 附件扩展名（与常见 Bot 发送惯例一致）。
fn media_extension(media_type: &str, mime_type: Option<&str>) -> &'static str {
    if let Some(m) = mime_type {
        match m {
            "image/jpeg" => return ".jpg",
            "image/png" => return ".png",
            "image/gif" => return ".gif",
            "audio/ogg" => return ".ogg",
            "audio/mpeg" => return ".mp3",
            "audio/mp4" => return ".m4a",
            _ => {}
        }
    }
    match media_type {
        "image" => ".jpg",
        "voice" => ".ogg",
        "audio" => ".mp3",
        "file" => "",
        _ => "",
    }
}

#[async_trait]
impl BaseChannel for TelegramChannel {
    fn name(&self) -> &str {
        "telegram"
    }

    async fn start(&self) -> anyhow::Result<()> {
        if self.running.load(Ordering::Relaxed) {
            tracing::warn!("Telegram channel is already running");
            return Ok(());
        }

        tracing::info!("Starting Telegram channel");

        // 更新状态
        {
            let mut state = self.state.write().await;
            state.is_running = true;
            state.connected_at = Some(chrono::Utc::now());
        }
        self.running.store(true, Ordering::Relaxed);

        let bot = self.bot.clone();
        let channel = self.clone();

        // 创建消息处理器
        let handler = Update::filter_message().branch(
            dptree::endpoint(move |msg: Message, bot: AutoSend<Bot>| {
                let channel = channel.clone();
                async move {
                    if let Err(e) = channel.handle_incoming_message(msg).await {
                        tracing::error!("Error handling Telegram message: {}", e);
                    }
                    respond(())
                }
            })
        );

        // 启动调度器
        Dispatcher::builder(bot, handler)
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;

        self.running.store(false, Ordering::Relaxed);
        {
            let mut state = self.state.write().await;
            state.is_running = false;
        }

        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        tracing::info!("Stopping Telegram channel");
        self.running.store(false, Ordering::Relaxed);

        let mut state = self.state.write().await;
        state.is_running = false;

        Ok(())
    }

    async fn send(&self, msg: &OutboundMessage) -> anyhow::Result<()> {
        let chat_id: i64 = msg.chat_id
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid chat_id {}: {}", msg.chat_id, e))?;

        tracing::debug!("Sending Telegram message to {}: {}", chat_id, msg.content);

        // 支持解析模式
        let parse_mode = ParseMode::Html;

        // 对齐 Python：Markdown -> Telegram-safe HTML；失败则回退纯文本
        let html = markdown_to_telegram_html(&msg.content);
        let send_res = self.bot
            .send_message(ChatId(chat_id), html)
            .parse_mode(parse_mode)
            .await;

        if send_res.is_err() {
            self.bot
                .send_message(ChatId(chat_id), msg.content.clone())
                .await?;
        }

        // 更新统计
        {
            let mut state = self.state.write().await;
            state.messages_sent += 1;
        }

        Ok(())
    }

    fn is_allowed(&self, sender_id: &str) -> bool {
        if self.allow_from.is_empty() {
            return true;
        }

        // 支持用户 ID 和用户名
        self.allow_from.iter().any(|allowed| {
            allowed == sender_id
                || allowed.strip_prefix('@').unwrap_or(allowed) == sender_id.strip_prefix('@').unwrap_or(sender_id)
        })
    }

    fn bus(&self) -> &MessageBus {
        &self.bus
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

/// 将 Markdown 转为 Telegram HTML（`ParseMode::Html`）；解析失败时由调用方回退纯文本。
fn markdown_to_telegram_html(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    // Protect code blocks
    let mut code_blocks: Vec<String> = Vec::new();
    let re_code_block = Regex::new(r"```[\w]*\n?([\s\S]*?)```").unwrap();
    let mut tmp = re_code_block
        .replace_all(text, |caps: &regex::Captures| {
            code_blocks.push(caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string());
            format!("\u{0000}CB{}\u{0000}", code_blocks.len() - 1)
        })
        .to_string();

    // Protect inline code
    let mut inline_codes: Vec<String> = Vec::new();
    let re_inline = Regex::new(r"`([^`]+)`").unwrap();
    tmp = re_inline
        .replace_all(&tmp, |caps: &regex::Captures| {
            inline_codes.push(caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string());
            format!("\u{0000}IC{}\u{0000}", inline_codes.len() - 1)
        })
        .to_string();

    // Headers: strip leading #'s
    let re_headers = Regex::new(r"(?m)^#{1,6}\s+(.+)$").unwrap();
    tmp = re_headers.replace_all(&tmp, "$1").to_string();

    // Blockquotes: strip >
    let re_quote = Regex::new(r"(?m)^>\s*(.*)$").unwrap();
    tmp = re_quote.replace_all(&tmp, "$1").to_string();

    // Escape HTML
    tmp = tmp.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;");

    // Links [text](url)
    let re_link = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();
    tmp = re_link.replace_all(&tmp, "<a href=\"$2\">$1</a>").to_string();

    // Bold **text** and __text__
    let re_bold1 = Regex::new(r"\*\*(.+?)\*\*").unwrap();
    tmp = re_bold1.replace_all(&tmp, "<b>$1</b>").to_string();
    let re_bold2 = Regex::new(r"__(.+?)__").unwrap();
    tmp = re_bold2.replace_all(&tmp, "<b>$1</b>").to_string();

    // Italic _text_（避免字内 `_`）：`regex` crate 不支持 look-behind/look-ahead，用前后分界捕获组近似 Python 语义
    let re_italic = Regex::new(r"(^|[^a-zA-Z0-9])_([^_]+)_([^a-zA-Z0-9]|$)").unwrap();
    tmp = re_italic.replace_all(&tmp, "$1<i>$2</i>$3").to_string();

    // Strikethrough ~~text~~
    let re_strike = Regex::new(r"~~(.+?)~~").unwrap();
    tmp = re_strike.replace_all(&tmp, "<s>$1</s>").to_string();

    // Bullet lists
    let re_bullet = Regex::new(r"(?m)^[-*]\s+").unwrap();
    tmp = re_bullet.replace_all(&tmp, "• ").to_string();

    // Restore inline codes
    for (i, code) in inline_codes.iter().enumerate() {
        let escaped = code.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;");
        tmp = tmp.replace(&format!("\u{0000}IC{}\u{0000}", i), &format!("<code>{}</code>", escaped));
    }

    // Restore code blocks
    for (i, code) in code_blocks.iter().enumerate() {
        let escaped = code.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;");
        tmp = tmp.replace(&format!("\u{0000}CB{}\u{0000}", i), &format!("<pre><code>{}</code></pre>", escaped));
    }

    tmp
}

#[cfg(test)]
mod telegram_tests {
    use super::{markdown_to_telegram_html, media_extension, telegram_sender_id};
    use teloxide::types::{User, UserId};

    #[test]
    fn sender_id_prefers_username_like_python() {
        let u = User {
            id: UserId(42),
            is_bot: false,
            first_name: "A".into(),
            last_name: None,
            username: Some("alice".into()),
            language_code: None,
            is_premium: false,
            added_to_attachment_menu: false,
        };
        assert_eq!(telegram_sender_id(&u), "alice");
    }

    #[test]
    fn sender_id_falls_back_to_numeric_id() {
        let u = User {
            id: UserId(99),
            is_bot: false,
            first_name: "A".into(),
            last_name: None,
            username: None,
            language_code: None,
            is_premium: false,
            added_to_attachment_menu: false,
        };
        assert_eq!(telegram_sender_id(&u), "99");
    }

    #[test]
    fn media_extension_defaults_match_python() {
        assert_eq!(media_extension("voice", None), ".ogg");
        assert_eq!(media_extension("audio", None), ".mp3");
        assert_eq!(media_extension("file", None), "");
    }

    #[test]
    fn markdown_bold_becomes_b_tag() {
        let out = markdown_to_telegram_html("**hi**");
        assert!(out.contains("<b>hi</b>"), "got: {out}");
    }

    #[test]
    fn markdown_inline_code() {
        let out = markdown_to_telegram_html("`x`");
        assert!(out.contains("<code>x</code>"), "got: {out}");
    }

    #[test]
    fn markdown_link_to_telegram_a() {
        let out = markdown_to_telegram_html("[a](https://ex.com)");
        assert!(
            out.contains("<a href=\"https://ex.com\">a</a>"),
            "got: {out}"
        );
    }
}

impl Clone for TelegramChannel {
    fn clone(&self) -> Self {
        Self {
            bot: self.bot.clone(),
            token: self.token.clone(),
            allow_from: self.allow_from.clone(),
            bus: self.bus.clone(),
            state: self.state.clone(),
            running: self.running.clone(),
            chat_context: self.chat_context.clone(),
        }
    }
}

/// 获取 Telegram 用户 ID
///
/// 用户可以使用 @userinfobot 获取自己的用户 ID
pub async fn get_telegram_user_id(bot_token: &str, username: &str) -> anyhow::Result<Option<i64>> {
    let client = reqwest::Client::new();
    let url = format!("https://api.telegram.org/bot{}/getChat", bot_token);

    let response = client
        .post(&url)
        .json(&serde_json::json!({"chat_id": username}))
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

    if response["ok"].as_bool().unwrap_or(false) {
        if let Some(id) = response["result"]["id"].as_i64() {
            return Ok(Some(id));
        }
    }

    Ok(None)
}
