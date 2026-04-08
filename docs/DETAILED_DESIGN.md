# stormclaw Rust 版本详细设计文档

## 1. 系统架构设计

### 1.1 整体架构

```
┌─────────────────────────────────────────────────────────────────┐
│                        CLI Application                         │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐       │
│  │ Onboard   │  │ Agent    │  │ Gateway  │  │ Status   │       │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘       │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Gateway Service                            │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐               │
│  │    Agent   │  │  Channels  │  │  Services  │               │
│  │    Loop    │  │  Manager   │  │ Supervisor │               │
│  └────────────┘  └────────────┘  └────────────┘               │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                       Message Bus                               │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  Inbound Queue  │  │  Outbound Queue                     │  │
│  └──────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                              │
        ┌─────────────────────┼─────────────────────┐
        ▼                     ▼                     ▼
┌───────────────┐    ┌───────────────┐    ┌───────────────┐
│   Channels    │    │     Core      │    │   Services    │
│ ┌───────────┐ │    │ ┌───────────┐ │    │ ┌───────────┐ │
│ │Telegram   │ │    │ │   Agent   │ │    │ │  Cron     │ │
│ │WhatsApp   │ │    │ │   Tools   │ │    │ │Heartbeat  │ │
│ │Discord    │ │    │ │  Session  │ │    │ │Lifecycle  │ │
│ │Slack      │ │    │ │  Memory   │ │    │ │ Metrics   │ │
│ │CLI        │ │    │ │  Skills   │ │    │ │  Logging  │ │
│ └───────────┘ │    │ └───────────┘ │    │ └───────────┘ │
└───────────────┘    └───────────────┘    └───────────────┘
        │                     │                     │
        └─────────────────────┼─────────────────────┘
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      LLM Provider                                │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │           OpenAI Compatible API                            │ │
│  └────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

### 1.2 分层架构

#### 1.2.1 表现层 (CLI)

**职责：**
- 用户交互界面
- 命令解析和执行
- 输入输出处理

**模块：**
```rust
cli/
├── src/
│   ├── main.rs              # 主入口
│   └── commands/
│       ├── mod.rs           # 命令模块
│       ├── onboard.rs       # 初始化命令
│       ├── agent.rs         # Agent 交互
│       ├── gateway.rs       # 网关服务
│       ├── status.rs        # 状态查询
│       ├── channels.rs      # 渠道管理
│       └── cron.rs          # 定时任务
```

#### 1.2.2 服务层 (Services)

**职责：**
- 业务逻辑协调
- 服务生命周期管理
- 跨模块协调

**模块：**
```rust
services/
├── src/
│   ├── gateway.rs           # 网关服务
│   ├── supervisor.rs        # 服务监管
│   ├── lifecycle.rs         # 生命周期管理
│   ├── cron.rs              # 定时任务
│   ├── heartbeat.rs         # 心跳服务
│   ├── config_reload.rs     # 配置热重载
│   ├── logging.rs           # 日志服务
│   └── metrics.rs           # 指标收集
```

#### 1.2.3 核心层 (Core)

**职责：**
- Agent 引擎实现
- 工具系统
- 会话和记忆管理

**模块：**
```rust
core/
├── src/
│   ├── lib.rs
│   ├── bus/                 # 消息总线
│   │   ├── events.rs         # 事件定义
│   │   ├── queue.rs          # 消息队列
│   │   └── mod.rs
│   ├── agent/               # Agent 引擎
│   │   ├── mod.rs
│   │   ├── loop.rs           # 主循环
│   │   ├── context.rs        # 上下文构建
│   │   ├── subagent.rs       # 子代理
│   │   └── tools/            # 工具系统
│   │       ├── base.rs       # 工具接口
│   │       ├── registry.rs   # 工具注册
│   │       ├── filesystem.rs # 文件工具
│   │       ├── shell.rs      # Shell 工具
│   │       ├── web.rs        # Web 工具
│   │       ├── message.rs    # 消息工具
│   │       └── spawn.rs      # 子代理工具
│   ├── providers/            # LLM 提供商
│   │   ├── base.rs           # 接口定义
│   │   ├── openai.rs         # OpenAI 实现
│   │   └── mod.rs
│   ├── session/              # 会话管理
│   │   ├── manager.rs
│   │   └── mod.rs
│   ├── memory/               # 记忆系统
│   │   └── mod.rs
│   └── skills/               # 技能系统
│       └── mod.rs
```

#### 1.2.4 渠道层 (Channels)

**职责：**
- 外部平台适配
- 协议转换
- 消息路由

**模块：**
```rust
channels/
├── src/
│   ├── lib.rs
│   ├── base.rs              # 渠道接口
│   ├── manager.rs           # 渠道管理器
│   ├── telegram.rs          # Telegram 实现
│   ├── whatsapp.rs          # WhatsApp 实现
│   ├── discord.rs           # Discord 实现
│   ├── slack.rs             # Slack 实现
│   ├── cli.rs               # CLI 实现
│   ├── webhook.rs           # Webhook 实现
│   ├── email.rs             # Email 实现
│   ├── converter.rs         # 消息转换
│   ├── template.rs          # 模板系统
│   ├── testing.rs           # 测试工具
│   └── monitor.rs           # 监控工具
```

#### 1.2.5 基础设施层 (Config & Utils)

**职责：**
- 配置管理
- 通用工具函数

**模块：**
```rust
config/
├── src/
│   ├── lib.rs
│   ├── schema.rs            # 配置结构
│   └── loader.rs            # 配置加载

