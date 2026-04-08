//! 子代理管理器

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::agent::runtime_state::AgentRuntimeState;
use crate::agent::tool_executor::{ToolExecutor, ToolInvocationContext};
use crate::agent::tools::{
    normalize_tool_arguments, ExecTool, ListDirTool, ReadFileTool, ToolRegistry, WebFetchTool,
    WebSearchTool, WriteFileTool,
};
#[cfg(feature = "wasm-tools")]
use crate::agent::tools::WasmEvalTool;
use crate::bus::{InboundMessage, MessageBus};
use crate::providers::{ChatMessage, LLMProvider};

/// 子代理管理器
pub struct SubagentManager<P: LLMProvider + 'static> {
    provider: Arc<P>,
    runtime: Arc<RwLock<AgentRuntimeState>>,
    bus: Arc<MessageBus>,
    tool_executor: Arc<ToolExecutor>,
    running_tasks: Arc<RwLock<HashMap<String, tokio::task::JoinHandle<()>>>>,
}

impl<P: LLMProvider + 'static> SubagentManager<P> {
    /// 创建新的子代理管理器
    pub fn new(
        provider: Arc<P>,
        runtime: Arc<RwLock<AgentRuntimeState>>,
        bus: Arc<MessageBus>,
        tool_executor: Arc<ToolExecutor>,
    ) -> Self {
        Self {
            provider,
            runtime,
            bus,
            tool_executor,
            running_tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 生成子代理
    pub async fn spawn(
        &self,
        task: String,
        label: Option<String>,
        origin_channel: String,
        origin_chat_id: String,
    ) -> anyhow::Result<String> {
        let task_id = Uuid::new_v4().to_string()[..8].to_string();
        let display_label = label.unwrap_or_else(|| {
            if task.len() > 30 {
                format!("{}...", &task[..30])
            } else {
                task.clone()
            }
        });

        let rt = self.runtime.read().await.clone();

        let provider = self.provider.clone();
        let workspace = rt.workspace.clone();
        let bus = self.bus.clone();
        let model = rt.model.clone();
        let brave_api_key = rt.brave_api_key.clone();
        let running_tasks = self.running_tasks.clone();
        let tool_executor = self.tool_executor.clone();

        let task_clone = task.clone();
        let label_clone = display_label.clone();
        let origin_channel_clone = origin_channel.clone();
        let origin_chat_id_clone = origin_chat_id.clone();

        let task_id_for_async = task_id.clone();
        let task_id_for_result = task_id.clone();

        let handle = tokio::spawn(async move {
            if let Err(e) = Self::run_subagent(
                provider,
                workspace,
                bus,
                model,
                brave_api_key,
                tool_executor,
                task_id_for_async.clone(),
                task_clone,
                label_clone,
                origin_channel_clone,
                origin_chat_id_clone,
            )
            .await
            {
                tracing::error!("Subagent {} failed: {}", task_id_for_async, e);
            }

            running_tasks.write().await.remove(&task_id_for_async);
        });

        self.running_tasks.write().await.insert(task_id, handle);

        Ok(format!(
            "Subagent [{}] started (id: {}). I'll notify you when it completes.",
            display_label, task_id_for_result
        ))
    }

    /// 运行子代理任务
    async fn run_subagent(
        provider: Arc<P>,
        workspace: PathBuf,
        bus: Arc<MessageBus>,
        model: Option<String>,
        brave_api_key: Option<String>,
        tool_executor: Arc<ToolExecutor>,
        task_id: String,
        task: String,
        label: String,
        origin_channel: String,
        origin_chat_id: String,
    ) -> anyhow::Result<()> {
        tracing::info!("Subagent [{}] starting task: {}", task_id, label);

        let sec = tool_executor.security_snapshot().await;
        let docker = sec.docker.clone();

        let tools = ToolRegistry::new();
        tools
            .register(Arc::new(ReadFileTool {
                workspace_root: workspace.clone(),
            }))
            .await;
        tools
            .register(Arc::new(WriteFileTool {
                workspace_root: workspace.clone(),
            }))
            .await;
        tools
            .register(Arc::new(ListDirTool {
                workspace_root: workspace.clone(),
            }))
            .await;
        tools
            .register(Arc::new(ExecTool::new(workspace.clone(), docker)))
            .await;
        tools
            .register(Arc::new(WebSearchTool::new(brave_api_key.clone())))
            .await;
        tools.register(Arc::new(WebFetchTool)).await;

        #[cfg(feature = "wasm-tools")]
        if sec.wasm_tools_enabled {
            tools.register(Arc::new(WasmEvalTool)).await;
        }

        let session_key = format!("subagent:{}:{}:{}", task_id, origin_channel, origin_chat_id);

        let system_prompt = Self::build_subagent_prompt(&task, &workspace);

        let mut messages: Vec<ChatMessage> = vec![
            ChatMessage::system(system_prompt),
            ChatMessage::user(task.clone()),
        ];

        let max_iterations = 15;
        let mut final_result = None;

        for _ in 0..max_iterations {
            let model_name = model
                .as_deref()
                .map(|s| s.to_string())
                .unwrap_or_else(|| provider.get_default_model());

            let response = provider
                .chat(
                    messages.clone(),
                    Some(tools.get_definitions().await),
                    Some(model_name.as_str()),
                )
                .await?;

            if response.has_tool_calls() {
                let mut assistant =
                    ChatMessage::assistant(response.content.clone().unwrap_or_default());
                assistant.tool_calls = Some(response.tool_calls.clone());
                messages.push(assistant);

                for tool_call in response.tool_calls {
                    tracing::debug!("Subagent [{}] executing: {}", task_id, tool_call.name);

                    let args = normalize_tool_arguments(&tool_call.arguments)?;
                    let ctx = ToolInvocationContext {
                        session_key: session_key.clone(),
                        channel: "subagent".to_string(),
                        subagent_id: Some(task_id.clone()),
                        session_policy_mode: None,
                    };
                    let result = tool_executor
                        .execute(&tools, &tool_call.name, args, &ctx)
                        .await?;
                    messages.push(ChatMessage::tool(result, tool_call.id));
                }
            } else {
                final_result = response.content;
                break;
            }
        }

        let result = final_result.unwrap_or_else(|| {
            "Task completed but no final response was generated.".to_string()
        });

        tracing::info!("Subagent [{}] completed successfully", task_id);

        Self::announce_result(
            &bus,
            &task_id,
            &label,
            &task,
            &result,
            &origin_channel,
            &origin_chat_id,
        )
        .await;

        Ok(())
    }

    /// 公布子代理结果
    async fn announce_result(
        bus: &MessageBus,
        _task_id: &str,
        label: &str,
        task: &str,
        result: &str,
        origin_channel: &str,
        origin_chat_id: &str,
    ) {
        let content = format!(
            r#"[Subagent '{}' completed successfully]

Task: {}

Result:
{}

Summarize this naturally for the user. Keep it brief (1-2 sentences). Do not mention technical details like "subagent" or task IDs."#,
            label, task, result
        );

        let msg = InboundMessage::new(
            "system",
            "subagent",
            format!("{}:{}", origin_channel, origin_chat_id),
            content,
        );

        let _ = bus.publish_inbound(msg).await;
    }

    /// 构建子代理提示词
    fn build_subagent_prompt(task: &str, workspace: &Path) -> String {
        format!(
            r#"# Subagent

You are a subagent spawned by the main agent to complete a specific task.

## Your Task
{}

## Rules
1. Stay focused - complete only the assigned task, nothing else
2. Your final response will be reported back to the main agent
3. Do not initiate conversations or take on side tasks
4. Be concise but informative in your findings

## What You Can Do
- Read and write files in the workspace (paths must be relative to workspace root)
- Execute shell commands
- Search the web and fetch web pages
- Complete the task thoroughly

## What You Cannot Do
- Send messages directly to users (no message tool available)
- Spawn other subagents
- Access the main agent's conversation history

## Workspace
Your workspace is at: {}

When you have completed the task, provide a clear summary of your findings or actions."#,
            task,
            workspace.display()
        )
    }

    /// 获取运行中的子代理数量
    pub async fn running_count(&self) -> usize {
        self.running_tasks.read().await.len()
    }
}
