//! Agent 循环引擎

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use stormclaw_config::{IngressMode, SecurityConfig};

use crate::agent::subagent::SubagentManager;
use crate::agent::tool_executor::{ToolExecutor, ToolInvocationContext};
use crate::agent::tools::{
    normalize_tool_arguments, EditFileTool, ExecTool, ListDirTool, MessageTool, ReadFileTool,
    SpawnTool, Tool, ToolRegistry, WebFetchTool, WebSearchTool, WriteFileTool,
};
#[cfg(feature = "wasm-tools")]
use crate::agent::tools::WasmEvalTool;
use crate::agent::context::ContextBuilder;
use crate::agent::runtime_state::AgentRuntimeState;
use crate::bus::{InboundMessage, MessageBus, OutboundMessage};
use crate::providers::{ChatMessage, LLMProvider, OpenAIProvider};
use crate::session::SessionManager;

/// Agent 循环引擎
pub struct AgentLoop<P: LLMProvider + 'static> {
    bus: Arc<MessageBus>,
    provider: Arc<P>,
    pub(crate) runtime: Arc<RwLock<AgentRuntimeState>>,
    context: Arc<RwLock<ContextBuilder>>,
    sessions: SessionManager,
    tools: Arc<ToolRegistry>,
    subagents: Arc<SubagentManager<P>>,
    message_tool: Arc<MessageTool>,
    spawn_tool: Arc<SpawnTool<P>>,
    running: Arc<RwLock<bool>>,
    tool_executor: Arc<ToolExecutor>,
}

