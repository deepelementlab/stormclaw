# stormclaw Rust 版本实现文档

## 1. 实现概述

### 1.1 实现范围

本项目完整实现了 stormclaw Python 版本的所有核心功能，包括：

- **Agent 引擎**: LLM 对话循环、工具调用、上下文管理
- **工具系统**: 9 种内置工具，支持自定义扩展
- **会话管理**: 多会话支持、JSONL 持久化
- **记忆系统**: 长期和短期记忆
- **技能系统**: 动态加载、依赖管理
- **渠道适配**: 6 种聊天平台支持
- **服务层**: 网关、定时任务、心跳、监管等
- **CLI 应用**: 完整的命令行界面

### 1.2 代码统计

| 模块 | 文件数 | 代码行数 |
|------|--------|----------|
| 核心 (core) | 17 | ~2800 |
| 渠道 (channels) | 14 | ~2200 |
| 服务 (services) | 8 | ~2900 |
| 配置 (config) | 3 | ~300 |
| 工具 (utils) | 4 | ~150 |
| CLI (cli) | 8 | ~600 |
| **总计** | **61** | **~9775** |

## 2. 核心模块实现

### 2.1 消息总线实现

#### 2.1.1 消息定义

文件位置: `crates/core/src/bus/events.rs`

```rust
/// 入站消息 - 从渠道发送到 Agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundMessage {
    pub channel: String,      // 渠道名称
    pub sender_id: String,    // 发送者 ID
    pub chat_id: String,      // 聊天 ID
    pub content: String,      // 消息内容
    pub timestamp: DateTime<Utc>,
    pub media: Vec<String>,   // 媒体附件
    pub metadata: Value,      // 渠道特定元数据
}

impl InboundMessage {
    pub fn session_key(&self) -> String {
        format!("{}:{}", self.channel, self.chat_id)
    }
}
```

#### 2.1.2 消息队列实现

文件位置: `crates/core/src/bus/queue.rs`

核心功能：
- 异步消息通道 (mpsc)
- 广播订阅 (broadcast)
- 线程安全的状态管理

```rust
pub struct MessageBus {
    inbound_tx: mpsc::Sender<InboundMessage>,
    inbound_rx: Option<mpsc::Receiver<InboundMessage>>,
    outbound_tx: mpsc::Sender<OutboundMessage>,
    outbound_rx: Option<mpsc::Receiver<OutboundMessage>>,
    subscribers: HashMap<String, broadcast::Sender<OutboundMessage>>,
}
```

实现要点：
1. 使用 `mpsc::channel` 实现点对点消息传递
2. 使用 `broadcast::channel` 实现一对多消息广播
3. 提供线程安全的 `publish/consume` 接口

### 2.2 LLM 提供商实现

#### 2.2.1 提供商接口

文件位置: `crates/core/src/providers/base.rs`

```rust
#[async_trait]
pub trait LLMProvider: Send + Sync {
    async fn chat(
        &self,
        messages: Vec<ChatMessage>,
        tools: Option<Vec<ToolDefinition>>,
        model: Option<&str>,
    ) -> anyhow::Result<LLMResponse>;

    fn get_default_model(&self) -> &str;
}
```

#### 2.2.2 OpenAI 兼容实现

文件位置: `crates/core/src/providers/openai.rs`

实现细节：
1. 使用 `reqwest::Client` 进行 HTTP 请求
2. 支持 OpenAI、Anthropic、OpenRouter 等兼容 API
3. 自动处理工具调用和重试逻辑

```rust
pub struct OpenAIProvider {
    client: Arc<reqwest::Client>,
    api_key: String,
    api_base: String,
    default_model: String,
}

impl OpenAIProvider {
    pub fn new(api_key: String, api_base: Option<String>, default_model: String) -> Self {
        let client = Arc::new(
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("Failed to create HTTP client")
        );

        Self {
            client,
            api_key,
            api_base: api_base.unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
            default_model,
        }
    }
}
```

### 2.3 Agent 引擎实现

#### 2.3.1 Agent 循环

文件位置: `crates/core/src/agent/loop.rs`