utils/
├── src/
│   ├── lib.rs
│   ├── fs.rs                # 文件系统工具
│   ├── time.rs              # 时间工具
│   └── path.rs              # 路径处理
```

## 2. 核心组件设计

### 2.1 消息总线设计

#### 2.1.1 消息定义

```rust
/// 入站消息 - 从渠道到 Agent
pub struct InboundMessage {
    pub channel: String,
    pub sender_id: String,
    pub chat_id: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub media: Vec<String>,
    pub metadata: Value,
}

/// 出站消息 - 从 Agent 到渠道
pub struct OutboundMessage {
    pub channel: String,
    pub chat_id: String,
    pub content: String,
    pub reply_to: Option<String>,
    pub media: Vec<String>,
    pub metadata: Value,
}
```

#### 2.1.2 消息队列

```rust
pub struct MessageBus {
    inbound_tx: mpsc::Sender<InboundMessage>,
    inbound_rx: Option<mpsc::Receiver<InboundMessage>>,
    outbound_tx: mpsc::Sender<OutboundMessage>,
    outbound_rx: Option<mpsc::Receiver<OutboundMessage>>,
    subscribers: HashMap<String, broadcast::Sender<OutboundMessage>>,
}

impl MessageBus {
    // 发布入站消息
    pub async fn publish_inbound(&self, msg: InboundMessage) -> Result<()>;

    // 消费入站消息
    pub async fn consume_inbound(&mut self) -> Option<InboundMessage>;

    // 发布出站消息
    pub async fn publish_outbound(&self, msg: OutboundMessage) -> Result<()>;

    // 消费出站消息
    pub async fn consume_outbound(&mut self) -> Option<OutboundMessage>;

    // 订阅渠道
    pub fn subscribe_outbound(&mut self, channel: String) -> broadcast::Receiver<OutboundMessage>;
}
```

### 2.2 Agent 引擎设计

#### 2.2.1 Agent 循环

```rust
pub struct AgentLoop<P: LLMProvider> {
    bus: Arc<MessageBus>,
    provider: Arc<P>,
    workspace: PathBuf,
    model: Option<String>,
    max_iterations: usize,
    context: ContextBuilder,
    sessions: SessionManager,
    tools: Arc<ToolRegistry>,
    subagents: Arc<SubagentManager<P>>,
    running: Arc<RwLock<bool>>,
}

impl<P: LLMProvider> AgentLoop<P> {
    /// 运行 Agent 循环
    pub async fn run(&self) -> Result<()> {
        // 1. 注册默认工具
        // 2. 订阅入站消息
        // 3. 处理消息循环
    }

    /// 处理消息
    async fn process_message(&self, msg: InboundMessage) -> Result<()> {
        // 1. 获取会话
        // 2. 构建上下文
        // 3. Agent 循环
        // 4. 保存会话
        // 5. 发送响应
    }
}
```

#### 2.2.2 上下文构建

```rust
pub struct ContextBuilder {
    workspace: PathBuf,
}

impl ContextBuilder {
    /// 构建系统提示词
    pub fn build_system_prompt(&self, skill_names: Option<Vec<String>>) -> String {
        // 1. 核心身份
        // 2. 引导文件 (AGENTS.md, SOUL.md, USER.md, TOOLS.md)
        // 3. 技能摘要
    }