impl<P: LLMProvider + 'static> AgentLoop<P> {
    /// 创建新的 Agent 循环
    pub async fn new(
        bus: Arc<MessageBus>,
        provider: Arc<P>,
        workspace: PathBuf,
        model: Option<String>,
        max_iterations: usize,
        brave_api_key: Option<String>,
        security: SecurityConfig,
    ) -> anyhow::Result<Self> {
        let runtime = Arc::new(RwLock::new(AgentRuntimeState {
            workspace: workspace.clone(),
            model: model.clone(),
            max_iterations,
            brave_api_key: brave_api_key.clone(),
        }));
        let context = Arc::new(RwLock::new(ContextBuilder::new(workspace.clone())));
        let sessions = SessionManager::new(workspace.clone()).await?;
        let tools = Arc::new(ToolRegistry::new());
        let tool_executor = Arc::new(ToolExecutor::new(security));
        let subagents = Arc::new(SubagentManager::new(
            provider.clone(),
            runtime.clone(),
            bus.clone(),
            tool_executor.clone(),
        ));
        let message_tool = Arc::new(MessageTool::new(bus.clone()));
        let spawn_tool = Arc::new(SpawnTool::new(subagents.clone(), bus.clone()));

        Ok(Self {
            bus,
            provider,
            runtime,
            context,
            sessions,
            tools,
            subagents,
            message_tool,
            spawn_tool,
            running: Arc::new(RwLock::new(false)),
            tool_executor,
        })
    }

    /// 运行 Agent 循环
    pub async fn run(&self) -> anyhow::Result<()> {
        *self.running.write().await = true;
        tracing::info!("Agent loop started");

        self.register_default_tools().await;

        while *self.running.read().await {
            tokio::select! {
                msg = self.bus.consume_inbound() => {
                    if let Some(msg) = msg {
                        if let Err(e) = self.handle_message(msg, None, true).await {
                            tracing::error!("Error processing message: {}", e);
                        }
                    } else {
                        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    }
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => continue,
            }
        }

        Ok(())
    }

    /// 直接处理一条消息并返回响应文本（用于 CLI/Cron/Heartbeat/Gateway 调用）
    pub async fn process_direct(&self, content: &str, session_key: &str) -> anyhow::Result<String> {
        let (channel, chat_id) = if let Some(idx) = session_key.find(':') {
            (session_key[..idx].to_string(), session_key[idx + 1..].to_string())
        } else {
            ("cli".to_string(), session_key.to_string())
        };

        let msg = InboundMessage::new(channel, "user", chat_id, content);
        let out = self.handle_message(msg, Some(session_key.to_string()), false).await?;
        Ok(out.map(|m| m.content).unwrap_or_default())
    }

    pub fn session_manager(&self) -> SessionManager {
        self.sessions.clone()
    }

    pub async fn stop(&self) {
        *self.running.write().await = false;
        tracing::info!("Agent loop stopping");
    }

    async fn apply_ingress(&self, content: &str) -> Result<String, String> {
        let mode = self.tool_executor.security_snapshot().await.ingress_mode;
        match mode {
            IngressMode::Off => Ok(content.to_string()),
            IngressMode::Warn => {
                if let Some(w) = self.tool_executor.scan_inbound_secrets(content).await {
                    tracing::warn!(target: "stormclaw_security", "{}", w);
                }
                let v = self.tool_executor.validate_user_text(content).await;
                if !v.is_valid {
                    tracing::warn!(
                        target: "stormclaw_security",
                        "Ingress validation warnings: {:?}",
                        v.errors
                    );
                }
                Ok(content.to_string())
            }
            IngressMode::Enforce => {
                if let Some(w) = self.tool_executor.scan_inbound_secrets(content).await {
                    return Err(w);
                }
                let v = self.tool_executor.validate_user_text(content).await;
                if !v.is_valid {
                    let msg = v
                        .errors
                        .iter()
                        .map(|e| format!("{}: {}", e.field, e.message))
                        .collect::<Vec<_>>()
                        .join("; ");
                    return Err(format!("Message rejected: {}", msg));
                }
                Ok(content.to_string())
            }
        }
    }

    async fn handle_message(
        &self,
        msg: InboundMessage,
        session_key_override: Option<String>,
        publish_outbound: bool,
    ) -> anyhow::Result<Option<OutboundMessage>> {
        tracing::info!("Processing message from {}:{}", msg.channel, msg.sender_id);

        if msg.channel == "system" {
            return self.process_system_message(msg, publish_outbound).await;
        }

        let session_key = session_key_override.unwrap_or_else(|| msg.session_key());
        let session = self.sessions.get_or_create(&session_key).await?;

        self.message_tool
            .set_context(msg.channel.clone(), msg.chat_id.clone())
            .await;
        self.spawn_tool
            .set_context(msg.channel.clone(), msg.chat_id.clone())
            .await;

        let user_body = match self.apply_ingress(&msg.content).await {
            Ok(s) => s,
            Err(user_reply) => {
                let response =
                    OutboundMessage::new(msg.channel.clone(), msg.chat_id.clone(), user_reply);
                if publish_outbound {
                    self.bus.publish_outbound(response.clone()).await?;
                }
                return Ok(Some(response));
            }
        };

        let mut messages = self.build_llm_messages(&session, &user_body).await?;

        let mut final_content = None;

        let max_iterations = self.runtime.read().await.max_iterations;
        for _ in 0..max_iterations {
            let model = {
                let rt = self.runtime.read().await;
                rt.model
                    .clone()
                    .unwrap_or_else(|| self.provider.get_default_model())
            };

            let response = self
                .provider
                .chat(
                    messages.clone(),
                    Some(self.tools.get_definitions().await),
                    Some(model.as_str()),
                )
                .await?;

            if response.has_tool_calls() {
                let mut assistant =
                    ChatMessage::assistant(response.content.clone().unwrap_or_default());
                assistant.tool_calls = Some(response.tool_calls.clone());
                messages.push(assistant);

                for tool_call in response.tool_calls {
                    tracing::debug!("Executing tool: {}", tool_call.name);

                    let args = normalize_tool_arguments(&tool_call.arguments)?;
                    let ctx = ToolInvocationContext {
                        session_key: session_key.clone(),
                        channel: msg.channel.clone(),
                        subagent_id: None,
                        session_policy_mode: None,
                    };
                    let result = self
                        .tool_executor
                        .execute(&self.tools, &tool_call.name, args, &ctx)
                        .await?;

                    messages.push(ChatMessage::tool(result, tool_call.id));
                }
            } else {
                final_content = response.content;
                break;
            }
        }

        let content = final_content.unwrap_or_else(|| {
            "I've completed processing but have no response to give.".to_string()
        });

        let mut session = session;
        session.add_message("user", &user_body);
        session.add_message("assistant", &content);
        self.sessions.save(&session).await?;

        let response = OutboundMessage::new(msg.channel, msg.chat_id, content);
        if publish_outbound {
            self.bus.publish_outbound(response.clone()).await?;
        }

        Ok(Some(response))
    }

    async fn process_system_message(
        &self,
        msg: InboundMessage,
        publish_outbound: bool,
    ) -> anyhow::Result<Option<OutboundMessage>> {
        tracing::info!("Processing system message from {}", msg.sender_id);

        let (origin_channel, origin_chat_id) = if let Some(idx) = msg.chat_id.find(':') {
            (&msg.chat_id[..idx], &msg.chat_id[idx + 1..])
        } else {
            ("cli", msg.chat_id.as_str())
        };

        let session_key = format!("{}:{}", origin_channel, origin_chat_id);
        let session = self.sessions.get_or_create(&session_key).await?;

        self.message_tool
            .set_context(origin_channel.to_string(), origin_chat_id.to_string())
            .await;
        self.spawn_tool
            .set_context(origin_channel.to_string(), origin_chat_id.to_string())
            .await;

        let mut messages = self.build_llm_messages(&session, &msg.content).await?;

        let max_iterations = self.runtime.read().await.max_iterations;
        for _ in 0..max_iterations {
            let model = {
                let rt = self.runtime.read().await;
                rt.model
                    .clone()
                    .unwrap_or_else(|| self.provider.get_default_model())
            };

            let response = self
                .provider
                .chat(
                    messages.clone(),
                    Some(self.tools.get_definitions().await),
                    Some(model.as_str()),
                )
                .await?;

            if response.has_tool_calls() {
                let mut assistant =
                    ChatMessage::assistant(response.content.clone().unwrap_or_default());
                assistant.tool_calls = Some(response.tool_calls.clone());
                messages.push(assistant);

                for tool_call in response.tool_calls {
                    let args = normalize_tool_arguments(&tool_call.arguments)?;
                    let ctx = ToolInvocationContext {
                        session_key: session_key.clone(),
                        channel: origin_channel.to_string(),
                        subagent_id: None,
                        session_policy_mode: None,
                    };
                    let result = self
                        .tool_executor
                        .execute(&self.tools, &tool_call.name, args, &ctx)
                        .await?;
                    messages.push(ChatMessage::tool(result, tool_call.id));
                }
            } else {
                let content = response.content.unwrap_or_default();

                let mut session = session;
                session.add_message(
                    "user",
                    &format!("[System: {}] {}", msg.sender_id, msg.content),
                );
                session.add_message("assistant", &content);
                self.sessions.save(&session).await?;

                let response = OutboundMessage::new(origin_channel, origin_chat_id, content);
                if publish_outbound {
                    self.bus.publish_outbound(response.clone()).await?;
                }
                break;
            }
        }

        Ok(None)
    }

    async fn build_llm_messages(
        &self,
        session: &crate::session::Session,
        current_message: &str,
    ) -> anyhow::Result<Vec<ChatMessage>> {
        let mut messages = Vec::new();
        let system_prompt = self.context.read().await.build_system_prompt(None);
        messages.push(ChatMessage::system(system_prompt));

        for m in session.get_history(50) {
            let role = m.get("role").cloned().unwrap_or_default();
            let content = m.get("content").cloned().unwrap_or_default();
            messages.push(ChatMessage {
                role,
                content,
                tool_call_id: None,
                tool_calls: None,
            });
        }

        messages.push(ChatMessage::user(current_message));
        Ok(messages)
    }

    async fn register_default_tools(&self) {
        let (workspace, brave_api_key) = {
            let rt = self.runtime.read().await;
            (rt.workspace.clone(), rt.brave_api_key.clone())
        };

        let sec = self.tool_executor.security_snapshot().await;
        let docker = sec.docker.clone();

        self.tools
            .register(Arc::new(ReadFileTool {
                workspace_root: workspace.clone(),
            }) as Arc<dyn Tool>)
            .await;
        self.tools
            .register(Arc::new(WriteFileTool {
                workspace_root: workspace.clone(),
            }) as Arc<dyn Tool>)
            .await;
        self.tools
            .register(Arc::new(EditFileTool {
                workspace_root: workspace.clone(),
            }) as Arc<dyn Tool>)
            .await;
        self.tools
            .register(Arc::new(ListDirTool {
                workspace_root: workspace.clone(),
            }) as Arc<dyn Tool>)
            .await;

        self.tools
            .register(Arc::new(ExecTool::new(workspace.clone(), docker)) as Arc<dyn Tool>)
            .await;

        self.tools
            .register(Arc::new(WebSearchTool::new(brave_api_key)) as Arc<dyn Tool>)
            .await;
        self.tools
            .register(Arc::new(WebFetchTool) as Arc<dyn Tool>)
            .await;

        self.tools
            .register(self.message_tool.clone() as Arc<dyn Tool>)
            .await;

        self.tools
            .register(self.spawn_tool.clone() as Arc<dyn Tool>)
            .await;

        #[cfg(feature = "wasm-tools")]
        if sec.wasm_tools_enabled {
            self.tools
                .register(Arc::new(WasmEvalTool) as Arc<dyn Tool>)
                .await;
        }
    }
}

