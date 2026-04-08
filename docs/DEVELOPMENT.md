# stormclaw 开发指南

## 项目结构

```
stormclaw/
├── Cargo.toml              # 工作空间配置
├── crates/
│   ├── core/               # 核心库
│   │   ├── bus/            # 消息总线
│   │   ├── agent/          # Agent 引擎
│   │   ├── providers/      # LLM 提供商
│   │   ├── session/        # 会话管理
│   │   ├── memory/         # 记忆系统
│   │   └── skills/         # 技能系统
│   ├── channels/           # 渠道适配
│   ├── services/           # 业务服务
│   ├── config/             # 配置管理
│   └── utils/              # 工具函数
├── cli/                    # CLI 应用
├── bridge/                 # WhatsApp 网桥 (复用原版)
├── benches/                # 基准测试
├── tests/                  # 集成测试
└── docs/                   # 文档
```

## 开发环境设置

### 安装依赖

```bash
# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 安装开发工具
cargo install cargo-watch
cargo install cargo-tarpaulin  # 测试覆盖率
cargo install cargo-edit        # 依赖管理
```

### 构建项目

```bash
# 开发构建（更快）
cargo build

# 发布构建（优化）
cargo build --release
```

### 运行测试

```bash
# 所有测试
cargo test --workspace

# 带输出的测试
cargo test --workspace -- --nocapture

# 测试覆盖率
cargo tarpaulin --workspace --out Html
```

### 代码检查

```bash
# 格式化代码
cargo fmt --all

# Clippy 检查
cargo clippy --all-targets -- -D warnings

# 文档检查
cargo doc --workspace --no-deps
```

## 核心概念

### 消息总线 (MessageBus)

消息总线是 stormclaw 的核心消息传递机制，负责连接渠道和 Agent。

```rust
use stormclaw_core::MessageBus;
use std::sync::Arc;

let bus = Arc::new(MessageBus::new(1000));

// 发布入站消息
let inbound = InboundMessage::new("telegram", "user123", "chat456", "Hello!");
bus.publish_inbound(inbound).await?;

// 订阅出站消息
let mut rx = {
    let mut bus_mut = bus.clone();
    bus_mut.subscribe_outbound("telegram".to_string())
};

while let Some(msg) = rx.recv().await {
    // 处理出站消息
}
```

### Agent 循环 (AgentLoop)

Agent 循环处理入站消息，调用 LLM，执行工具，并发送响应。

```rust
use stormclaw_core::{AgentLoop, MessageBus};

let bus = Arc::new(MessageBus::new(1000));
let provider = Arc::new(OpenAIProvider::new(api_key, None, None));
let agent = AgentLoop::new(
    bus,
    provider,
    workspace,
    Some("gpt-4".to_string()),
    10,  // 最大迭代次数
    None,
).await?;

agent.run().await?;
```

### 工具系统 (Tools)

工具系统允许 Agent 执行各种操作。

#### 创建自定义工具

```rust
use async_trait::async_trait;
use stormclaw_core::agent::tools::Tool;
use std::sync::Arc;

struct MyTool {
    name: &'static str,
}

#[async_trait]
impl Tool for MyTool {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        "我的自定义工具"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "string",
                    "description": "输入参数"
                }
            },
            "required": ["input"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<String> {
        let input = args["input"].as_str().ok_or_else(|| anyhow::anyhow!("缺少 input"))?;
        // 实现你的逻辑
        Ok(format!("处理结果: {}", input))
    }

    fn to_schema(&self) -> serde_json::Value {
        // 返回 OpenAI 函数调用格式
        serde_json::json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description(),
                "parameters": self.parameters()
            }
        })
    }
}
```

#### 注册工具

```rust
use stormclaw_core::agent::tools::ToolRegistry;

let registry = ToolRegistry::new();
let tool = Arc::new(MyTool { name: "my_tool" });
registry.register(tool).await;

// 执行工具
let args = serde_json::json!({"input": "test"});
let result = registry.execute("my_tool", args).await?;
```

### 渠道 (Channels)

渠道适配器负责与外部消息平台通信。

#### 渠道接口

```rust
use async_trait::async_trait;
use stormclaw_core::{MessageBus, InboundMessage, OutboundMessage};

#[async_trait]
pub trait BaseChannel: Send + Sync {
    fn name(&self) -> &str;
    async fn start(&self) -> anyhow::Result<()>;
    async fn stop(&self) -> anyhow::Result<()>;
    async fn send(&self, msg: &OutboundMessage) -> anyhow::Result<()>;
    fn is_allowed(&self, sender_id: &str) -> bool;
    fn bus(&self) -> &MessageBus;
    fn is_running(&self) -> bool;
}
```

