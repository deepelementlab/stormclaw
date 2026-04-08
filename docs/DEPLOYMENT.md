# stormclaw 部署和使用指南

## 系统要求

### 最低要求
- Rust 1.70 或更高版本
- 操作系统：
  - Linux (推荐 Ubuntu 20.04+, Debian 11+, CentOS 8+)
  - macOS 11+
  - Windows 10/11 (需要正确配置 MSVC)

### 推荐配置
- CPU: 2 核心及以上
- 内存: 2GB 及以上
- 磁盘: 100MB 及以上

## 编译问题诊断

### 当前环境检查结果

**检查时间**: 2026-03-16

**问题**: Windows 路径包含中文字符导致链接器失败

```
error: link: extra operand 'D:\\寮\200婧\220椤圭\233甛\stormclaw-main\...'
```

**解决方案**: 移动项目到纯英文路径

```bash
# 错误的路径（包含中文）
D:\开源项目\stormclaw-main\stormclaw-main

# 正确的路径（纯英文）
D:\projects\stormclaw-main
C:\dev\stormclaw
```

### 修复步骤

1. **移动项目到英文路径**
```bash
# 将项目移动到不含中文的路径
xcopy "D:\开源项目\stormclaw-main\stormclaw-main" "D:\projects\stormclaw" /E /I /H
cd D:\projects\stormclaw
```

2. **清理构建缓存**
```bash
cargo clean
cargo update
```

3. **重新编译**
```bash
cargo build --release
```

## 完整编译流程

### 1. 环境准备

#### Linux/macOS
```bash
# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# 安装构建依赖
sudo apt-get install build-essential pkg-config libssl-dev  # Ubuntu/Debian
# 或
brew install openssl pkg-config                           # macOS
```

#### Windows
```powershell
# 安装 Visual Studio Build Tools
# 下载并安装 Visual Studio Installer
# 选择 "C++ build tools" 工作负载

# 安装 Rust
# https://rustup.rs/
```

### 2. 克隆项目

```bash
git clone https://github.com/deepelementlab/stormclaw.git
cd stormclaw
```

### 3. 编译项目

```bash
# 开发版本（快速编译）
cargo build

# 发布版本（优化编译）
cargo build --release
```

### 4. 验证编译

```bash
# 运行所有测试
cargo test --workspace

# 运行特定模块测试
cargo test --package stormclaw-core
cargo test --package stormclaw-channels
cargo test --package stormclaw-services

# 检查编译
cargo check --workspace
```

## 部署方式

### 方式一：本地部署

#### 1. 安装
```bash
cargo install --path .
```

安装后二进制文件位于 `~/.cargo/bin/stormclaw`

#### 2. 配置
```bash
# 初始化配置
stormclaw onboard

# 编辑配置
nano ~/.stormclaw/config.json
```

配置示例：
```json
{
  "agents": {
    "defaults": {
      "model": "anthropic/claude-opus-4-5"
    }
  },
  "providers": {
    "openrouter": {
      "apiKey": "sk-or-v1-xxx"
    }
  },
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "YOUR_BOT_TOKEN",
      "allowFrom": []
    }
  },
  "tools": {
    "web": {
      "search": {
        "apiKey": "BSA-xxx"
      }
    }
  }
}
```

#### 3. 运行
```bash
# 测试 Agent
stormclaw agent -m "你好！"

# 启动网关
stormclaw gateway
```

### 方式二：Docker 部署

#### Dockerfile

```dockerfile
# Dockerfile
FROM rust:1.75 as builder

WORKDIR /usr/src/stormclaw
COPY . .

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/stormclaw/target/release/stormclaw /usr/local/bin/

WORKDIR /root/.stormclaw
VOLUME ["/root/.stormclaw"]

CMD ["stormclaw", "gateway"]
```

#### docker-compose.yml

```yaml
version: '3.8'

services:
  stormclaw:
    build: .
    container_name: stormclaw
    restart: unless-stopped
    environment:
      - RUST_LOG=info
    volumes:
      - ./config:/root/.stormclaw
      - ./workspace:/root/.stormclaw/workspace
    ports:
      - "3000:3000"  # 网关 API 端口
```

