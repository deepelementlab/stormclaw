//! 子代理生成工具

use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::bus::MessageBus;
use crate::providers::LLMProvider;
use crate::agent::subagent::SubagentManager;
use super::base::Tool;

/// 子代理生成工具
///
/// 允许 Agent 生成子代理来处理后台任务
pub struct SpawnTool<P: LLMProvider + 'static> {
    manager: Arc<SubagentManager<P>>,
    bus: Arc<MessageBus>,
    context: Arc<tokio::sync::RwLock<Option<SpawnContext>>>,
}

struct SpawnContext {
    channel: String,
    chat_id: String,
}

impl<P: LLMProvider> SpawnTool<P> {
    /// 创建新的子代理生成工具
    pub fn new(manager: Arc<SubagentManager<P>>, bus: Arc<MessageBus>) -> Self {
        Self {
            manager,
            bus,
            context: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    /// 设置当前上下文
    pub async fn set_context(&self, channel: String, chat_id: String) {
        *self.context.write().await = Some(SpawnContext { channel, chat_id });
    }
}

#[async_trait]
impl<P: LLMProvider + 'static> Tool for SpawnTool<P> {
    fn name(&self) -> &str {
        "spawn"
    }

    fn description(&self) -> &str {
        "Spawn a subagent to handle a specific task in the background"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task": {
                    "type": "string",
                    "description": "Task description for the subagent"
                },
                "label": {
                    "type": "string",
                    "description": "Human-readable label for the task (optional)"
                }
            },
            "required": ["task"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let task = args["task"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'task' argument"))?;

        let label = args["label"].as_str().map(|s| s.to_string());

        // 获取当前上下文
        let (origin_channel, origin_chat_id) = {
            let ctx = self.context.read().await;

            let channel = ctx.as_ref()
                .map(|c| c.channel.clone())
                .unwrap_or_else(|| "cli".to_string());

            let chat_id = ctx.as_ref()
                .map(|c| c.chat_id.clone())
                .unwrap_or_else(|| "direct".to_string());

            (channel, chat_id)
        };

        let result = self.manager.spawn(
            task.to_string(),
            label,
            origin_channel,
            origin_chat_id,
        ).await?;

        Ok(result)
    }
}