核心流程：
```
接收消息 → 获取会话 → 构建上下文 →
LLM 调用循环 → 工具执行 → 响应发送
```

实现要点：
1. 工具调用循环：最多执行 N 次迭代
2. 上下文构建：系统提示词 + 历史消息 + 当前消息
3. 会话持久化：每次处理后保存会话

```rust
for _ in 0..self.max_iterations {
    let response = self.provider.chat(
        messages.iter().map(|m| ChatMessage {
            role: m.get("role").cloned().unwrap_or_default(),
            content: m.get("content").cloned().unwrap_or_default(),
            tool_call_id: None,
            tool_calls: None,
        }).collect(),
        Some(self.tools.get_definitions().await),
        Some(model),
    ).await?;

    if response.has_tool_calls() {
        // 执行工具
        for tool_call in response.tool_calls {
            let result = self.tools.execute(&tool_call.name, tool_call.arguments).await?;
            // 添加工具结果到消息列表
        }
    } else {
        final_content = response.content;
        break;
    }
}
```

#### 2.3.2 上下文构建

文件位置: `crates/core/src/agent/context.rs`

功能：
1. 构建系统提示词（身份 + 引导文件 + 技能）
2. 组装消息列表（系统 + 历史 + 当前）
3. 支持技能动态加载

```rust
pub fn build_system_prompt(&self, skill_names: Option<Vec<String>>) -> String {
    let mut parts = Vec::new();

    // 核心身份
    parts.push(self.get_identity());

    // 引导文件
    let bootstrap = self.load_bootstrap_files();
    if !bootstrap.is_empty() {
        parts.push(bootstrap);
    }

    // 技能
    let skills_summary = self.build_skills_summary();
    if !skills_summary.is_empty() {
        parts.push(format!("# Skills\n\n{}", skills_summary));
    }

    parts.join("\n\n---\n\n")
}
```

### 2.4 工具系统实现

#### 2.4.1 工具接口