#### 部署命令

```bash
# 构建镜像
docker build -t stormclaw:latest .

# 运行容器
docker run -d \
  -v $(pwd)/config:/root/.stormclaw \
  -v $(pwd)/workspace:/root/.stormclaw/workspace \
  --name stormclaw \
  stormclaw:latest

# 或使用 docker-compose
docker-compose up -d
```

### 方式三：Systemd 服务（Linux）

#### 服务文件

```ini
# /etc/systemd/system/stormclaw.service
[Unit]
Description=stormclaw AI Assistant
After=network.target

[Service]
Type=simple
User=stormclaw
WorkingDirectory=/home/stormclaw/.stormclaw
ExecStart=/usr/local/bin/stormclaw gateway
Restart=always
RestartSec=10

Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
```

#### 启用服务

```bash
# 创建用户
sudo useradd -r -s /bin/bash stormclaw

# 安装二进制
sudo cp target/release/stormclaw /usr/local/bin/

# 创建服务文件
sudo nano /etc/systemd/system/stormclaw.service
# 粘贴上面的服务配置

# 启用并启动服务
sudo systemctl daemon-reload
sudo systemctl enable stormclaw
sudo systemctl start stormclaw

# 查看状态
sudo systemctl status stormclaw

# 查看日志
sudo journalctl -u stormclaw -f
```

## 配置详解

### 基础配置

```json
{
  "agents": {
    "defaults": {
      "model": "anthropic/claude-opus-4-5",
      "maxIterations": 10
    }
  },
  "providers": {
    "openrouter": {
      "apiKey": "sk-or-v1-xxx"
    }
  },
  "workspace": "/path/to/workspace"
}
```

### 渠道配置

#### Telegram 渠道

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "123456:ABC-DEF1234567890",
      "allowFrom": ["123456789"]
    }
  }
}
```

#### Discord 渠道

```json
{
  "channels": {
    "discord": {
      "enabled": true,
      "token": "MTAwMDAwMDAwMA.XXXXX.XXXXXXXXXXXXXXXXX",
      "allowFrom": ["GUILD_ID"],
      "commandPrefix": "!"
    }
  }
}
```

#### Slack 渠道

```json
{
  "channels": {
    "slack": {
      "enabled": true,
      "botToken": "xoxb-YOUR-BOT-TOKEN",
      "appToken": "xapp-YOUR-APP-TOKEN",
      "mode": "socket"
    }
  }
}
```

#### Email 渠道

```json
{
  "channels": {
    "email": {
      "enabled": true,
      "imap": {
        "server": "imap.gmail.com",
        "port": 993,
        "username": "your@gmail.com",
        "password": "app-password",
        "useTls": true
      },
      "smtp": {
        "server": "smtp.gmail.com",
        "port": 587,
        "username": "your@gmail.com",
        "password": "app-password",
        "useTls": true,
        "fromName": "stormclaw",
        "fromAddress": "your@gmail.com"
      },
      "checkInterval": 60,
      "allowFrom": ["sender@example.com"],
      "folder": "INBOX"
    }
  }
}
```

## 运行模式

### 1. CLI 交互模式

```bash
# 单次消息
stormclaw agent -m "分析以下数据..."

# 交互模式
stormclaw agent
> 你好！
> 帮我写一个快速排序算法
> exit
```

### 2. 网关模式

```bash
# 前台运行
stormclaw gateway

# 后台运行（Linux/macOS）
nohup stormclaw gateway > stormclaw.log 2>&1 &

# 后台运行（Windows PowerShell）
Start-Process powershell -ArgumentList "-NoExit", "-Command", "stormclaw gateway"
```

### 3. 特定渠道

```bash
# 仅启动 Telegram 渠道
# 在配置中禁用其他渠道
```

## 功能验证清单

### 基础功能

- [ ] 编译成功（`cargo build --release`）
- [ ] 测试通过（`cargo test --workspace`）
- [ ] 配置文件生成（`stormclaw onboard`）
- [ ] Agent 对话（`stormclaw agent -m "test"`）

### 渠道功能

- [ ] Telegram 接收消息
- [ ] Telegram 发送消息
- [ ] WhatsApp 网桥连接
- [ ] Discord 基础集成
- [ ] Slack 基础集成
- [ ] Email SMTP 发送
- [ ] Email IMAP 接收（需要 `email-imap` feature）

### 服务功能

- [ ] 定时任务创建（`stormclaw cron add`）
- [ ] 定时任务执行
- [ ] 心跳服务运行
- [ ] 网关 HTTP API（`curl http://localhost:3000/status`）

