//! Email 渠道 (IMAP/SMTP)
//!
//! 通过电子邮件与 Agent 交互

use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::RwLock;
use stormclaw_core::{MessageBus, OutboundMessage, InboundMessage};
use super::{BaseChannel, ChannelState};
use chrono::Utc;

/// Email 渠道配置
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct EmailConfig {
    pub enabled: bool,
    pub imap: ImapConfig,
    pub smtp: SmtpConfig,
    pub check_interval: u64, // 秒
    pub allow_from: Vec<String>,
    pub folder: String, // IMAP 文件夹名称
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ImapConfig {
    pub server: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub use_tls: bool,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct SmtpConfig {
    pub server: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub use_tls: bool,
    pub from_name: String,
    pub from_address: String,
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            imap: ImapConfig {
                server: "imap.gmail.com".to_string(),
                port: 993,
                username: String::new(),
                password: String::new(),
                use_tls: true,
            },
            smtp: SmtpConfig {
                server: "smtp.gmail.com".to_string(),
                port: 587,
                username: String::new(),
                password: String::new(),
                use_tls: true,
                from_name: "stormclaw".to_string(),
                from_address: String::new(),
            },
            check_interval: 60,
            allow_from: Vec::new(),
            folder: "INBOX".to_string(),
        }
    }
}

/// Email 渠道
///
/// 通过 IMAP 接收邮件，通过 SMTP 发送邮件
pub struct EmailChannel {
    config: EmailConfig,
    bus: Arc<MessageBus>,
    state: Arc<RwLock<ChannelState>>,
    running: Arc<AtomicBool>,
    message_count: Arc<AtomicU64>,
    last_uid: Arc<RwLock<Option<u32>>>,
}

impl EmailChannel {
    pub fn new(
        config: EmailConfig,
        bus: Arc<MessageBus>,
    ) -> Self {
        Self {
            config,
            bus,
            state: Arc::new(RwLock::new(ChannelState::default())),
            running: Arc::new(AtomicBool::new(false)),
            message_count: Arc::new(AtomicU64::new(0)),
            last_uid: Arc::new(RwLock::new(None)),
        }
    }

    /// 发送邮件 (使用 lettre)
    async fn send_email(&self, to: &str, subject: &str, body: &str) -> anyhow::Result<()> {
        use lettre::{
            message::{header::ContentType, Mailbox},
            transport::smtp::authentication::Credentials,
            Message, SmtpTransport, Transport,
        };

        // 构建邮件
        let from_mailbox = format!("{} <{}>", self.config.smtp.from_name, self.config.smtp.from_address)
            .parse::<Mailbox>()?;

        let to_mailbox = to.parse::<Mailbox>()?;

        let email = Message::builder()
            .from(from_mailbox)
            .to(to_mailbox)
            .subject(subject)
            .header(ContentType::TEXT_PLAIN)
            .body(body.to_string())?;

        // 创建 SMTP 传输
        let creds = Credentials::new(
            self.config.smtp.username.clone(),
            self.config.smtp.password.clone(),
        );

        let mailer = if self.config.smtp.use_tls {
            SmtpTransport::relay(&self.config.smtp.server)?
                .port(self.config.smtp.port)
                .credentials(creds)
                .build()
        } else {
            SmtpTransport::builder_dangerous(&self.config.smtp.server)
                .port(self.config.smtp.port)
                .credentials(creds)
                .build()
        };

        // 发送邮件
        mailer.send(&email)?;

        tracing::info!("Email sent to {}: {}", to, subject);

        Ok(())
    }

    /// 检查新邮件
    async fn check_new_emails(&self) -> anyhow::Result<()> {
        #[cfg(feature = "email-imap")]
        {
            self.check_new_emails_impl().await
        }

        #[cfg(not(feature = "email-imap"))]
        {
            tracing::debug!("Checking for new emails in {} (IMAP feature not enabled)", self.config.folder);
            tracing::warn!("Email IMAP support requires the 'email-imap' feature");
            Ok(())
        }
    }