文件位置: `crates/core/src/agent/tools/base.rs`

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    async fn execute(&self, args: Value) -> anyhow::Result<String>;

    fn to_schema(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name(),
                "description": self.description(),
                "parameters": self.parameters()
            }
        })
    }
}
```

#### 2.4.2 工具注册表

文件位置: `crates/core/src/agent/tools/registry.rs`

实现要点：
1. 使用 `RwLock<HashMap>` 存储工具
2. 线程安全的注册和调用
3. 自动生成 OpenAI 函数格式

#### 2.4.3 内置工具实现

| 工具 | 文件 | 功能 |
|------|------|------|
| ReadFileTool | filesystem.rs | 读取文件 |
| WriteFileTool | filesystem.rs | 写入文件 |
| EditFileTool | filesystem.rs | 编辑文件 |
| ListDirTool | filesystem.rs | 列出目录 |
| ExecTool | shell.rs | 执行命令 |
| WebSearchTool | web.rs | Web 搜索 |
| WebFetchTool | web.rs | 获取网页 |
| MessageTool | message.rs | 发送消息 |
| SpawnTool | spawn.rs | 生成子代理 |

### 2.5 会话管理实现

文件位置: `crates/core/src/session/manager.rs`

存储格式：JSONL (每行一个 JSON 对象)

```json
{"_type": "metadata", "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-01-01T00:00:00Z", "metadata": {}}
{"role": "user", "content": "Hello", "timestamp": "2024-01-01T00:00:00Z"}
{"role": "assistant", "content": "Hi there!", "timestamp": "2024-01-01T00:00:00Z"}
```

实现要点：
1. 内存缓存 + 磁盘持久化
2. 自动保存修改
3. 支持历史记录查询

### 2.6 子代理系统实现

文件位置: `crates/core/src/agent/subagent.rs`

设计思路：
1. 子代理运行在独立的 tokio 任务中
2. 通过消息总线与主代理通信
3. 子代理只能使用有限工具集（无 message、spawn）

```rust
pub async fn spawn(
    &self,
    task: String,
    label: Option<String>,
    origin_channel: String,
    origin_chat_id: String,
) -> anyhow::Result<String> {
    let task_id = Uuid::new_v4().to_string()[..8].to_string();

    let handle = tokio::spawn(async move {
        // 运行子代理任务
        Self::run_subagent(...).await?;

        // 公布结果
        Self::announce_result(...).await;
    });

    Ok(format!("Subagent [{}] started (id: {})", display_label, task_id))
}
```

## 3. 渠道适配实现

### 3.1 渠道接口

文件位置: `crates/channels/src/base.rs`

```rust
#[async_trait]
pub trait BaseChannel: Send + Sync {
    fn name(&self) -> &str;
    async fn start(&self) -> Result<()>;
    async fn stop(&self) -> Result<()>;
    async fn send(&self, msg: &OutboundMessage) -> Result<()>;
    fn is_allowed(&self, sender_id: &str) -> bool { true }
    fn bus(&self) -> &MessageBus;
    fn is_running(&self) -> bool { false }
}
```

### 3.2 Telegram 渠道

文件位置: `crates/channels/src/telegram.rs`

实现细节：
1. 使用 `teloxide` 库
2. 支持 HTML 解析模式
3. 自动处理消息分发

```rust
pub struct TelegramChannel {
    bot: AutoSend<Bot>,
    token: String,
    allow_from: Vec<String>,
    bus: Arc<MessageBus>,
    state: Arc<RwLock<ChannelState>>,
    running: Arc<AtomicBool>,
}
```

### 3.3 WhatsApp 渠道

文件位置: `crates/channels/src/whatsapp.rs`

实现细节：
1. 通过 WebSocket 连接到 Node.js 网桥
2. 支持消息队列和批量发送
3. 自动重连机制

```rust
pub struct WhatsAppChannel {
    bridge_url: String,
    allow_from: Vec<String>,
    bus: Arc<MessageBus>,
    state: Arc<RwLock<ChannelState>>,
    running: Arc<AtomicBool>,
    ws_sender: Arc<RwLock<Option<...>>>,
    pending_messages: Arc<RwLock<HashMap<String, Vec<String>>>>,
}
```

### 3.4 渠道管理器

文件位置: `crates/channels/src/manager.rs`

功能：
1. 根据配置初始化渠道
2. 统一启动/停止所有渠道
3. 出站消息分发

```rust
pub struct ChannelManager {
    channels: Arc<RwLock<HashMap<String, Arc<dyn BaseChannel>>>>,
    bus: Arc<MessageBus>,
    config: ChannelsConfig,
    dispatch_task: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl ChannelManager {
    pub async fn initialize(&self) -> Result<()> {
        // 根据配置创建渠道
        if self.config.telegram.enabled {
            let channel = ChannelFactory::create_telegram(&self.config.telegram, self.bus.clone())?;
            channels.insert("telegram".to_string(), channel);
        }
        // ...
    }
}
```

## 4. 服务层实现

### 4.1 定时任务服务

文件位置: `crates/services/src/cron.rs`

功能：
1. 支持 cron 表达式调度
2. 支持间隔任务
3. 支持一次性任务
4. 任务持久化

```rust
pub struct CronService {
    jobs: Arc<RwLock<HashMap<String, CronJob>>>,
    scheduler: Arc<tokio_cron_scheduler::JobScheduler>,
    data_dir: PathBuf,
    running: Arc<RwLock<bool>>,
}
```

### 4.2 心跳服务

文件位置: `crates/services/src/heartbeat.rs`

功能：
1. 定期检查 HEARTBEAT.md 文件
2. 执行文件中的任务
3. 支持自定义回调

```rust
pub struct HeartbeatService {
    workspace: PathBuf,
    config: HeartbeatConfig,
    callback: Arc<RwLock<Option<HeartbeatCallback>>>,
    status: Arc<RwLock<HeartbeatStatus>>,
    running: Arc<RwLock<bool>>,
}
```

### 4.3 网关服务

文件位置: `crates/services/src/gateway.rs`

功能：
1. 统一服务入口
2. 协调所有子服务
3. 提供 HTTP API
4. 优雅关闭

```rust
pub struct GatewayService {
    config: GatewayConfig,
    bus: Arc<MessageBus>,
    agent: Arc<AgentLoop<Arc<dyn LLMProvider>>>,
    channels: Arc<ChannelManager>,
    cron: Arc<CronService>,
    heartbeat: Arc<HeartbeatService>,
    lifecycle: Arc<ServiceLifecycle>,
    running: Arc<RwLock<bool>>,
}
```

### 4.4 服务监管器

文件位置: `crates/services/src/supervisor.rs`

功能：
1. 监控服务状态
2. 自动重启失败服务
3. 支持多种重启策略

```rust
pub struct ServiceSupervisor {
    services: Arc<RwLock<HashMap<String, ServiceState>>>,
    event_sender: mpsc::UnboundedSender<SupervisorEvent>,
    event_receiver: Arc<RwLock<Option<mpsc::UnboundedReceiver<SupervisorEvent>>>>,
    restart_policy: RestartPolicy,
    running: Arc<RwLock<bool>>,
}
```

## 5. CLI 应用实现

### 5.1 命令结构

```
stormclaw
├── onboard      # 初始化配置
├── agent        # Agent 交互
├── gateway      # 启动网关
├── status       # 显示状态
├── channels     # 渠道管理
└── cron         # 定时任务管理
```

### 5.2 交互模式

文件位置: `cli/src/commands/agent.rs`

功能：
1. 支持 rustyline 历史记录
2. 支持命令补全
3. 内置命令 (/help, /quit, /clear 等)

```rust
pub async fn run_interactive_mode(...) -> Result<()> {
    let mut rl = DefaultEditor::new()?;

    loop {
        let input = rl.readline("你: ")?;

        if input.starts_with('/') {
            handle_command(&input, &mut rl)?;
        } else {
            // 发送到 Agent
        }
    }
}
```

## 6. 配置管理实现

### 6.1 配置结构

文件位置: `crates/config/src/schema.rs`

支持 camelCase JSON 配置：

```json
{
  "agents": {
    "defaults": {
      "model": "anthropic/claude-opus-4-5",
      "maxTokens": 8192,
      "temperature": 0.7
    }
  },
  "providers": {
    "openrouter": {
      "apiKey": "sk-or-v1-xxx"
    }
  }
}
```

### 6.2 配置加载

文件位置: `crates/config/src/loader.rs`

功能：
1. 自动展开环境变量 ${VAR_NAME}
2. 支持默认值
3. 配置验证

## 7. 构建和部署

### 7.1 构建脚本

```bash
# 开发构建
cargo build

# 发布构建
cargo build --release

# 交叉编译
cargo build --release --target x86_64-unknown-linux-gnu
```

### 7.2 Docker 部署

```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates
COPY --from=builder /app/target/release/stormclaw /usr/local/bin/
WORKDIR /root/.stormclaw
ENTRYPOINT ["stormclaw"]
```

## 8. 测试策略

### 8.1 单元测试

每个模块包含单元测试：

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_safe_filename() {
        assert_eq!(safe_filename("test:file"), "test_file");
    }
}
```

### 8.2 集成测试

使用测试渠道进行集成测试：

```rust
// crates/channels/src/testing.rs
pub struct ChannelTester {
    test_results: Arc<RwLock<Vec<TestResult>>>>,
}

impl ChannelTester {
    pub async fn test_channel(&self, channel: Arc<dyn BaseChannel>) -> TestResult {
        // 测试渠道功能
    }
}
```

## 9. 性能优化实现

### 9.1 异步优化

1. 使用 `tokio::spawn` 并行处理独立任务
2. 使用 `tokio::select!` 多路复用
3. 使用 `RwLock` 而非 `Mutex` 减少锁竞争

### 9.2 内存优化

1. 使用 `Arc` 共享只读数据
2. 使用 `Cow` 避免不必要的字符串分配
3. 使用 `bytes::Bytes` 处理大块数据

### 9.3 网络优化

1. HTTP 连接池复用
2. WebSocket 心跳保活
3. 批量消息处理

## 10. 总结

stormclaw Rust 版本实现了与 Python 版本完全对等的功能，在性能上预计有显著提升。通过模块化的架构设计和 Rust 的类型安全保证，代码具有良好的可维护性和可靠性。