## 故障排查

### 编译失败

**问题**: `error: linking with link.exe failed`
**解决**: 确保项目路径不含中文字符

### 配置错误

**问题**: `Failed to load config`
**解决**: 检查 `~/.stormclaw/config.json` 语法

### API 密钥错误

**问题**: `Authentication failed`
**解决**: 检查 `providers.apiKey` 配置

### 渠道连接失败

**问题**: `Failed to connect to channel`
**解决**:
1. 检查 token 是否正确
2. 检查网络连接
3. 查看日志：`RUST_LOG=debug stormclaw gateway`

### 内存不足

**问题**: `Out of memory`
**解决**:
1. 调整消息队列容量
2. 启用 LRU 缓存
3. 限制并发数量

## 性能调优

### 调整队列容量

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "queueCapacity": 1000
    }
  }
}
```

### 启用性能监控

```rust
// 在代码中使用
let metrics = Metrics::new();
let _timer = Timer::message().with_metrics(metrics.clone());
```

### 查看性能指标

访问 `http://localhost:3000/metrics` 获取 Prometheus 格式指标。

## 更新升级

### 更新代码

```bash
git pull origin main
cargo build --release
sudo systemctl restart stormclaw  # 如果使用 systemd
```

### 数据迁移

```bash
# 备份配置
cp -r ~/.stormclaw ~/.stormclaw.backup

# 更新配置格式
# 编辑新配置文件

# 测试新版本
/usr/local/bin/stormclaw --version
```

## 安全建议

1. **保护 API 密钥**
   - 不要将配置文件提交到版本控制
   - 使用环境变量存储敏感信息
   - 定期轮换 API 密钥

2. **限制渠道访问**
   - 配置 `allowFrom` 列表
   - 使用独立的管理员账户

3. **启用 HTTPS**
   - 网关 API 使用 TLS
   - 使用安全的 IMAP/SMTP 连接

4. **日志管理**
   - 定期清理旧日志
   - 避免记录敏感信息

## 监控和维护

### 日志查看

```bash
# 实时日志
tail -f /var/log/stormclaw/stormclaw.log

# Systemd 日志
journalctl -u stormclaw -f

# Docker 日志
docker logs -f stormclaw
```

### 健康检查

```bash
# 检查服务状态
curl http://localhost:3000/health

# 检查服务状态
curl http://localhost:3000/status

# 检查指标
curl http://localhost:3000/metrics
```

### 定期维护

```bash
# 每周任务
- 清理过期会话
- 检查磁盘空间
- 更新依赖

# 每月任务
- 安全更新
- 性能分析
- 备份数据
```

## 支持和反馈

- GitHub Issues: https://github.com/deepelementlab/stormclaw/issues
- 文档: `docs/` 目录
- 开发指南: `docs/DEVELOPMENT.md`

## 快速参考

### 常用命令

```bash
# 安装
cargo install --path .

# 配置
stormclaw onboard

# 测试
stormclaw agent -m "test"

# 运行
stormclaw gateway

# 查看状态
stormclaw status

# 定时任务
stormclaw cron list
stormclaw cron add --name "daily" --every "1d" --message "Check status"

# 渠道管理
stormclaw channels status
stormclaw channels login telegram

# 会话管理
stormclaw session list
```

### 环境变量

```bash
# 日志级别
export RUST_LOG=debug

# 配置路径
export stormclaw_config=/path/to/config.json

# 工作区路径
export STORMCLAW_WORKSPACE=/path/to/workspace
```

---

**文档版本**: 1.0
**最后更新**: 2026-03-16