    #[cfg(feature = "email-imap")]
    async fn check_new_emails_impl(&self) -> anyhow::Result<()> {
        use async_imap::{Client, types::Fetch};
        use async_native_tls::TlsConnector;
        use mail_parser::{Message, HeaderValue};

        tracing::debug!("Checking for new emails in {}", self.config.folder);

        // 连接到 IMAP 服务器
        let tls = TlsConnector::new();
        let client = Client::connect(
            (self.config.imap.server.as_str(), self.config.imap.port),
            if self.config.imap.use_tls { Some(tls) } else { None },
        ).await?;

        // 登录
        let mut session = client
            .login(&self.config.imap.username, &self.config.imap.password)
            .await?
            .1;

        // 选择文件夹
        session.select(self.config.folder.clone()).await?;

        // 获取上次检查的 UID
        let last_uid = *self.last_uid.read().await;

        // 搜索新邮件
        let search_query = if let Some(uid) = last_uid {
            format!("UID:{}", uid + 1)
        } else {
            "UNSEEN".to_string()
        };

        let emails = session.search(search_query,).await?;

        if emails.is_empty() {
            tracing::debug!("No new emails found");
            session.logout().await?;
            return Ok(());
        }

        tracing::info!("Found {} new emails", emails.len());

        // 获取并处理邮件
        for uid in emails.iter() {
            if let Ok(messages) = session.fetch(*uid, "(RFC822)",).await {
                for message in messages.iter() {
                    if let Some(body) = message.body() {
                        // 解析邮件
                        if let Err(e) = self.process_email(body,).await {
                            tracing::error!("Failed to process email: {}", e);
                        } else {
                            // 更新最后 UID
                            *self.last_uid.write().await = Some(*uid);

                            // 标记为已读
                            let _ = session.store(*uid, "+FLAGS (\\Seen)",).await;
                        }
                    }
                }
            }
        }

        session.logout().await?;

        Ok(())
    }

    #[cfg(feature = "email-imap")]
    async fn process_email(&self, raw_email: &[u8]) -> anyhow::Result<()> {
        use mail_parser::{Message, HeaderValue};

        let parsed = Message::parse(raw_email)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse email"))?;

        // 提取发件人
        let from = parsed.from()
            .and_then(|f| f.first())
            .and_then(|a| a.address())
            .unwrap_or("unknown");

        // 提取主题
        let subject = parsed.subject()
            .and_then(|s| s.as_text())
            .unwrap_or("No Subject");

        // 提取正文
        let body = self.extract_email_body(&parsed)?;

        // 检查权限
        if !self.is_allowed(from) {
            tracing::debug!("Email from unauthorized sender: {}", from);
            return Ok(());
        }

        // 创建入站消息
        let inbound = InboundMessage {
            channel: "email".to_string(),
            sender_id: from.to_string(),
            chat_id: from.to_string(),
            content: format!("Subject: {}\n\n{}", subject, body),
            timestamp: Utc::now(),
            media: Vec::new(),
            metadata: serde_json::json!({
                "subject": subject,
                "raw_from": parsed.from().map(|v| v.to_string()),
            }),
        };

        // 发布到消息总线
        self.bus.publish_inbound(inbound).await?;

        // 更新计数
        self.message_count.fetch_add(1, Ordering::Relaxed);

        tracing::info!("Processed email from {}: {}", from, subject);

        Ok(())
    }

    #[cfg(feature = "email-imap")]
    fn extract_email_body(&self, email: &Message) -> anyhow::Result<String> {
        use mail_parser::Body;

        // 尝试获取纯文本正文
        if let Some(text_body) = email.text_body() {
            Ok(text_body.to_string())
        } else if let Some(html_body) = email.html_body() {
            // 如果没有纯文本，尝试从 HTML 提取
            Ok(extract_email_text(html_body))
        } else {
            // 尝试从第一个部分获取
            if let Some(part) = email.parts.first() {
                Ok(part.contents().to_string())
            } else {
                Ok(String::new())
            }
        }
    }

    /// 运行邮件检查循环
    async fn run_check_loop(&self) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(Duration::from_secs(self.config.check_interval));

        while self.running.load(Ordering::Relaxed) {
            interval.tick().await;

            if let Err(e) = self.check_new_emails().await {
                tracing::error!("Email check error: {}", e);
            }
        }

        Ok(())
    }

    /// 测试 IMAP 连接
    pub async fn test_imap_connection(&self) -> anyhow::Result<()> {
        #[cfg(feature = "email-imap")]
        {
            use async_imap::Client;
            use async_native_tls::TlsConnector;

            tracing::info!("Testing IMAP connection to {}:{}", self.config.imap.server, self.config.imap.port);

            let tls = TlsConnector::new();
            let client = Client::connect(
                (self.config.imap.server.as_str(), self.config.imap.port),
                if self.config.imap.use_tls { Some(tls) } else { None },
            ).await?;

            let session = client
                .login(&self.config.imap.username, &self.config.imap.password)
                .await?;

            tracing::info!("IMAP connection test successful");
            session.logout().await?;
            Ok(())
        }

        #[cfg(not(feature = "email-imap"))]
        {
            tracing::warn!("IMAP connection test skipped: 'email-imap' feature not enabled");
            Err(anyhow::anyhow!("IMAP feature not enabled"))
        }
    }

    /// 测试 SMTP 连接
    pub async fn test_smtp_connection(&self) -> anyhow::Result<()> {
        use lettre::transport::smtp::authentication::Credentials;
        use lettre::{SmtpTransport, Transport};

        let creds = Credentials::new(
            self.config.smtp.username.clone(),
            self.config.smtp.password.clone(),
        );

        let mailer = if self.config.smtp.use_tls {
            SmtpTransport::relay(&self.config.smtp.server)?
                .port(self.config.smtp.port)
                .credentials(creds)
                .build()
        } else {
            SmtpTransport::builder_dangerous(&self.config.smtp.server)
                .port(self.config.smtp.port)
                .credentials(creds)
                .build()
        };

        // 测试连接
        let _ = mailer.test_connection()?;

        tracing::info!("SMTP connection test successful");

        Ok(())
    }
}

