//! 聊天渠道适配模块
//!
//! 提供与各种聊天平台的集成

pub mod base;
pub mod telegram;
pub mod whatsapp;
pub mod discord;
pub mod slack;
pub mod cli;
pub mod webhook;
pub mod email;
pub mod manager;
pub mod converter;
pub mod testing;
pub mod monitor;
pub mod template;

pub use base::{BaseChannel, ChannelFactory, ChannelState};
pub use telegram::TelegramChannel;
pub use whatsapp::WhatsAppChannel;
pub use discord::DiscordChannel;
pub use slack::SlackChannel;
pub use cli::CliChannel;
pub use webhook::WebhookChannel;
pub use email::EmailChannel;
pub use manager::{ChannelManager, ChannelStatus};
pub use converter::{UniversalMessage, Attachment, MarkdownConverter, MessageParser};
pub use testing::{ChannelTester, TestResult, MessageLogger, LoggedMessage};
pub use monitor::{ChannelMonitor, ChannelHealth, HealthStatus, ChannelStatsCollector, ChannelStats};
pub use template::{CustomChannelTemplate, CustomConfig, create_custom_channel};