    /// 构建消息列表
    pub fn build_messages(
        &self,
        history: Vec<HashMap<String, String>>,
        current_message: &str,
        skill_names: Option<Vec<String>>,
    ) -> Vec<HashMap<String, String>> {
        // 1. 系统提示词
        // 2. 历史消息
        // 3. 当前消息
    }
}
```

### 2.3 工具系统设计

#### 2.3.1 工具接口

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    async fn execute(&self, args: Value) -> Result<String>;

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

#### 2.3.2 工具注册表

```rust
pub struct ToolRegistry {
    tools: Arc<RwLock<HashMap<String, Arc<dyn Tool>>>>,
}

impl ToolRegistry {
    pub async fn register(&self, tool: Arc<dyn Tool>);
    pub async fn get(&self, name: &str) -> Option<Arc<dyn Tool>>;
    pub async fn get_definitions(&self) -> Vec<Value>;
    pub async fn execute(&self, name: &str, args: Value) -> Result<String>;
}
```

#### 2.3.3 内置工具

| 工具名称 | 功能 | 参数 |
|---------|------|------|
| read_file | 读取文件 | path: String |
| write_file | 写入文件 | path: String, content: String |
| edit_file | 编辑文件 | path: String, old_text: String, new_text: String |
| list_dir | 列出目录 | path: String |
| exec | 执行 Shell 命令 | command: String |
| web_search | Web 搜索 | query: String, count?: Number |
| web_fetch | 获取网页 | url: String |
| message | 发送消息 | content: String, channel?: String, chat_id?: String |
| spawn | 生成子代理 | task: String, label?: String |

### 2.4 LLM 提供商设计

#### 2.4.1 提供商接口

```rust
#[async_trait]
pub trait LLMProvider: Send + Sync {
    async fn chat(
        &self,
        messages: Vec<ChatMessage>,
        tools: Option<Vec<ToolDefinition>>,
        model: Option<&str>,
    ) -> Result<LLMResponse>;

    fn get_default_model(&self) -> &str;
}
```

#### 2.4.2 OpenAI 兼容实现

```rust
pub struct OpenAIProvider {
    client: Arc<reqwest::Client>,
    api_key: String,
    api_base: String,
    default_model: String,
}

impl OpenAIProvider {
    pub fn new(api_key: String, api_base: Option<String>, default_model: String) -> Self;
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    async fn chat(&self, messages: Vec<ChatMessage>, tools: Option<Vec<ToolDefinition>>, model: Option<&str>) -> Result<LLMResponse> {
        // 1. 构建 API 请求
        // 2. 发送 HTTP 请求
        // 3. 解析响应
        // 4. 处理工具调用
    }
}
```

### 2.5 渠道适配设计

#### 2.5.1 渠道接口

```rust
#[async_trait]
pub trait BaseChannel: Send + Sync {
    fn name(&self) -> &str;
    async fn start(&self) -> Result<()>;
    async fn stop(&self) -> Result<()>;
    async fn send(&self, msg: &OutboundMessage) -> Result<()>;
    fn is_allowed(&self, sender_id: &str) -> bool {
        true
    }
    fn bus(&self) -> &MessageBus;
    fn is_running(&self) -> bool {
        false
    }
}
```

#### 2.5.2 渠道工厂

```rust
pub struct ChannelFactory;

impl ChannelFactory {
    pub fn create_telegram(config: &TelegramConfig, bus: Arc<MessageBus>) -> Result<Arc<dyn BaseChannel>>;
    pub fn create_whatsapp(config: &WhatsAppConfig, bus: Arc<MessageBus>) -> Result<Arc<dyn BaseChannel>>;
}
```

#### 2.5.3 渠道管理器

```rust
pub struct ChannelManager {
    channels: Arc<RwLock<HashMap<String, Arc<dyn BaseChannel>>>>,
    bus: Arc<MessageBus>,
    config: ChannelsConfig,
}

impl ChannelManager {
    pub fn new(config: Config, bus: Arc<MessageBus>) -> Self;
    pub async fn initialize(&self) -> Result<()>;
    pub async fn start_all(&self) -> Result<()>;
    pub async fn stop_all(&self) -> Result<()>;
    pub async fn get_channel(&self, name: &str) -> Option<Arc<dyn BaseChannel>>;
    pub async fn get_status(&self) -> HashMap<String, ChannelStatus>;
}
```

## 3. 数据结构设计

### 3.1 配置数据结构

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub agents: AgentsConfig,
    pub channels: ChannelsConfig,
    pub providers: ProvidersConfig,
    pub gateway: GatewayConfig,
    pub tools: ToolsConfig,
}
```