#[async_trait]
impl BaseChannel for EmailChannel {
    fn name(&self) -> &str {
        "email"
    }

    async fn start(&self) -> anyhow::Result<()> {
        if self.running.load(Ordering::Relaxed) {
            return Ok(());
        }

        tracing::info!("Starting email channel");
        tracing::info!("  IMAP: {}:{}", self.config.imap.server, self.config.imap.port);
        tracing::info!("  SMTP: {}:{}", self.config.smtp.server, self.config.smtp.port);
        tracing::info!("  Folder: {}", self.config.folder);

        // 测试连接
        if let Err(e) = self.test_smtp_connection().await {
            tracing::warn!("SMTP connection test failed: {}", e);
        }

        self.running.store(true, Ordering::Relaxed);
        {
            let mut state = self.state.write().await;
            state.is_running = true;
            state.connected_at = Some(Utc::now());
        }

        // 启动邮件检查循环
        let running = self.running.clone();
        let check_interval = self.config.check_interval;
        let bus = self.bus.clone();
        let allow_from = self.config.allow_from.clone();
        let folder = self.config.folder.clone();
        let imap_config = self.config.imap.clone();
        let message_count = self.message_count.clone();
        let last_uid = self.last_uid.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(check_interval));

            while running.load(Ordering::Relaxed) {
                interval.tick().await;

                #[cfg(feature = "email-imap")]
                {
                    use async_imap::Client;
                    use async_native_tls::TlsConnector;

                    tracing::debug!("Checking for new emails in {}", folder);

                    // 连接到 IMAP 服务器
                    let tls = match TlsConnector::new() {
                        Ok(t) => t,
                        Err(e) => {
                            tracing::error!("Failed to create TLS connector: {}", e);
                            continue;
                        }
                    };

                    let client = match Client::connect(
                        (imap_config.server.as_str(), imap_config.port),
                        if imap_config.use_tls { Some(tls) } else { None },
                    ).await {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::error!("IMAP connection error: {}", e);
                            continue;
                        }
                    };

                    // 登录
                    let mut session = match client
                        .login(&imap_config.username, &imap_config.password)
                        .await {
                            Ok(s) => s.1,
                            Err(e) => {
                                tracing::error!("IMAP login error: {:?}", e);
                                continue;
                            }
                        };

                    // 选择文件夹
                    if let Err(e) = session.select(&folder).await {
                        tracing::error!("Failed to select folder {}: {}", folder, e);
                        let _ = session.logout().await;
                        continue;
                    }

                    // 获取上次检查的 UID
                    let last_uid_val = *last_uid.read().await;

                    // 搜索新邮件
                    let search_query = if let Some(uid) = last_uid_val {
                        format!("UID:{}", uid + 1)
                    } else {
                        "UNSEEN".to_string()
                    };

                    let emails = match session.search(&search_query).await {
                        Ok(e) => e,
                        Err(e) => {
                            tracing::error!("IMAP search error: {}", e);
                            let _ = session.logout().await;
                            continue;
                        }
                    };

                    if emails.is_empty() {
                        tracing::debug!("No new emails found");
                        let _ = session.logout().await;
                        continue;
                    }

                    tracing::info!("Found {} new emails", emails.len());

                    // 获取并处理邮件
                    for uid in emails.iter() {
                        if let Ok(messages) = session.fetch(*uid, "(RFC822)").await {
                            for message in messages.iter() {
                                if let Some(body) = message.body() {
                                    // 简单处理邮件内容
                                    let content = String::from_utf8_lossy(body).to_string();

                                    // 创建入站消息
                                    let inbound = InboundMessage {
                                        channel: "email".to_string(),
                                        sender_id: "unknown".to_string(),
                                        chat_id: "unknown".to_string(),
                                        content,
                                        timestamp: Utc::now(),
                                        media: Vec::new(),
                                        metadata: serde_json::json!({}),
                                    };

                                    if let Err(e) = bus.publish_inbound(inbound).await {
                                        tracing::error!("Failed to publish email: {}", e);
                                    } else {
                                        // 更新最后 UID
                                        *last_uid.write().await = Some(*uid);
                                        message_count.fetch_add(1, Ordering::Relaxed);

                                        // 标记为已读
                                        let _ = session.store(*uid, "+FLAGS (\\Seen)").await;
                                    }
                                }
                            }
                        }
                    }

                    let _ = session.logout().await;
                }

