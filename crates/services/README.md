# stormclaw 服务层文档

## 概述

服务层提供了 stormclaw 的所有业务服务，包括定时任务、心跳、网关等核心服务，以及辅助服务如日志、配置热重载、指标收集等。

## 核心服务

### 1. 定时任务服务 (`cron.rs`)

**功能**:
- 对齐 Python：单定时器 + `nextRunAtMs` 计算
- 支持三种调度类型：
  - 间隔调度 (`every` / `everyMs`)
  - Cron 表达式 (`cron` / `expr`)
  - 一次性任务 (`at` / `atMs`)
- 任务持久化到 JSON（camelCase 字段）
- 任务启用/禁用
- 手动运行任务

**API**:
```rust
use stormclaw_services::{CronService, every_job};

// 创建服务
let cron = CronService::new(store_path)?;

// 添加间隔任务（每5分钟）
let job = every_job(
    "daily_summary".to_string(),
    "Generate daily summary".to_string(),
    300,
);
cron.add_job(job).await?;

// Cron/At 任务：使用 CronJob/CronSchedule 结构体手动构造（与 Python jobs.json 一致）

// 列出任务
let jobs = cron.list_jobs(true).await;

// 手动运行任务
cron.run_job("job_id", true).await?;
```

**配置文件格式（示例）**:
```json
{
  "version": 1,
  "jobs": [
    {
      "id": "daily_001",
      "name": "daily_summary",
      "enabled": true,
      "schedule": {
        "kind": "cron",
        "expr": "0 9 * * *",
        "tz": "UTC"
      },
      "payload": {
        "kind": "agent_turn",
        "message": "Generate daily summary",
        "deliver": false
      },
      "state": {
        "nextRunAtMs": 0,
        "lastRunAtMs": null,
        "lastStatus": null,
        "lastError": null
      }
    }
  ]
}
```

---

### 2. 心跳服务 (`heartbeat.rs`)

**功能**:
- 定期检查 HEARTBEAT.md 文件
- 执行文件中定义的任务
- 支持定时触发和手动触发
- 任务完成状态跟踪

**API**:
```rust
use stormclaw_services::{HeartbeatService, HeartbeatConfig};

// 创建服务
let config = HeartbeatConfig {
    interval_seconds: 30 * 60, // 30 分钟
    enabled: true,
    heartbeat_file: "HEARTBEAT.md".to_string(),
};

let heartbeat = HeartbeatService::new(workspace, config);

// 设置回调
let callback = Arc::new(|prompt: &str| -> anyhow::Result<String> {
    // 调用 Agent 处理心跳
    Ok("Heartbeat OK".to_string())
});
heartbeat.set_callback(callback).await;

// 启动服务
heartbeat.start().await?;

// 手动触发
let response = heartbeat.trigger_now().await?;
```

**HEARTBEAT.md 文件格式**:
```markdown
# Heartbeat Tasks

## Daily Tasks

- [ ] Check daily schedule
- [ ] Send morning report

## One-time Tasks

- [ ] Update documentation
- [ ] Clean up old files

Instructions:
- Items marked with [x] are considered completed
- Uncheck items after completion to re-run
```

---

### 3. 网关服务 (`gateway.rs`)

**功能**:
- 统一的网关入口
- 协调所有子服务
- HTTP API 端点
- 生命周期管理
- 优雅关闭

**API**:
```rust
use stormclaw_services::{GatewayService, GatewayBuilder, GatewayConfig};

// 使用 Builder
let gateway = GatewayBuilder::new()
    .with_config(config)
    .with_gateway_config(gateway_config)
    .build()
    .await?;

// 或者直接创建
let gateway = GatewayService::new(config, gateway_config).await?;

// 启动网关
gateway.start().await?;
```

**HTTP 端点**:
- `GET /health` - 健康检查
- `GET /status` - 网关状态
- `POST /heartbeat/trigger` - 手动触发心跳

---

## 辅助服务

### 4. 服务监管器 (`supervisor.rs`)

**功能**:
- 管理多个子服务
- 自动重启失败的服务
- 重启策略配置
- 健康检查
- 服务依赖管理

**API**:
```rust
use stormclaw_services::{ServiceSupervisor, RestartPolicy, ServiceTopology};

let supervisor = ServiceSupervisor::new(RestartPolicy::Limit {
    max_restarts: 3,
});

// 注册服务
supervisor.register_service("agent".to_string(), start_fn).await;

// 设置依赖
let mut topology = ServiceTopology::new();
topology.add_service("cron".to_string());
topology.add_dependency("agent".to_string(), "cron".to_string());

// 启动监管器
supervisor.start().await?;
```

---

### 5. 生命周期管理 (`lifecycle.rs`)

**功能**:
- 服务生命周期事件发布/订阅
- 生命周期阶段跟踪
- 启动器模式
- 优雅关闭

**API**:
```rust
use stormclaw_services::{ServiceLifecycle, LifecyclePhase, ServiceLauncher};

// 获取生命周期
let lifecycle = ServiceLifecycle::new("agent".to_string());
lifecycle.publish(LifecycleEvent::Starting).await;

// 等待启动完成
lifecycle.wait_for_phase(LifecyclePhase::Running).await?;

// 使用启动器
let launcher = ServiceLauncher::new("agent".to_string());
launcher.launch(|lifecycle| async move {
    lifecycle.publish(LifecycleEvent::Started).await?;
    // 服务逻辑
    Ok(())
}).await?;
```

---

### 6. 日志服务 (`logging.rs`)

**功能**:
- 集中式日志管理
- 多输出目标（控制台、文件）
- 日志轮换
- 服务日志器