### 3.2 会话数据结构

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub key: String,
    pub messages: Vec<SessionMessage>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    pub role: String,
    pub content: String,
    pub timestamp: Option<DateTime<Utc>>,
}
```

### 3.3 任务数据结构

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub schedule: CronSchedule,
    pub payload: CronPayload,
    pub state: CronJobState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub delete_after_run: bool,
}
```

## 4. 并发设计

### 4.1 异步任务模型

#### 4.1.1 任务分类

| 任务类型 | 处理方式 | 示例 |
|---------|---------|------|
| CPU 密集型 | tokio::task::spawn_blocking | 文件读写 |
| IO 密集型 | async/await | HTTP 请求 |
| 长期运行 | tokio::spawn | 消息循环 |

#### 4.1.2 并发控制

```rust
// 使用 Semaphore 限制并发
pub struct ConcurrencyLimiter {
    semaphore: Arc<Semaphore>,
}

impl ConcurrencyLimiter {
    pub async fn acquire(&self) -> SemaphorePermit<'_> {
        self.semaphore.acquire().await.unwrap()
    }
}
```

### 4.2 状态共享

#### 4.2.1 读写锁

```rust
pub struct SharedState<T> {
    data: Arc<RwLock<T>>,
}

impl<T: Clone> SharedState<T> {
    pub async fn read(&self) -> T {
        self.data.read().await.clone()
    }

    pub async fn write<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        let mut data = self.data.write().await;
        f(&mut data)
    }
}
```

#### 4.2.2 消息传递

```rust
// 使用 mpsc 通道进行任务分发
pub struct TaskDispatcher<T> {
    tx: mpsc::Sender<T>,
    rx: Arc<Mutex<Option<mpsc::Receiver<T>>>>,
}

impl<T> TaskDispatcher<T> {
    pub async fn dispatch(&self, task: T) -> Result<()> {
        self.tx.send(task).await?;
        Ok(())
    }

    pub async fn receive(&self) -> Option<T> {
        let mut rx = self.rx.lock().await;
        rx.as_mut()?.recv().await
    }
}
```

## 5. 错误处理设计

### 5.1 错误类型

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StormclawError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("LLM provider error: {0}")]
    Provider(String),

    #[error("Channel error: {0}")]
    Channel(String),

    #[error("Tool execution error: {0}")]
    Tool(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}
```

### 5.2 错误恢复

#### 5.2.1 重试策略

```rust
pub struct RetryPolicy {
    max_attempts: usize,
    base_delay: Duration,
    max_delay: Duration,
}

impl RetryPolicy {
    pub async fn retry<F, T, E>(&self, mut f: F) -> Result<T, E>
    where
        F: FnMut() -> Pin<Box<dyn Future<Output = Result<T, E>> + Send>>,
        E: std::fmt::Display,
    {
        let mut delay = self.base_delay;

        for attempt in 0..self.max_attempts {
            match f().await {
                Ok(result) => return Ok(result),
                Err(e) if attempt < self.max_attempts - 1 => {
                    tracing::warn!("Attempt {} failed: {}, retrying in {:?}", attempt + 1, e, delay);
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(self.max_delay);
                }
                Err(e) => return Err(e),
            }
        }

        unreachable!()
    }
}
```

## 6. 性能优化设计

### 6.1 内存管理

#### 6.1.1 对象池

```rust
pub struct ObjectPool<T> {
    objects: Arc<Mutex<Vec<T>>>,
    factory: Arc<dyn Fn() -> T + Send + Sync>,
}

impl<T> ObjectPool<T> {
    pub async fn acquire(&self) -> T {
        let mut objects = self.objects.lock().await;
        objects.pop().unwrap_or_else(|| (self.factory)())
    }

    pub async fn release(&self, object: T) {
        let mut objects = self.objects.lock().await;
        objects.push(object);
    }
}
```

#### 6.1.2 缓存设计

```rust
pub struct Cache<K, V> {
    data: Arc<RwLock<HashMap<K, CacheEntry<V>>>>,
    ttl: Duration,
    max_size: usize,
}

