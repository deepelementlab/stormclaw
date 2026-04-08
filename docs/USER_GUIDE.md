# stormclaw 用户指南

## 简介

stormclaw 是 stormclaw Python 版本的 Rust 复刻，提供更好的性能和资源效率。

## 安装

### 从源码构建

```bash
git clone https://github.com/deepelementlab/stormclaw.git
cd stormclaw
cargo build --release
```

### 使用预构建二进制文件

下载最新版本并解压，然后将二进制文件放到你的 PATH 中。

## 快速开始

### 1. 初始化配置

```bash
stormclaw onboard
```

这将在 `~/.stormclaw/` 创建配置文件和工作区。

### 2. 配置 API 密钥

编辑 `~/.stormclaw/config.json`：

```json
{
  "agents": {
    "defaults": {
      "model": "gpt-4"
    }
  },
  "providers": {
    "openrouter": {
      "apiKey": "your-api-key"
    }
  }
}
```

### 3. 发送第一条消息

```bash
stormclaw agent -m "Hello!"
```

## 功能特性

### CLI 交互

```bash
# 单次消息
stormclaw agent -m "你的问题"

# 交互模式
stormclaw agent
```

### 渠道集成

stormclaw 支持多种消息渠道：

#### Telegram

1. 在 Telegram 中找到 [@BotFather](https://t.me/BotFather)
2. 发送 `/newbot` 创建新机器人
3. 获取 Bot Token
4. 添加到配置文件：

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "YOUR_BOT_TOKEN",
      "allowFrom": []
    }
  }
}
```

5. 启动网关：

```bash
stormclaw gateway
```

#### WhatsApp

需要 Node.js >= 18。

```bash
# 登录 WhatsApp
stormclaw channels login whatsapp

# 在另一个终端启动网关
stormclaw gateway
```

#### Discord

```json
{
  "channels": {
    "discord": {
      "enabled": true,
      "token": "YOUR_BOT_TOKEN",
      "allowFrom": ["GUILD_ID"],
      "commandPrefix": "!"
    }
  }
}
```

#### Slack

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

#### Email

```json
{
  "channels": {
    "email": {
      "enabled": true,
      "imap": {
        "server": "imap.gmail.com",
        "port": 993,
        "username": "your-email@gmail.com",
        "password": "app-password",
        "useTls": true
      },
      "smtp": {
        "server": "smtp.gmail.com",
        "port": 587,
        "username": "your-email@gmail.com",
        "password": "app-password",
        "useTls": true,
        "fromName": "stormclaw",
        "fromAddress": "your-email@gmail.com"
      },
      "checkInterval": 60,
      "allowFrom": ["sender@example.com"]
    }
  }
}
```

### 定时任务

```bash
# 列出所有任务
stormclaw cron list

# 添加任务
stormclaw cron add --name "daily" --every "1d" --message "检查每日任务"

# 启用/禁用任务
stormclaw cron enable <id> true

# 手动运行任务
stormclaw cron run <id>
```

### 配置管理

```bash
# 显示配置
stormclaw config show
stormclaw config show --path agents.defaults.model

# 设置配置
stormclaw config set agents.defaults.model "gpt-4"

# 验证配置
stormclaw config validate

# 编辑配置
stormclaw config edit
```

### 会话管理

```bash
# 列出所有会话
stormclaw session list

# 查看会话详情
stormclaw session show <id>

# 清除会话
stormclaw session clear <id>

# 导出会话
stormclaw session export <id> --output session.json

# 导入会话
stormclaw session import session.json
```

## 工作区结构

stormclaw 会在工作区（默认 `~/.stormclaw/workspace`）中创建以下目录：

```
~/.stormclaw/
├── config.json         # 配置文件
├── workspace/          # 工作区
│   ├── sessions/       # 会话存储
│   ├── memory/         # 记忆存储
│   └── skills/         # 技能文件
└── HEARTBEAT.md        # 心跳任务文件
```

## 技能系统

stormclaw 支持通过 YAML 文件定义自定义技能。

技能文件示例：

```yaml
---
name: weather
description: 获取天气信息
category: utility

parameters:
  location:
    type: string
    description: 城市名称

steps:
  - 使用 web_search 工具搜索 "{{location}} 天气"
  - 从搜索结果中提取天气信息
  - 总结天气情况
```

将技能文件保存到工作区的 `skills/` 目录即可。

## 高级功能

### 心跳服务

stormclaw 会定期检查 `HEARTBEAT.md` 文件并执行其中列出的任务。

编辑 `~/.stormclaw/workspace/HEARTBEAT.md`：

```markdown
# 心跳任务

## 每日任务

- [x] 检查每日日程
- [ ] 回复待处理消息
- [ ] 生成每日报告
```

### 子代理生成

Agent 可以生成子代理来并行处理后台任务。

使用 `spawn` 工具：

```
请帮我搜索 Rust 性能优化的最佳实践，并总结成文档
```

Agent 会创建子代理来执行这个任务，完成后会通知你。

## 故障排查

### 常见问题

**Q: 编译失败怎么办？**

A: 确保你使用的是最新的稳定版 Rust：

```bash
rustup update
cargo build --release
```

**Q: 渠道连接失败**

A: 检查配置文件中的 token 和凭证是否正确。

**Q: 如何启用调试日志？**

A: 设置环境变量：

```bash
export RUST_LOG=debug
stormclaw gateway
```

## 性能优化

stormclaw 相比 Python 版本有以下性能提升：

| 指标 | Python 版本 | Rust 版本 |
|------|-----------|----------|
| 启动时间 | ~500ms | ~50ms |
| 内存占用 | ~100MB | ~30MB |
| 消息处理延迟 | ~200ms | ~50ms |
| 并发连接数 | ~100 | ~10000+ |

## 更多信息

- [API 文档](./API_REFERENCE.md)
- [开发指南](./DEVELOPMENT.md)
- [贡献指南](./CONTRIBUTING.md)
