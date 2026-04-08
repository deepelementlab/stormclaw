# stormclaw API 参考

## 核心库 (stormclaw-core)

### MessageBus

消息总线，用于在组件之间传递消息。

```rust
use stormclaw_core::MessageBus;
use std::sync::Arc;

// 创建消息总线
let bus = Arc::new(MessageBus::new(1000));

// 发布入站消息
bus.publish_inbound(inbound_message).await?;

// 发布出站消息
bus.publish_outbound(outbound_message).await?;

// 订阅出站消息
let mut rx = {
    let mut bus_mut = bus.clone();
    bus_mut.subscribe_outbound("telegram".to_string())
};
```

### AgentLoop

Agent 循环引擎，处理消息并调用 LLM。

```rust
use stormclaw_core::{AgentLoop, MessageBus};
use std::sync::Arc;

let bus = Arc::new(MessageBus::new(1000));
let provider = Arc::new(OpenAIProvider::new(api_key, None, None));

let agent = AgentLoop::new(
    bus,
    provider,
    workspace,
    Some("gpt-4".to_string()),
    10,
    None,
).await?;

// 启动 Agent 循环
agent.run().await?;

// 停止 Agent 循环
agent.stop().await;
```

### SessionManager

会话管理器，存储和管理对话历史。

```rust
use stormclaw_core::SessionManager;

let manager = SessionManager::new(workspace).await?;

// 获取或创建会话
let session = manager.get_or_create("telegram:chat123").await?;

// 添加消息
session.add_message("user", "Hello");
session.add_message("assistant", "Hi there!");

// 保存会话
manager.save(&session).await?;

// 获取历史记录
let history = session.get_history(50);
```

### MemoryStore

记忆存储，管理长期和短期记忆。

```rust
use stormclaw_core::MemoryStore;

let store = MemoryStore::new(workspace);

// 读取今日记忆
let today = store.read_today()?;

// 追加今日记忆
store.append_today("今天完成了重要任务")?;

// 读取长期记忆
let long_term = store.read_long_term()?;

// 写入长期记忆
store.write_long_term("重要信息：...")?;

// 获取最近 N 天的记忆
let recent = store.get_recent_memories(7)?;
```

### ToolRegistry

工具注册表，管理可用工具。

```rust
use stormclaw_core::agent::tools::ToolRegistry;

let registry = ToolRegistry::new();

// 注册工具
registry.register(Arc::new(MyTool)).await;

// 检查工具是否存在
if registry.has("my_tool").await {
    // ...
}

// 获取工具
if let Some(tool) = registry.get("my_tool").await {
    // 使用工具
}

// 执行工具
let result = registry.execute("my_tool", args).await?;

// 获取所有工具定义
let definitions = registry.get_definitions().await;
```

## 渠道库 (stormclaw-channels)

### TelegramChannel

Telegram 机器人渠道。

```rust
use stormclaw_channels::TelegramChannel;

let channel = TelegramChannel::new(token, allow_from, bus)?;

channel.start().await?;

// 发送消息
channel.send(&outbound_message).await?;

channel.stop().await?;
```

### DiscordChannel

Discord 机器人渠道。

```rust
use stormclaw_channels::DiscordChannel;

let config = DiscordConfig {
    enabled: true,
    token: "bot_token".to_string(),
    allow_from: vec!["guild_id".to_string()],
    command_prefix: "!".to_string(),
};

let channel = DiscordChannel::new(config, bus)?;

channel.start().await?;
```

### EmailChannel

电子邮件渠道。

```rust
use stormclaw_channels::EmailChannel;

let config = EmailConfig {
    enabled: true,
    imap: ImapConfig { ... },
    smtp: SmtpConfig { ... },
    check_interval: 60,
    allow_from: vec![],
    folder: "INBOX".to_string(),
};

let channel = EmailChannel::new(config, bus);

channel.start().await?;
```

## 服务库 (stormclaw-services)

### GatewayService

网关服务，提供 HTTP API 和服务协调。

```rust
use stormclaw_services::GatewayService;

let gateway = GatewayService::new(config, bus).await?;

// 启动网关
gateway.start().await?;

// 停止网关
gateway.stop().await?;
```

### CronService

定时任务服务。

```rust
use stormclaw_services::cron::{CronService, CronJob, CronSchedule};

let service = CronService::new(store_path)?;

// 添加任务
let job = CronJob {
    id: "daily_job".to_string(),
    name: "Daily Task".to_string(),
    enabled: true,
    schedule: CronSchedule::Every { every_ms: 86400000 },
    payload: CronPayload {
        kind: "agent_turn".to_string(),
        message: "检查每日任务".to_string(),
        deliver: false,
        channel: None,
        to: None,
    },
    state: CronJobState::default(),
    created_at: Utc::now(),
    updated_at: Utc::now(),
    delete_after_run: false,
};

service.add_job(job).await?;

// 启动服务
service.start().await?;
```

### HeartbeatService

心跳服务，定期执行 HEARTBEAT.md 中的任务。

```rust
use stormclaw_services::HeartbeatService;

let config = HeartbeatConfig {
    interval_seconds: 1800,  // 30 分钟
    enabled: true,
    heartbeat_file: "HEARTBEAT.md".to_string(),
};

let service = HeartbeatService::new(workspace, config);

// 设置回调
service.set_callback(callback).await;

// 启动服务
service.start().await?;

// 手动触发
service.trigger_now().await?;
```

## 工具库 (stormclaw-utils)

### 文件系统工具

```rust
use stormclaw_utils::{ensure_dir, safe_filename};

// 确保目录存在
ensure_dir(&path)?;

// 创建安全文件名
let safe = safe_filename("path:to/file");  // "path_to_file"
```

### 时间工具

```rust
use stormclaw_utils::{today_date, now_ms};

// 获取当前日期
let date = today_date();  // "2024-01-01"

// 获取当前时间戳（毫秒）
let ts = now_ms();
```

### 性能监控

```rust
use stormclaw_utils::metrics::{Metrics, Timer};

let metrics = Metrics::new();

// 记录性能
let _timer = Timer::message().with_metrics(metrics.clone());
// ... 执行操作 ...
// timer 自动记录

// 获取摘要
let summary = metrics.summary();
println!("平均处理时间: {:.2}ms", summary.average_processing_time_ms.unwrap());
```

## 配置 (stormclaw-config)

### 配置结构

```rust
use stormclaw_config::{Config, AgentConfig, ProviderConfig};

// 加载配置
let config = Config::load(path).await?;

// 访问配置
let model = config.agents.defaults.model;
let api_key = config.providers.openrouter.api_key;
```

## 错误处理

所有函数返回 `anyhow::Result<T>`，使用 `?` 传播错误：

```rust
use anyhow::{Result, Context};

async fn my_function() -> Result<String> {
    let value = some_operation()
        .await
        .context("操作失败")?;

    Ok(value)
}
```

## 异步模式

使用 `async/await` 处理异步操作：

```rust
async fn process_message(bus: Arc<MessageBus>) -> anyhow::Result<()> {
    bus.publish_inbound(message).await?;
    Ok(())
}

// 运行异步代码
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    process_message(bus).await?;
    Ok(())
}
```

## 更多信息

- [用户指南](./USER_GUIDE.md)
- [开发指南](./DEVELOPMENT.md)
- [贡献指南](./CONTRIBUTING.md)