#### 创建自定义渠道

```rust
pub struct MyChannel {
    config: MyConfig,
    bus: Arc<MessageBus>,
    running: Arc<AtomicBool>,
}

impl MyChannel {
    pub fn new(config: MyConfig, bus: Arc<MessageBus>) -> anyhow::Result<Self> {
        Ok(Self {
            config,
            bus,
            running: Arc::new(AtomicBool::new(false)),
        })
    }
}

#[async_trait]
impl BaseChannel for MyChannel {
    fn name(&self) -> &str {
        "my_channel"
    }

    async fn start(&self) -> anyhow::Result<()> {
        // 启动连接
        self.running.store(true, Ordering::Relaxed);
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        self.running.store(false, Ordering::Relaxed);
        Ok(())
    }

    async fn send(&self, msg: &OutboundMessage) -> anyhow::Result<()> {
        // 发送消息到外部平台
        Ok(())
    }

    fn is_allowed(&self, sender_id: &str) -> bool {
        // 权限检查
        true
    }

    fn bus(&self) -> &MessageBus {
        &self.bus
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}
```

## 添加新的 LLM 提供商

```rust
use async_trait::async_trait;
use stormclaw_core::providers::{LLMProvider, LLMResponse, ChatMessage, ToolDefinition};

pub struct MyProvider {
    api_key: String,
    base_url: String,
}

#[async_trait]
impl LLMProvider for MyProvider {
    async fn chat(
        &self,
        messages: Vec<ChatMessage>,
        tools: Option<Vec<ToolDefinition>>,
        model: Option<&str>,
    ) -> anyhow::Result<LLMResponse> {
        // 调用 LLM API
        Ok(LLMResponse {
            content: Some("响应内容".to_string()),
            tool_calls: vec![],
            finish_reason: "stop".to_string(),
            usage: None,
        })
    }

    fn get_default_model(&self) -> &str {
        "my-model"
    }
}
```

## 测试指南

### 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_my_function() {
        let result = my_function().await.unwrap();
        assert_eq!(result, "expected");
    }
}
```

### 集成测试

在 `tests/` 目录下创建测试文件：

```rust
// tests/integration_test.rs
use stormclaw_core::{MessageBus, InboundMessage};

#[tokio::test]
async fn test_message_flow() {
    let bus = Arc::new(MessageBus::new(100));
    let msg = InboundMessage::new("test", "user", "chat", "Hello");

    bus.publish_inbound(msg).await.unwrap();

    // 验证结果
}
```

### 基准测试

在 `benches/` 目录下创建基准测试：

```rust
// benches/my_benchmark.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_my_function(c: &mut Criterion) {
    c.bench_function("my_function", |b| {
        b.iter(|| {
            black_box(my_function())
        });
    });
}

criterion_group!(benches, benchmark_my_function);
criterion_main!(benches);
```

## 贡献流程

1. Fork 项目
2. 创建功能分支：`git checkout -b feature/amazing-feature`
3. 提交更改：`git commit -m 'Add amazing feature'`
4. 推送分支：`git push origin feature/amazing-feature`
5. 创建 Pull Request

### 代码规范

- 使用 `cargo fmt` 格式化代码
- 通过 `cargo clippy` 检查
- 添加测试覆盖新功能
- 更新相关文档

### 提交信息规范

```
feat: 添加 Discord Gateway 支持
fix: 修复内存泄漏问题
docs: 更新用户指南
test: 添加消息总线测试
refactor: 优化工具注册表实现
```

## 性能优化建议

1. **使用 Arc 减少克隆**
   ```rust
   // 好
   let bus = Arc::new(MessageBus::new(1000));

   // 避免
   let bus = MessageBus::new(1000);  // 每次传递都会克隆
   ```

2. **异步优先**
   - 使用 tokio 的异步函数
   - 避免阻塞的同步操作

3. **错误处理**
   - 使用 `anyhow::Result` 简化错误处理
   - 提供有意义的错误信息

4. **内存管理**
   - 及时释放大对象
   - 考虑使用流式处理大文件

## 调试技巧

### 启用日志

```bash
RUST_LOG=debug stormclaw agent -m "test"
```

### 使用调试器

```bash
rust-lldb -- target/debug/stormclaw agent -m "test"
```

### 性能分析

```bash
# CPU 分析
cargo flamegraph --bin stormclaw

# 内存分析
valgrind --tool=massif ./target/release/stormclaw
```

## 相关资源

- [Rust 官方文档](https://doc.rust-lang.org/)
- [Tokio 异步运行时](https://tokio.rs/)
- [Async Trait](https://docs.rs/async-trait/)
- [Anyhow 错误处理](https://docs.rs/anyhow/)
