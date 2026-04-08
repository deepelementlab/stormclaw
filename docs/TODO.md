# stormclaw 待完成事项 (TODO)

## 创建时间
2026-03-15

## 高优先级任务

### 1. 子代理集成

**文件**: `crates/core/src/agent/subagent.rs`

**需要实现**:
```rust
// 子代理管理器
pub struct SubagentManager<P: LLMProvider> {
    agents: Arc<RwLock<HashMap<String, AgentLoop<P>>>>,
    bus: Arc<MessageBus>,
}

// 子代理调用
pub async fn spawn_subagent(
    &self,
    name: String,
    task: String,
    context: Option<Context>,
) -> anyhow::Result<String>
```

**验证方式**:
- 在对话中调用 "使用子代理完成XXX任务"
- 验证子代理能正确执行并返回结果

---

### 2. 集成测试 / 回归测试

**已实现位置**: `crates/integration-tests/`（Cargo 包名 `stormclaw-integration-tests`）

虚拟 workspace 根目录下的 `tests/` 不会自动参与 `cargo test`，因此共享辅助与回归用例放在上述 crate 的 `src/common/` 与 `tests/regression_*.rs`。

**当前覆盖**（可持续扩展）:

- `regression_config.rs`：`Config` 默认序列化/反序列化
- `regression_cron.rs`：`CronService` 落盘、`list_jobs`、`enable_job`
- `regression_message_bus.rs`：入站发布与消费、出站 broadcast
- `regression_cli_smoke.rs`：`stormclaw --help` 等烟测（依赖已构建的 `target/*/stormclaw` 二进制）

**后续可增强**（仍待实现）:

- 完整 Agent 循环（`MockLLMProvider` + 临时配置目录）
- 真实渠道消息流（mock 或录制 fixture）
- 定时任务在短时间隔下的真实触发与回调断言

**验证方式**:
```bash
cd stormclaw
cargo test -p stormclaw-integration-tests
# 或
cargo test --workspace
```

---

### 3. 性能基准测试

**文件**: `benches/message_bus_bench.rs`

**需要实现的测试**:
```rust
#[bench]
fn bench_message_publish(b: &mut Bencher) {
    // 测试消息发布性能
}

#[bench]
fn bench_agent_loop(b: &mut Bencher) {
    // 测试 Agent 循环性能
}

#[bench]
fn bench_tool_execution(b: &mut Bencher) {
    // 测试工具执行性能
}
```

**验证方式**:
```bash
cargo bench --workspace
```

---

## 中优先级任务

### 4. Discord 渠道增强 (使用 serenity)

**文件**: `crates/channels/src/discord.rs`

**需要实现**:
- 集成 `serenity` 库
- 实现 Gateway WebSocket 连接
- 支持 Slash Commands
- 支持更多消息类型（嵌入、附件等）

**依赖更新**:
```toml
serenity = { version = "0.12", default-features = false, features = ["client", "gateway", "rustls_backend"] }
```

---

### 5. Email 渠道完善 (使用 async-imap)

**文件**: `crates/channels/src/email.rs`

**需要实现**:
- 集成 `async-imap` 库
- 实现 IDLE 模式监听
- 支持附件处理
- 支持 richer 邮件解析

**依赖更新**:
```toml
async-imap = { version = "0.10", default-features = false, features = ["runtime-tokio"] }
```

---

### 6. 单元测试增强

**目标覆盖率**: >80%

**需要测试的模块**:
- `crates/core/src/agent/loop.rs`
- `crates/channels/src/manager.rs`
- `crates/services/src/gateway.rs`

**验证方式**:
```bash
cargo tarpaulin --workspace --out Html
```

---

## 低优先级任务

### 7. 示例代码

**需要创建的文件**:
```
examples/
├── simple_agent.rs      # 简单 Agent 示例
├── custom_channel.rs    # 自定义渠道示例
├── custom_tool.rs       # 自定义工具示例
└── webhook_server.rs    # Webhook 服务器示例
```

---

### 8. 用户文档

**需要创建的文档**:
```
docs/
├── USER_GUIDE.md         # 用户指南
├── CHANNEL_SETUP.md      # 渠道配置指南
├── API_REFERENCE.md      # API 参考
├── DEPLOYMENT.md         # 部署指南
└── DEVELOPMENT.md        # 开发者指南
```

---

### 9. Docker 支持

**需要创建的文件**:
```
docker/
├── Dockerfile
└── docker-compose.yml
```

---

## 测试检查清单

### 编译检查

```bash
# 检查代码
cargo check --workspace

# 格式检查
cargo fmt --workspace --check

# Lint 检查
cargo clippy --workspace
```

### 功能检查

```bash
# 初始化
./target/release/stormclaw onboard

# 配置验证
./target/release/stormclaw config validate

# 状态查看
./target/release/stormclaw status

# 渠道状态
./target/release/stormclaw channels status

# 定时任务
./target/release/stormclaw cron list

# 会话列表
./target/release/stormclaw session list

# 网关启动
./target/release/stormclaw gateway
```

### API 检查

```bash
# 健康检查
curl http://localhost:18789/health

# 服务状态
curl http://localhost:18789/status

# 会话列表
curl http://localhost:18789/sessions

# 渠道状态
curl http://localhost:18789/channels

# 定时任务
curl http://localhost:18789/cron/jobs

# 指标
curl http://localhost:18789/metrics
```

---

## 问题跟踪

### 已知问题

1. **Windows 路径编译问题**
   - 问题: 中文路径导致链接器失败
   - 状态: 环境问题，代码无问题
   - 解决: 使用英文路径或 WSL

2. **Discord Gateway 功能**
   - 问题: 当前仅实现 HTTP API
   - 解决: 需要集成 serenity 库

3. **Email IMAP 功能**
   - 问题: 当前仅实现 SMTP
   - 解决: 需要集成 async-imap 库

---

## 时间估算

| 任务 | 预估时间 | 优先级 |
|------|----------|--------|
| 子代理集成 | 4-6 小时 | 高 |
| 集成测试 | 3-4 小时 | 高 |
| 性能测试 | 2-3 小时 | 高 |
| Discord 增强 | 3-4 小时 | 中 |
| Email 完善 | 2-3 小时 | 中 |
| 单元测试增强 | 2-3 小时 | 中 |
| 示例代码 | 2-3 小时 | 低 |
| 用户文档 | 3-4 小时 | 低 |
| Docker 支持 | 1-2 小时 | 低 |

**总计**: 约 22-32 小时

---

## 下次开发起点

建议从以下任务开始：

1. **第一选择**: 子代理集成（核心功能）
2. **第二选择**: 集成测试（质量保证）
3. **第三选择**: 性能测试（优化基础）

选择原因：
- 子代理是原版 Python 功能之一，完成后功能对齐
- 集成测试确保现有功能稳定
- 性能测试为优化提供数据支持