                #[cfg(not(feature = "email-imap"))]
                {
                    tracing::debug!("Checking for new emails in {} (IMAP feature not enabled)", folder);
                    let _ = bus; // 避免未使用警告
                    let _ = allow_from;
                    let _ = message_count;
                    let _ = last_uid;
                }
            }
        });

        tracing::info!("Email channel started");

        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        tracing::info!("Stopping email channel");
        self.running.store(false, Ordering::Relaxed);

        let mut state = self.state.write().await;
        state.is_running = false;

        Ok(())
    }

    async fn send(&self, msg: &OutboundMessage) -> anyhow::Result<()> {
        tracing::debug!("Sending email to {}: {}", msg.chat_id, msg.content);

        // msg.chat_id 包含收件人邮箱地址
        // msg.content 包含邮件正文
        let subject = if let Some(metadata) = msg.metadata.get("subject") {
            metadata.as_str().unwrap_or("Re: Your message").to_string()
        } else {
            "Re: Your message".to_string()
        };

        self.send_email(&msg.chat_id, &subject, &msg.content).await?;

        {
            let mut state = self.state.write().await;
            state.messages_sent += 1;
        }

        Ok(())
    }

    fn is_allowed(&self, sender_id: &str) -> bool {
        if self.config.allow_from.is_empty() {
            return true;
        }

        // 支持电子邮件地址匹配
        self.config.allow_from.iter().any(|allowed| {
            sender_id.ends_with(allowed) || sender_id == allowed
        })
    }

    fn bus(&self) -> &MessageBus {
        &self.bus
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

/// 解析邮件地址
pub fn parse_email_address(addr: &str) -> Option<String> {
    // 简单的邮件地址解析
    // 支持格式: "user@example.com" 或 "Name <user@example.com>"
    use regex::Regex;

    let re = Regex::new(r"<([^@<>]+@[^@<>]+)>").ok()?;
    if let Some(caps) = re.captures(addr) {
        caps.get(1).map(|m| m.as_str().to_string())
    } else if addr.contains('@') {
        Some(addr.trim().to_string())
    } else {
        None
    }
}

/// 提取邮件纯文本内容
pub fn extract_email_text(body: &str) -> String {
    // 简单提取纯文本，移除 HTML 标签
    use regex::Regex;

    let re = match Regex::new(r"<[^>]+>") {
        Ok(r) => r,
        Err(_) => return body.to_string(),
    };
    re.replace_all(body, "").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_email_address() {
        assert_eq!(
            parse_email_address("user@example.com"),
            Some("user@example.com".to_string())
        );
        assert_eq!(
            parse_email_address("John Doe <user@example.com>"),
            Some("user@example.com".to_string())
        );
        assert_eq!(parse_email_address("invalid"), None);
    }

    #[test]
    fn test_extract_email_text() {
        let html = "<p>Hello <b>World</b>!</p>";
        assert_eq!(extract_email_text(html), "Hello World!");
    }

    #[test]
    fn test_email_config_default() {
        let config = EmailConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.imap.port, 993);
        assert_eq!(config.smtp.port, 587);
    }

    #[test]
    fn test_email_channel_creation() {
        let config = EmailConfig::default();
        let bus = std::sync::Arc::new(stormclaw_core::MessageBus::new(10));

        let channel = EmailChannel::new(config, bus);
        assert!(!channel.is_running());
    }

    #[test]
    fn test_is_allowed_empty() {
        let config = EmailConfig {
            allow_from: vec![],
            ..Default::default()
        };
        let bus = std::sync::Arc::new(stormclaw_core::MessageBus::new(10));
        let channel = EmailChannel::new(config, bus);

        // 空列表意味着允许所有人
        assert!(channel.is_allowed("anyone@example.com"));
    }

    #[test]
    fn test_is_allowed_specific() {
        let config = EmailConfig {
            allow_from: vec!["allowed@example.com".to_string()],
            ..Default::default()
        };
        let bus = std::sync::Arc::new(stormclaw_core::MessageBus::new(10));
        let channel = EmailChannel::new(config, bus);

        assert!(channel.is_allowed("allowed@example.com"));
        assert!(!channel.is_allowed("other@example.com"));
    }

    #[test]
    fn test_extract_html_with_nested_tags() {
        let html = "<div><p>Hello <span>World</span>!</p></div>";
        assert_eq!(extract_email_text(html), "Hello World!");
    }

    #[test]
    fn test_extract_empty_html() {
        let html = "";
        let result = extract_email_text(html);
        assert!(result.is_empty() || result == "None"); // 正则可能返回 None
    }
}
