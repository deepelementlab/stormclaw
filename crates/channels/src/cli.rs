//! CLI 虚拟渠道
//!
//! 用于命令行界面的虚拟渠道，方便测试和本地使用

use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{RwLock, mpsc};
use stormclaw_core::{MessageBus, OutboundMessage};
use super::{BaseChannel, ChannelState};

/// CLI 虚拟渠道
///
/// 允许通过标准输入/输出与 Agent 交互
pub struct CliChannel {
    bus: Arc<MessageBus>,
    state: Arc<RwLock<ChannelState>>,
    running: Arc<AtomicBool>,
    sender: Arc<RwLock<Option<mpsc::UnboundedSender<String>>>>,
    /// 仅可被 [Self::handle] 消费一次，供测试或程序化读取 Agent 回复
    receiver: Arc<Mutex<Option<mpsc::UnboundedReceiver<String>>>>,
}

pub struct CliHandle {
    receiver: mpsc::UnboundedReceiver<String>,
}

impl CliHandle {
    /// 接收来自 Agent 的响应
    pub async fn recv(&mut self) -> Option<String> {
        self.receiver.recv().await
    }
}

impl CliChannel {
    pub fn new(bus: Arc<MessageBus>) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();

        Self {
            bus,
            state: Arc::new(RwLock::new(ChannelState::default())),
            running: Arc::new(AtomicBool::new(false)),
            sender: Arc::new(RwLock::new(Some(sender))),
            receiver: Arc::new(Mutex::new(Some(receiver))),
        }
    }

    /// 获取消息接收句柄（仅首次调用成功，之后返回 `None`）
    pub fn handle(&self) -> Option<CliHandle> {
        self.receiver
            .lock()
            .ok()?
            .take()
            .map(|receiver| CliHandle { receiver })
    }

    /// 发送用户消息到 Agent
    pub async fn send_user_message(&self, content: String) -> anyhow::Result<()> {
        let inbound = stormclaw_core::InboundMessage::new(
            "cli",
            "user",
            "terminal",
            content,
        );

        self.bus.publish_inbound(inbound).await?;
        Ok(())
    }
}

#[async_trait]
impl BaseChannel for CliChannel {
    fn name(&self) -> &str {
        "cli"
    }

    async fn start(&self) -> anyhow::Result<()> {
        if self.running.load(Ordering::Relaxed) {
            return Ok(());
        }

        tracing::debug!("CLI channel started");

        self.running.store(true, Ordering::Relaxed);
        {
            let mut state = self.state.write().await;
            state.is_running = true;
            state.connected_at = Some(chrono::Utc::now());
        }

        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        tracing::debug!("CLI channel stopped");
        self.running.store(false, Ordering::Relaxed);

        let mut state = self.state.write().await;
        state.is_running = false;

        Ok(())
    }

    async fn send(&self, msg: &OutboundMessage) -> anyhow::Result<()> {
        // 发送到 CLI 输出
        println!("\n🐈 Agent: {}", msg.content);

        if let Some(tx) = self.sender.read().await.as_ref() {
            let _ = tx.send(msg.content.clone());
        }

        {
            let mut state = self.state.write().await;
            state.messages_sent += 1;
        }

        Ok(())
    }

    fn bus(&self) -> &MessageBus {
        &self.bus
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}