struct CacheEntry<V> {
    value: V,
    expires_at: Instant,
}

impl<K: Eq + Hash + Clone, V: Clone> Cache<K, V> {
    pub async fn get(&self, key: &K) -> Option<V> {
        let mut data = self.data.write().await;

        if let Some(entry) = data.get(key) {
            if entry.expires_at > Instant::now() {
                return Some(entry.value.clone());
            } else {
                data.remove(key);
            }
        }

        None
    }

    pub async fn put(&self, key: K, value: V) {
        let mut data = self.data.write().await;

        if data.len() >= self.max_size {
            // LRU 淘汰
        }

        data.insert(key, CacheEntry {
            value,
            expires_at: Instant::now() + self.ttl,
        });
    }
}
```

### 6.2 并发优化

#### 6.2.1 批量处理

```rust
pub struct BatchProcessor<T> {
    buffer: Arc<Mutex<Vec<T>>>,
    batch_size: usize,
    flush_interval: Duration,
}

impl<T> BatchProcessor<T> {
    pub async fn add(&self, item: T) -> Result<()> {
        let mut buffer = self.buffer.lock().await;
        buffer.push(item);

        if buffer.len() >= self.batch_size {
            let items = buffer.drain(..).collect();
            drop(buffer);
            self.flush(items).await?;
        }

        Ok(())
    }

    async fn flush(&self, items: Vec<T>) -> Result<()> {
        // 批量处理逻辑
        Ok(())
    }
}
```

## 7. 安全设计

### 7.1 权限控制

```rust
pub struct PermissionManager {
    allowed_users: HashMap<String, Vec<String>>,
}

impl PermissionManager {
    pub fn is_allowed(&self, channel: &str, user_id: &str) -> bool {
        self.allowed_users
            .get(channel)
            .map(|users| users.contains(&user_id.to_string()))
            .unwrap_or(false)
    }
}
```

### 7.2 输入验证

```rust
pub fn validate_path(path: &str) -> Result<PathBuf> {
    let path = PathBuf::from(path);

    // 防止路径遍历攻击
    if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(anyhow!("Path traversal not allowed"));
    }

    Ok(path)
}
```

### 7.3 敏感信息处理

```rust
pub fn redact_api_key(key: &str) -> String {
    if key.len() <= 8 {
        "***".to_string()
    } else {
        format!("{}***{}", &key[..4], &key[key.len()-4..])
    }
}
```

## 8. 监控和日志设计

### 8.1 结构化日志

```rust
use tracing::{info, warn, error, instrument};

#[instrument(skip(self))]
pub async fn process_message(&self, msg: InboundMessage) -> Result<()> {
    info!(
        channel = %msg.channel,
        sender = %msg.sender_id,
        "Processing message"
    );

    // 处理逻辑

    Ok(())
}
```

### 8.2 指标收集

```rust
pub struct MetricsCollector {
    counters: Arc<RwLock<HashMap<String, AtomicU64>>>,
    gauges: Arc<RwLock<HashMap<String, AtomicI64>>>,
    histograms: Arc<RwLock<HashMap<String, Histogram>>>,
}

impl MetricsCollector {
    pub fn increment(&self, name: &str) {
        let mut counters = self.counters.write().unwrap();
        counters.entry(name.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }
}
```

## 9. 部署架构设计

### 9.1 单机部署

```
┌─────────────────────────────────────┐
│         stormclaw Gateway            │
│  ┌──────────┐    ┌──────────┐     │
│  │   Agent  │◄──►│ Channels │     │
│  └──────────┘    └──────────┘     │
│         ▲              ▲            │
│         │              │            │
└─────────┼──────────────┼────────────┘
          │              │
          ▼              ▼
    ┌──────────┐   ┌──────────┐
    │ OpenAI   │   │ Telegram │
    │   API    │   │   Bot    │
    └──────────┘   └──────────┘
```

### 9.2 容器化部署

```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates
COPY --from=builder /app/target/release/stormclaw /usr/local/bin/
ENTRYPOINT ["stormclaw"]
```

## 10. 总结

本设计文档详细描述了 stormclaw Rust 版本的系统架构、核心组件设计、数据结构、并发模型、错误处理、性能优化、安全措施等方面。通过模块化的架构设计和完善的接口定义，系统具备良好的可扩展性和可维护性。