impl AgentLoop<OpenAIProvider> {
    pub async fn apply_gateway_hot_reload(
        &self,
        api_key: String,
        api_base: Option<String>,
        default_model: String,
        max_iterations: usize,
        brave_api_key: Option<String>,
        new_workspace: PathBuf,
        security: SecurityConfig,
    ) -> anyhow::Result<()> {
        self.provider
            .apply_credentials(api_key, api_base, default_model.clone());

        {
            let mut rt = self.runtime.write().await;
            if new_workspace != rt.workspace {
                tracing::warn!(
                    "热重载：配置工作区 {:?} 与运行中 {:?} 不一致；不切换工作区，请重启网关以应用新路径。",
                    new_workspace,
                    rt.workspace
                );
            }
            rt.model = Some(default_model);
            rt.max_iterations = max_iterations;
            rt.brave_api_key = brave_api_key;
        }

        self.tool_executor.refresh_security(security).await;

        self.tools.clear().await;
        self.register_default_tools().await;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{create_test_workspace, MockLLMProvider};

    async fn create_test_agent_loop() -> anyhow::Result<(AgentLoop<MockLLMProvider>, tempfile::TempDir)>
    {
        let (workspace, temp_dir) = create_test_workspace().await?;
        let bus = Arc::new(MessageBus::new(1000));
        let provider = Arc::new(MockLLMProvider::new().with_model("gpt-4"));

        let agent = AgentLoop::new(
            bus,
            provider,
            workspace,
            Some("gpt-4".to_string()),
            5,
            None,
            SecurityConfig::default(),
        )
        .await?;

        Ok((agent, temp_dir))
    }

    #[tokio::test]
    async fn test_agent_loop_creation() {
        let (agent, _temp_dir) = create_test_agent_loop().await.unwrap();
        assert!(!*agent.running.read().await);
    }

    #[tokio::test]
    async fn test_agent_loop_start_stop() {
        let (agent, _temp_dir) = create_test_agent_loop().await.unwrap();

        let agent = Arc::new(agent);

        let agent_for_task = agent.clone();
        let handle = tokio::spawn(async move { agent_for_task.run().await });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        assert!(*agent.running.read().await);

        agent.stop().await;
        let joined = tokio::time::timeout(tokio::time::Duration::from_millis(300), handle).await;
        assert!(joined.is_ok());
    }

    #[tokio::test]
    async fn test_agent_tools_registered() {
        let (agent, _temp_dir) = create_test_agent_loop().await.unwrap();

        agent.register_default_tools().await;

        assert!(agent.tools.has("read_file").await);
        assert!(agent.tools.has("write_file").await);
        assert!(agent.tools.has("exec").await);
        assert!(agent.tools.has("web_search").await);
    }

    #[tokio::test]
    async fn test_build_llm_messages() {
        let (agent, _temp_dir) = create_test_agent_loop().await.unwrap();
        let session = agent.sessions.get_or_create("cli:direct").await.unwrap();
        let messages = agent
            .build_llm_messages(&session, "Current message")
            .await
            .unwrap();

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[1].content, "Current message");
    }

