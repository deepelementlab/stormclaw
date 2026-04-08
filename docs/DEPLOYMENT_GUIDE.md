# stormclaw Rust 版本安装部署手册

## 目录

1. [系统要求](#1-系统要求)
2. [开发环境搭建](#2-开发环境搭建)
3. [源码编译](#3-源码编译)
4. [配置说明](#4-配置说明)
5. [安装部署](#5-安装部署)
6. [运行指南](#6-运行指南)
7. [Docker 部署](#7-docker-部署)
8. [故障排查](#8-故障排查)
9. [升级指南](#9-升级指南)
10. [生产环境建议](#10-生产环境建议)

## 1. 系统要求

### 1.1 最低要求

| 组件 | 要求 |
|------|------|
| 操作系统 | Linux / macOS / Windows (WSL2) |
| CPU | 64位处理器 |
| 内存 | 512MB RAM |
| 磁盘 | 100MB 可用空间 |

### 1.2 推荐配置

| 组件 | 要求 |
|------|------|
| 操作系统 | Ubuntu 22.04+ / Debian 12+ / macOS 13+ |
| CPU | 2核及以上 |
| 内存 | 2GB RAM |
| 磁盘 | 500MB 可用空间 |

### 1.3 开发环境要求

| 工具 | 版本 |
|------|------|
| Rust | 1.75+ |
| Cargo | 包含在 Rust 中 |
| Git | 任意版本 |
| OpenSSL | 开发库 |

## 2. 开发环境搭建

### 2.1 安装 Rust

#### Linux/macOS

```bash
# 使用 rustup 安装
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 配置环境变量
source $HOME/.cargo/env
```

#### Windows

```powershell
# 下载并运行 rustup-init.exe
# 访问 https://rustup.rs/

# 或使用 winget
winget install Rustlang.Rustup
```

### 2.2 验证安装

```bash
# 检查 Rust 版本
rustc --version

# 检查 Cargo 版本
cargo --version

# 检查工具链
rustup show
```

预期输出：
```
rustc 1.75.0
cargo 1.75.0
```

### 2.3 安装系统依赖

#### Ubuntu/Debian

```bash
sudo apt-get update
sudo apt-get install -y \
    pkg-config \
    libssl-dev \
    build-essential \
    curl
```

#### macOS

```bash
# 安装 Xcode Command Line Tools
xcode-select --install

# 或使用 Homebrew
brew install openssl
```

#### Windows (WSL2)

```bash
sudo apt-get update
sudo apt-get install -y \
    pkg-config \
    libssl-dev \
    build-essential \
    curl
```

## 3. 源码编译

### 3.1 获取源码

```bash
# 克隆仓库
git clone https://github.com/deepelementlab/stormclaw.git
cd stormclaw

# 或使用已存在的项目
cd D:\开源项目\stormclaw-main\stormclaw-main
```

### 3.2 编译项目

#### 开发版本编译

```bash
# 编译所有模块
cargo build

# 编译特定模块
cargo build -p stormclaw-core
cargo build -p stormclaw-cli
```

#### 发布版本编译

```bash
# 优化编译
cargo build --release

# 编译结果位置
ls -lh target/release/stormclaw
```

### 3.3 编译选项

```bash
# 静态编译 (Linux)
cargo build --release --target x86_64-unknown-linux-musl

# 交叉编译到 Windows
cargo build --release --target x86_64-pc-windows-gnu

# 交叉编译到 macOS (需要 osxcross)
cargo build --release --target x86_64-apple-darwin
```

### 3.4 编译问题排查

#### OpenSSL 链接错误

```bash
# Ubuntu/Debian
sudo apt-get install libssl-dev pkg-config

# 设置环境变量
export OPENSSL_DIR=/usr
export OPENSSL_LIB_DIR=/usr/lib/x86_64-linux-gnu
```

#### 其他依赖问题

```bash
# 清理并重新编译
cargo clean
cargo build --release
```

## 4. 配置说明

### 4.1 初始化配置

```bash
# 运行初始化向导
./target/release/stormclaw onboard

# 或使用开发版本
cargo run -- onboard
```

初始化过程会创建：
1. 配置文件：`~/.stormclaw/config.json`
2. 工作区目录：`~/.stormclaw/workspace/`
3. 模板文件：`AGENTS.md`, `SOUL.md`, `USER.md`, `TOOLS.md`
4. 记忆目录：`workspace/memory/`

### 4.2 配置文件结构

```json
{
  "agents": {
    "defaults": {
      "workspace": "~/.stormclaw/workspace",
      "model": "anthropic/claude-opus-4-5",
      "maxTokens": 8192,
      "temperature": 0.7,
      "maxToolIterations": 20
    }
  },
  "channels": {
    "telegram": {
      "enabled": false,
      "token": "",
      "allowFrom": []
    },
    "whatsapp": {
      "enabled": false,
      "bridgeUrl": "ws://localhost:3001",
      "allowFrom": []
    }
  },
  "providers": {
    "anthropic": {
      "apiKey": null,
      "apiBase": null
    },
    "openai": {
      "apiKey": null,
      "apiBase": null
    },
    "openrouter": {
      "apiKey": null,
      "apiBase": null
    }
  },
  "gateway": {
    "host": "0.0.0.0",
    "port": 18789
  },
  "tools": {
    "web": {
      "search": {
        "apiKey": "",
        "maxResults": 5
      }
    }
  }
}
```

### 4.3 环境变量配置

支持通过环境变量覆盖配置：

```bash
# API Key
export STORMCLAW_API_KEY="sk-or-v1-xxx"
export ANTHROPIC_API_KEY="sk-ant-xxx"

# API Base
export OPENAI_API_BASE="https://api.openai.com/v1"

# 工作区
export STORMCLAW_WORKSPACE="/path/to/workspace"
```

### 4.4 获取 API Keys

#### OpenRouter

```
1. 访问 https://openrouter.ai/keys
2. 注册账号
3. 创建 API Key
4. 复制 Key 到配置文件
```

#### Anthropic Claude

```
1. 访问 https://console.anthropic.com/
2. 注册账号
3. 创建 API Key
4. 复制 Key 到配置文件
```

#### OpenAI

```
1. 访问 https://platform.openai.com/api-keys
2. 创建 API Key
3. 复制 Key 到配置文件
```

#### Brave Search (Web 搜索)

```
1. 访问 https://search.brave.com/api/
2. 注册账号
3. 创建 API Key
4. 配置到 tools.web.search.apiKey
```

## 5. 安装部署

### 5.1 系统安装

#### Linux/macOS

```bash
# 复制二进制文件
sudo cp target/release/stormclaw /usr/local/bin/

# 确保可执行
sudo chmod +x /usr/local/bin/stormclaw

# 验证安装
stormclaw --version
```

#### Windows

```powershell
# 复制到系统路径
copy target\stormclaw.exe C:\Windows\System32\

# 或添加到 PATH
setx PATH "%PATH%;C:\path\to\stormclaw"

# 验证安装
stormclaw.exe --version
```

### 5.2 创建服务文件

#### systemd (Linux)

创建 `/etc/systemd/system/stormclaw.service`:

```ini
[Unit]
Description=stormclaw AI Assistant
After=network.target

[Service]
Type=simple
User=stormclaw
WorkingDirectory=/home/stormclaw/.stormclaw
ExecStart=/usr/local/bin/stormclaw gateway
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

启动服务：

```bash
# 重载配置
sudo systemctl daemon-reload

# 启用开机启动
sudo systemctl enable stormclaw

# 启动服务
sudo systemctl start stormclaw

# 查看状态
sudo systemctl status stormclaw

# 查看日志
sudo journalctl -u stormclaw -f
```

### 5.3 用户权限配置

```bash
# 创建专用用户
sudo useradd -r -s /bin/bash stormclaw

# 设置权限
sudo chown -R stormclaw:stormclaw /home/stormclaw/.stormclaw
```

## 6. 运行指南

### 6.1 初始化

```bash
# 首次运行，初始化配置
stormclaw onboard
```

交互式向导会引导你：
1. 创建配置文件
2. 设置工作区
3. 创建模板文件
4. 提示添加 API Key

### 6.2 命令行交互

#### 单次消息模式

```bash
stormclaw agent -m "你好！"
```

#### 交互模式

```bash
stormclaw agent

# 交互命令
你: 帮我列出当前目录的文件
你: /help    # 查看帮助
你: /quit   # 退出
```

#### 内置命令

| 命令 | 功能 |
|------|------|
| /help, /? | 显示帮助 |
| /quit, /exit | 退出交互 |
| /clear | 清屏 |
| /info | 显示系统信息 |

### 6.3 启动网关服务

```bash
# 前台运行
stormclaw gateway

# 指定端口
stormclaw gateway --port 8080

# 禁用心跳
stormclaw gateway --no-heartbeat

# 禁用定时任务
stormclaw gateway --no-cron

# 指定渠道
stormclaw gateway --channels telegram,whatsapp
```

### 6.4 查看状态

```bash
stormclaw status
```

输出示例：
```
🐈 stormclaw 状态

配置: /home/user/.stormclaw/config.json ✓
工作区: /home/user/.stormclaw/workspace ✓
模型: anthropic/claude-opus-4-5
OpenRouter API: ✓
Anthropic API: -
OpenAI API: -
```

## 7. Docker 部署

### 7.1 Dockerfile

创建 `Dockerfile`:

```dockerfile
# 构建阶段
FROM rust:1.75-slim as builder

WORKDIR /app

# 安装依赖
RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# 复制 Cargo 配置
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY cli/ cli/

# 构建
RUN cargo build --release && \
    mv target/release/stormclaw /usr/local/bin/

# 运行阶段
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# 创建非 root 用户
RUN useradd -m -u 1000 stormclaw

# 复制二进制文件
COPY --from=builder /usr/local/bin/stormclaw /usr/local/bin/

# 工作目录
WORKDIR /home/stormclaw/.stormclaw

USER stormclaw

ENTRYPOINT ["stormclaw"]
```

### 7.2 构建镜像

```bash
# 构建镜像
docker build -t stormclaw:latest .

# 或指定标签
docker build -t stormclaw:v1.0.0 .
```

### 7.3 运行容器

```bash
# 运行网关服务
docker run -d \
    --name stormclaw \
    -v ~/.stormclaw:/home/stormclaw/.stormclaw \
    -p 18789:18789 \
    stormclaw:latest gateway

# 查看日志
docker logs -f stormclaw

# 停止容器
docker stop stormclaw

# 删除容器
docker rm stormclaw
```

### 7.4 Docker Compose

创建 `docker-compose.yml`:

```yaml
version: '3.8'

services:
  stormclaw:
    build: .
    container_name: stormclaw
    restart: unless-stopped
    ports:
      - "18789:18789"
    volumes:
      - ./data:/home/stormclaw/.stormclaw
      - ./workspace:/home/stormclaw/.stormclaw/workspace
    environment:
      - RUST_LOG=info
      - STORMCLAW_API_KEY=${API_KEY}
```

运行：

```bash
# 启动服务
docker-compose up -d

# 查看日志
docker-compose logs -f

# 停止服务
docker-compose down
```

## 8. 故障排查

### 8.1 常见问题

#### 问题：编译失败

```bash
# 清理构建缓存
cargo clean
cargo build --release
```

#### 问题：API Key 无效

```bash
# 检查配置文件
cat ~/.stormclaw/config.json | grep apiKey

# 测试 API Key
curl https://openrouter.ai/api/v1/models \
  -H "Authorization: Bearer YOUR_API_KEY"
```

#### 问题：渠道连接失败

```bash
# 检查网络连接
ping api.telegram.org

# 检查端口占用
netstat -tuln | grep LISTEN

# 检查防火墙
sudo ufw status
```

### 8.2 日志查看

#### 应用日志

```bash
# 开发环境
RUST_LOG=debug stormclaw gateway

# 生产环境
RUST_LOG=info stormclaw gateway
```

#### 系统日志

```bash
# systemd 服务
sudo journalctl -u stormclaw -n 100

# 实时跟踪
sudo journalctl -u stormclaw -f
```

#### Docker 日志

```bash
# 查看最近日志
docker logs --tail 100 stormclaw

# 实时跟踪
docker logs -f stormclaw
```

### 8.3 性能问题

#### 内存使用过高

```bash
# 检查内存
ps aux | grep stormclaw

# 限制内存 (systemd)
# 在服务文件中添加：
MemoryLimit=512M
```

#### CPU 使用过高

```bash
# 限制 CPU (systemd)
CPUQuota=50%
```

## 9. 升级指南

### 9.1 版本升级

#### 从源码升级

```bash
# 拉取最新代码
git pull origin main

# 编译新版本
cargo build --release

# 备份配置
cp ~/.stormclaw/config.json ~/.stormclaw/config.json.bak

# 替换二进制文件
sudo cp target/release/stormclaw /usr/local/bin/stormclaw

# 重启服务
sudo systemctl restart stormclaw
```

#### Docker 升级

```bash
# 拉取最新镜像
docker pull stormclaw:latest

# 停止并删除旧容器
docker stop stormclaw
docker rm stormclaw

# 启动新容器
docker run -d --name stormclaw stormclaw:latest gateway
```

### 9.2 数据迁移

#### 配置文件迁移

```bash
# 备份旧配置
cp ~/.stormclaw/config.json ~/.stormclaw/config.json.old

# 检查配置兼容性
stormclaw doctor
```

#### 会话数据迁移

会话数据格式兼容，无需特殊处理。

## 10. 生产环境建议

### 10.1 安全建议

1. **API Key 保护**
   - 使用环境变量存储敏感信息
   - 不要将配置文件提交到版本控制

2. **权限控制**
   - 配置 `allowFrom` 列表
   - 使用专用用户运行服务

3. **网络安全**
   - 使用防火墙限制端口访问
   - 考虑使用反向代理

### 10.2 性能优化

1. **资源配置**
   ```ini
   # systemd 服务配置
   MemoryLimit=512M
   CPUQuota=100%
   TasksMax=100
   ```

2. **连接池配置**
   ```toml
   # 在配置中设置连接池大小
   [http]
   poolSize = 100
   timeout = 30
   ```

### 10.3 监控建议

1. **健康检查**
   ```bash
   # 定期检查健康端点
   curl http://localhost:18789/health
   ```

2. **日志轮转**
   ```
   # /etc/logrotate.d/stormclaw
   /var/log/stormclaw/*.log {
       daily
       rotate 7
       compress
       missingok
       notifempty
   }
   ```

3. **监控指标**
   ```bash
   # 获取服务状态
   curl http://localhost:18789/status
   ```

### 10.4 高可用部署

```yaml
# 多实例部署示例
services:
  stormclaw-1:
    image: stormclaw:latest
    ports: ["18789:18789"]
  stormclaw-2:
    image: stormclaw:latest
    ports: ["18790:18789"]
  nginx:
    image: nginx:alpine
    ports: ["80:80"]
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf:ro
```

---

**文档版本**: 1.0
**最后更新**: 2026-03-12
**维护者**: stormclaw 项目组
