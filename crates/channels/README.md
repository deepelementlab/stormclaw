# stormclaw 渠道模块文档

## 概述

渠道模块提供了与各种聊天平台和通信协议的集成。所有渠道都实现 `BaseChannel` trait，提供统一的接口。

## 支持的渠道

### 1. Telegram

使用 [teloxide](https://github.com/teloxide/teloxide) 库实现。

```toml
[channels.telegram]
enabled = true
token = "YOUR_BOT_TOKEN"
allow_from = ["123456789"]  # 用户 ID 列表
```

**获取用户 ID**: 使用 [@userinfobot](https://t.me/userinfobot)

### 2. WhatsApp

通过 WebSocket 连接到 Node.js 网桥实现。

```toml
[channels.whatsapp]
enabled = true
bridge_url = "ws://localhost:3001"
allow_from = ["86180000000"]  # 手机号列表
```

**启动网桥**: `stormclaw channels login`

### 3. Discord

```toml
[channels.discord]
enabled = true
token = "YOUR_BOT_TOKEN"
allow_from = ["GUILD_ID_1", "GUILD_ID_2"]
```

### 4. Slack

```toml
[channels.slack]
enabled = true
bot_token = "xoxb-YOUR_BOT_TOKEN"
app_token = "xapp-YOUR_APP_TOKEN"
allow_from = ["TEAM_ID"]
```

### 5. Webhook

通用 HTTP Webhook 接口，可接收来自任何服务的消息。

```rust
// POST /webhook/YOUR_API_KEY
{
  "channel": "custom",
  "sender_id": "user123",
  "chat_id": "room456",
  "content": "Hello from webhook!"
}
```

### 6. Email (IMAP/SMTP)

```toml
[channels.email]
enabled = true
imap_server = "imap.gmail.com"
imap_port = 993
imap_username = "your-email@gmail.com"
imap_password = "YOUR_APP_PASSWORD"
smtp_server = "smtp.gmail.com"
smtp_port = 587
check_interval = 60  # 秒
```

### 7. CLI

命令行界面虚拟渠道，用于本地测试。

## 创建自定义渠道

### 方法 1: 实现 BaseChannel trait

```rust
use async_trait::async_trait;
use stormclaw_channels::{BaseChannel, ChannelState};
use stormclaw_core::{MessageBus, OutboundMessage};

pub struct MyChannel {
    bus: Arc<MessageBus>,
    // 添加你的字段
}

#[async_trait]
impl BaseChannel for MyChannel {
    fn name(&self) -> &str {
        "mychannel"
    }

    async fn start(&self) -> anyhow::Result<()> {
        // 启动逻辑
    }

    async fn stop(&self) -> anyhow::Result<()> {
        // 停止逻辑
    }

    async fn send(&self, msg: &OutboundMessage) -> anyhow::Result<()> {
        // 发送消息
    }

    fn bus(&self) -> &MessageBus {
        &self.bus
    }
}
```

### 方法 2: 使用模板

参考 `template.rs` 文件中的 `CustomChannelTemplate`。

## 消息格式

不同渠道可能有不同的消息格式限制：

| 渠道 | 粗体 | 斜体 | 代码 | 链接 |
|-----|------|------|------|------|
| Telegram | `*text*` | `_text_` | `` `code` `` | [text](url) |
| Discord | `**text**` | `*text*` | `` `code` `` | [text](url) |
| Slack | `*text*` | `_text_` | `` `code` `` | <url\|text> |
| WhatsApp | `*text*` | `_text_` | 不支持 | 不支持 |

使用 `MarkdownConverter` 自动转换格式。

## 测试渠道

```rust
use stormclaw_channels::ChannelTester;

let tester = ChannelTester::new(bus);

// 测试单个渠道
let result = tester.test_connection("telegram").await?;
println!("Latency: {:?}", result.latency);

// 批量测试
let results = tester.test_all_channels(&["telegram", "whatsapp"]).await;
```

## 监控渠道健康

```rust
use stormclaw_channels::{ChannelMonitor, ChannelStatsCollector};

let monitor = ChannelMonitor::new(bus);
monitor.register_channel("telegram".to_string()).await;

// 检查所有渠道健康状态
let health = monitor.get_all_health().await;
for (name, h) in health {
    println!("{}: {:?}", name, h.status);
}
```

## 配置示例

完整的配置文件示例：

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "123456:ABC-DEF",
      "allowFrom": ["123456789"]
    },
    "whatsapp": {
      "enabled": true,
      "bridgeUrl": "ws://localhost:3001",
      "allowFrom": ["86180000000"]
    },
    "discord": {
      "enabled": false,
      "token": "BOT_TOKEN",
      "allowFrom": []
    },
    "webhook": {
      "enabled": false,
      "bindAddress": "0.0.0.0",
      "port": 8765,
      "apiKey": "YOUR_SECRET_KEY"
    }
  }
}
```

## 故障排查

### Telegram 无响应

1. 检查 bot token 是否正确
2. 使用 `/setprivacy` 关闭隐私模式（开发阶段）
3. 检查用户 ID 是否在 `allow_from` 列表中

### WhatsApp 连接失败

1. 确保 Node.js 网桥正在运行
2. 检查 WebSocket 连接地址
3. 重新扫描二维码

### Discord 权限问题

确保 bot 具有以下权限：
- `Send Messages`
- `Read Messages/View Channels`
- `Message Content`