    #[tokio::test]
    async fn test_agent_max_iterations() {
        let (workspace, temp_dir) = create_test_workspace().await.unwrap();
        let bus = Arc::new(MessageBus::new(1000));
        let provider = Arc::new(MockLLMProvider::new().with_model("gpt-4"));

        let agent = AgentLoop::new(
            bus,
            provider,
            workspace,
            Some("gpt-4".to_string()),
            3,
            None,
            SecurityConfig::default(),
        )
        .await
        .unwrap();

        assert_eq!(agent.runtime.read().await.max_iterations, 3);

        drop(temp_dir);
    }

    #[tokio::test]
    async fn test_agent_model_override() {
        let (workspace, temp_dir) = create_test_workspace().await.unwrap();
        let bus = Arc::new(MessageBus::new(1000));
        let provider = Arc::new(MockLLMProvider::new().with_model("gpt-3.5"));

        let agent = AgentLoop::new(
            bus,
            provider,
            workspace,
            Some("custom-model".to_string()),
            5,
            None,
            SecurityConfig::default(),
        )
        .await
        .unwrap();

        assert_eq!(
            agent.runtime.read().await.model.as_ref().unwrap(),
            "custom-model"
        );

        drop(temp_dir);
    }

    #[tokio::test]
    async fn test_agent_session_integration() {
        let (agent, _temp_dir) = create_test_agent_loop().await.unwrap();

        let session = agent
            .sessions
            .get_or_create("telegram:chat123")
            .await
            .unwrap();
        assert_eq!(session.key, "telegram:chat123");
    }

    #[tokio::test]
    async fn test_agent_tools_execution() {
        let (agent, _temp_dir) = create_test_agent_loop().await.unwrap();
        agent.register_default_tools().await;

        let read_file_tool = agent.tools.get("read_file").await;
        assert!(read_file_tool.is_some());
        assert_eq!(read_file_tool.unwrap().name(), "read_file");
    }
}