**API**:
```rust
use stormclaw_services::{LoggingService, LoggingConfig, LogLevel};

let config = LoggingConfig {
    min_level: LogLevel::Info,
    console_enabled: true,
    file_enabled: true,
    file_path: Some("/var/log/stormclaw/app.log".to_string()),
    max_file_size: 100 * 1024 * 1024, // 100MB
};

let logging = LoggingService::new(config);
logging.start().await?;

// 创建服务日志器
let logger = logging.logger("agent".to_string());
logger.info("Agent started".to_string());
logger.error("Error occurred".to_string());
```

---

### 7. 配置热重载 (`config_reload.rs`)

**功能**:
- 监控配置文件变化
- 自动重载配置
- 验证配置有效性
- 重载回调机制

**API**:
```rust
use stormclaw_services::{HotReloadService, HotReloadConfig, ConfigReloader};

let config = HotReloadConfig {
    enabled: true,
    check_interval_ms: 1000,
    ignore_patterns: vec!["*.tmp".to_string()],
};

let reload = HotReloadService::new(config)?;

// 监视主配置
reload.watch(
    PathBuf::from("~/.stormclaw/config.json"),
    ConfigReloader::main_config_callback(),
).await;

// 启动服务
reload.start().await?;
```

---

### 8. 指标收集 (`metrics.rs`)

**功能**:
- Prometheus 格式指标暴露
- Counter、Gauge、Histogram 指标
- 预定义服务指标
- JSON 格式导出

**API**:
```rust
use stormclaw_services::{MetricsCollector, ServiceMetrics};

let collector = MetricsCollector::new();

// 创建服务指标
let metrics = ServiceMetrics::new(&collector).await;

// 记录指标
metrics.record_message().await;
metrics.record_sent().await;
metrics.record_error().await;

// 收集 Prometheus 格式
let prometheus = collector.collect_prometheus().await;

// 收集 JSON 格式
let json_metrics = collector.collect_json().await;
```

**预定义指标**:
- `STORMCLAW_messages_processed_total` - 处理的消息总数
- `STORMCLAW_messages_sent_total` - 发送的消息总数
- `STORMCLAW_messages_received_total` - 接收的消息总数
- `STORMCLAW_errors_total` - 错误总数
- `STORMCLAW_active_connections` - 活跃连接数
- `STORMCLAW_request_duration_ms` - 请求耗时分布

---

## 服务依赖关系

```
┌─────────────────────────────────────────────────────────────┐
│                    GatewayService                            │
│                  (服务协调与编排)                              │
└────────────────────┬────────────────────────────────────────┘
                     │
        ┌────────────┼────────────┬────────────┐
        │            │            │            │
┌───────▼────┐ ┌─────▼──────┐ ┌────▼──────┐ ┌──────▼──────┐
│   Cron     │ │ Heartbeat  │ │  Agent    │ │ Channels   │
│  Service  │ │  Service  │ │   Loop    │ │  Manager   │
└───────────┘ └────────────┘ └───────────┘ └────────────┘
        ┌────────────┼────────────┐
        │            │            │
┌───────▼────┐ ┌─────▼──────┐ ┌────▼──────┐
│  Logging   │ │ Config    │ │ Metrics  │
│  Service  │ │ Reload    │ │ Service  │
└───────────┘ └────────────┘ └───────────┘
        ┌────────────┘
    ┌───▼────────┐
    │ Supervisor │
    └────────────┘
```

---

## 使用示例

### 完整启动流程

```rust
use stormclaw_services::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 加载配置
    let config = stormclaw_config::load_config()?;

    // 创建网关
    let gateway_config = GatewayConfig {
        host: "0.0.0.0".to_string(),
        port: 18789,
        http_enabled: true,
        metrics_enabled: true,
    };

    let gateway = GatewayService::new(config, gateway_config).await?;

    // 启动网关（会自动启动所有子服务）
    gateway.start().await?;

    Ok(())
}
```

### 仅启动定时任务服务

```rust
let cron_store_path = std::path::PathBuf::from("cron_jobs.json");

let cron = CronService::new(cron_store_path)?;

// 设置任务执行回调
let agent = Arc::new(MyAgent::new());
let callback = Arc::new(move |job| {
    agent.execute(&job.payload.message).await?;
    Ok(None)
});
cron.set_callback(callback).await;

// 添加任务
cron.add_job(every_job(
    "test".to_string(),
    "Test message".to_string(),
    60,
).await?;

// 启动服务
cron.start().await?;
```

### 仅启动心跳服务

```rust
let config = HeartbeatConfig {
    interval_seconds: 30 * 60,
    enabled: true,
    heartbeat_file: "HEARTBEAT.md".to_string(),
};

let heartbeat = HeartbeatService::new(workspace, config);

// 设置回调
let agent = Arc::new(MyAgent::new());
let callback = Arc::new(move |prompt| {
    agent.process(prompt).await
});
heartbeat.set_callback(callback).await;

// 启动服务
heartbeat.start().await?;
```

---

## 配置示例

### 完整的配置文件

```json
{
  "gateway": {
    "host": "0.0.0.0",
    "port": 18789,
    "httpEnabled": true,
    "metricsEnabled": true
  },
  "cron": {
    "enabled": true,
    "storePath": "~/.stormclaw/cron/jobs.json"
  },
  "heartbeat": {
    "enabled": true,
    "intervalSeconds": 1800,
    "heartbeatFile": "HEARTBEAT.md"
  },
  "logging": {
    "minLevel": "INFO",
    "consoleEnabled": true,
    "fileEnabled": true,
    "filePath": "~/.stormclaw/logs/app.log",
    "maxFileSize": 104857600
  },
  "hotReload": {
    "enabled": true,
    "checkIntervalMs": 1000
  },
  "supervisor": {
    "restartPolicy": "limit",
    "maxRestarts": 3
  }
}
```
