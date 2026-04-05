//! Channels 命令 - 管理聊天渠道

use clap::Subcommand;
use stormclaw_config::load_config;
use comfy_table::{Table, presets::UTF8_FULL};
use stormclaw_utils::data_dir;

#[derive(Subcommand)]
pub enum ChannelCommands {
    /// 显示渠道状态
    Status {
        /// 显示详细状态
        #[arg(short, long)]
        verbose: bool,
    },
    /// 登录渠道（二维码扫描）
    Login {
        /// 渠道名称 (telegram, whatsapp)
        #[arg(default_value = "whatsapp")]
        channel: String,
    },
    /// 测试渠道连接
    Test {
        /// 渠道名称 (telegram, whatsapp, discord, slack, email)
        channel: String,
    },
    /// 启动渠道
    Start {
        /// 渠道名称
        channel: String,
    },
    /// 停止渠道
    Stop {
        /// 渠道名称
        channel: String,
    },
    /// 获取渠道帮助信息
    Info {
        /// 渠道名称
        channel: String,
    },
    /// Telegram 相关命令
    Telegram {
        #[command(subcommand)]
        command: TelegramCommands,
    },
    /// WhatsApp 相关命令
    Whatsapp {
        #[command(subcommand)]
        command: WhatsappCommands,
    },
}

#[derive(Subcommand)]
pub enum TelegramCommands {
    /// 设置 Webhook
    Webhook {
        /// Webhook URL
        url: String,
        /// 删除 Webhook
        #[arg(long)]
        delete: bool,
    },
    /// 获取 Bot 信息
    Info,
    /// 获取更新（用于调试）
    Updates,
}

#[derive(Subcommand)]
pub enum WhatsappCommands {
    /// 显示连接状态
    Status,
    /// 显示 QR 码
    Qr,
    /// 重置连接
    Reset,
}

pub async fn run(command: ChannelCommands) -> anyhow::Result<()> {
    match command {
        ChannelCommands::Status { verbose } => run_status(verbose).await?,
        ChannelCommands::Login { channel } => run_login(channel).await?,
        ChannelCommands::Test { channel } => run_test(channel).await?,
        ChannelCommands::Start { channel } => run_start(channel).await?,
        ChannelCommands::Stop { channel } => run_stop(channel).await?,
        ChannelCommands::Info { channel } => run_info(channel).await?,
        ChannelCommands::Telegram { command } => run_telegram(command).await?,
        ChannelCommands::Whatsapp { command } => run_whatsapp(command).await?,
    }
    Ok(())
}

async fn run_status(verbose: bool) -> anyhow::Result<()> {
    let config = load_config()?;

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_header(vec!["渠道", "状态", "配置"]);

    // WhatsApp
    let wa = &config.channels.whatsapp;
    table.add_row(vec![
        "WhatsApp",
        if wa.enabled { "✓ 启用" } else { "✗ 禁用" },
        &wa.bridge_url,
    ]);

    // Telegram
    let tg = &config.channels.telegram;
    let tg_cfg_display = if tg.token.is_empty() {
        "(未配置 token)".to_string()
    } else if tg.token.len() > 20 {
        format!("token: {}***", &tg.token[..20.min(tg.token.len())])
    } else {
        "(已配置)".to_string()
    };
    table.add_row(vec![
        "Telegram",
        if tg.enabled { "✓ 启用" } else { "✗ 禁用" },
        &tg_cfg_display,
    ]);

    // Discord
    // NOTE: 当前配置结构仅包含 telegram / whatsapp，其它渠道在 Rust 版中尚未接入配置层

    println!("\n{}", table);

    if verbose {
        println!("\n详细配置:");

        if tg.enabled {
            println!("\n📱 Telegram:");
            println!("  Token: {}***", &tg.token[..20.min(tg.token.len())]);
            println!("  允许列表: {:?}", tg.allow_from);
        }

        if wa.enabled {
            println!("\n💬 WhatsApp:");
            println!("  网桥 URL: {}", wa.bridge_url);
            println!("  允许列表: {:?}", wa.allow_from);
        }

        // 其它渠道暂未在配置层暴露
    }

    Ok(())
}

async fn run_login(channel: String) -> anyhow::Result<()> {
    match channel.as_str() {
        "whatsapp" => {
            println!("📱 WhatsApp 登录\n");
            println!("WhatsApp 登录需要 Node.js 网桥");
            println!("请确保已安装 Node.js >= 18\n");
            println!("登录流程：");
            println!("1. 进入网桥目录: cd bridge");
            println!("2. 安装依赖: npm install");
            println!("3. 启动网桥: npm start");
            println!("4. 使用 WhatsApp 扫描显示的二维码");
            println!("5. 扫描完成后，在另一个终端运行: stormclaw gateway");
            println!("\n提示: WhatsApp 网桥将在单独的窗口中运行");

            // 检查网桥目录
            let bridge_path = std::env::current_dir()?.join("bridge");
            if bridge_path.exists() {
                println!("\n✅ 网桥目录已存在: {}", bridge_path.display());

                let package_json = bridge_path.join("package.json");
                if package_json.exists() {
                    println!("✅ package.json 已存在");

                    let node_modules = bridge_path.join("node_modules");
                    if !node_modules.exists() {
                        println!("\n⚠️  需要安装依赖:");
                        println!("   cd bridge && npm install");
                    } else {
                        println!("✅ 依赖已安装");
                        println!("\n🚀 启动网桥:");
                        println!("   cd bridge && npm start");
                    }
                } else {
                    println!("\n❌ 网桥不完整，缺少 package.json");
                }
            } else {
                println!("\n❌ 网桥目录不存在: {}", bridge_path.display());
            }
        }
        "telegram" => {
            println!("📱 Telegram 登录\n");
            println!("Telegram 使用 Bot Token 进行认证");
            println!("请按以下步骤操作：");
            println!("1. 在 Telegram 中找到 @BotFather");
            println!("2. 发送 /newbot 创建新机器人");
            println!("3. 按提示设置机器人名称");
            println!("4. 获取 Bot Token");
            println!("5. 编辑配置文件添加 token:");
            println!("   ~/.stormclaw/config.json");
            println!("   {{\"channels\": {{\"telegram\": {{\"token\": \"YOUR_TOKEN\"}}}}}}");
        }
        _ => {
            println!("❌ 不支持的渠道: {}", channel);
            println!("支持的渠道: whatsapp, telegram");
        }
    }

    Ok(())
}

async fn run_test(channel: String) -> anyhow::Result<()> {
    println!("🔍 测试渠道连接: {}\n", channel);

    match channel.as_str() {
        "telegram" => {
            let config = load_config()?;
            let tg = &config.channels.telegram;

            if !tg.enabled || tg.token.is_empty() {
                println!("❌ Telegram 未配置或未启用");
                println!("提示: 使用 `stormclaw channels login telegram` 查看配置说明");
                return Ok(());
            }

            println!("正在测试 Telegram 连接...");
            println!("[连接测试功能需要实际 API 调用，开发中]");
        }
        "whatsapp" => {
            let config = load_config()?;
            let wa = &config.channels.whatsapp;

            if !wa.enabled {
                println!("❌ WhatsApp 未启用");
                return Ok(());
            }

            println!("正在测试 WhatsApp 网桥连接...");
            println!("网桥 URL: {}", wa.bridge_url);

            // 尝试连接到网桥
            use tokio_tungstenite::tungstenite::client::IntoClientRequest;
            let mut request = format!("{}/ws", wa.bridge_url.trim_end_matches('/'))
                .into_client_request()?;

            // 添加测试模式
            request.headers_mut().insert("X-Stormclaw-Test", "true".parse().unwrap());

            match tokio_tungstenite::connect_async(request).await {
                Ok((_, _)) => {
                    println!("✅ WhatsApp 网桥连接成功");
                    println!("提示: 确保已使用 WhatsApp 扫描二维码登录");
                }
                Err(e) => {
                    println!("❌ WhatsApp 网桥连接失败: {}", e);
                    println!("提示: 确保 WhatsApp 网桥正在运行");
                    println!("运行: cd bridge && npm start");
                }
            }
        }
        _ => {
            println!("❌ 不支持的渠道: {}", channel);
            println!("支持的渠道: telegram, whatsapp");
        }
    }

    Ok(())
}

async fn run_start(channel: String) -> anyhow::Result<()> {
    println!("🚀 启动渠道: {}", channel);
    println!("[渠道启动功能需要网关服务运行中]");
    println!("提示: 使用 `stormclaw gateway --channels {}` 启动指定渠道的网关", channel);
    Ok(())
}

async fn run_stop(channel: String) -> anyhow::Result<()> {
    println!("🛑 停止渠道: {}", channel);
    println!("[渠道停止功能需要网关服务运行中]");
    Ok(())
}

async fn run_info(channel: String) -> anyhow::Result<()> {
    match channel.as_str() {
        "telegram" => {
            println!("📱 Telegram 渠道\n");
            println!("Telegram 是一个基于云的即时通讯服务，支持 Bot API。\n");
            println!("配置:");
            println!("  - Token: 从 @BotFather 获取");
            println!("  - allowFrom: 允许访问的用户 ID/用户名列表\n");
            println!("功能:");
            println!("  - 文本消息");
            println!("  - Markdown/HTML 格式");
            println!("  - 图片、文档等附件");
            println!("  - 命令处理 (/start, /help 等)");
        }
        "whatsapp" => {
            println!("💬 WhatsApp 渠道\n");
            println!("WhatsApp 通过 Node.js 网桥连接，使用 @whiskeysockets/baileys 库。\n");
            println!("配置:");
            println!("  - bridgeUrl: 网桥 WebSocket 地址 (默认: ws://localhost:3001)");
            println!("  - allowFrom: 允许访问的电话号码列表\n");
            println!("功能:");
            println!("  - 文本消息");
            println!("  - 图片、文档等附件");
            println!("  - 群组支持");
        }
        "discord" => {
            println!("🎮 Discord 渠道\n");
            println!("Discord 是一个游戏社区和聊天平台。\n");
            println!("配置:");
            println!("  - Token: Bot Token");
            println!("  - allowFrom: 允许访问的用户/角色 ID");
            println!("  - commandPrefix: 命令前缀 (默认: !)\n");
            println!("功能:");
            println!("  - 文本消息");
            println!("  - 嵌入消息");
            println!("  - 附件支持");
            println!("  - Slash Commands");
        }
        "slack" => {
            println!("💼 Slack 渠道\n");
            println!("Slack 是一个企业通讯平台。\n");
            println!("配置:");
            println!("  - appToken: App-Level Token (xapp-)");
            println!("  - botToken: Bot Token (xoxb-)");
            println!("  - mode: http 或 socket\n");
            println!("功能:");
            println!("  - 文本消息");
            println!("  - 附件支持");
            println!("  - Block Kit");
        }
        "email" => {
            println!("📧 Email 渠道\n");
            println!("Email 渠道使用 IMAP 接收、SMTP 发送。\n");
            println!("配置:");
            println!("  - IMAP: 接收服务器配置");
            println!("  - SMTP: 发送服务器配置");
            println!("  - allowFrom: 允许的发送者邮箱\n");
            println!("功能:");
            println!("  - 接收邮件");
            println!("  - 发送邮件回复");
        }
        _ => {
            println!("❌ 未知的渠道: {}", channel);
            println!("已知渠道: telegram, whatsapp, discord, slack, email");
        }
    }

    Ok(())
}

async fn run_telegram(command: TelegramCommands) -> anyhow::Result<()> {
    match command {
        TelegramCommands::Webhook { url, delete } => {
            if delete {
                println!("🔧 删除 Telegram Webhook");
                println!("[Webhook 功能开发中]");
            } else {
                println!("🔧 设置 Telegram Webhook: {}", url);
                println!("[Webhook 功能开发中]");
            }
        }
        TelegramCommands::Info => {
            println!("📱 Telegram Bot 信息");
            println!("[获取 Bot 信息功能开发中]");
        }
        TelegramCommands::Updates => {
            println!("📨 获取 Telegram 更新");
            println!("[获取更新功能开发中]");
        }
    }

    Ok(())
}

async fn run_whatsapp(command: WhatsappCommands) -> anyhow::Result<()> {
    match command {
        WhatsappCommands::Status => {
            println!("💬 WhatsApp 连接状态");
            println!("[状态检查功能开发中]");
        }
        WhatsappCommands::Qr => {
            println!("📱 显示 WhatsApp QR 码");
            println!("QR 码将在启动网桥时显示");
            println!("运行: cd bridge && npm start");
        }
        WhatsappCommands::Reset => {
            println!("🔄 重置 WhatsApp 连接");

            let auth_dir = data_dir().join("whatsapp-auth");
            if auth_dir.exists() {
                println!("删除认证信息: {}", auth_dir.display());
                tokio::fs::remove_dir_all(&auth_dir).await?;
                println!("✅ 已删除 WhatsApp 认证信息");
                println!("下次启动时需要重新扫描二维码");
            } else {
                println!("❌ 没有找到 WhatsApp 认证信息");
            }
        }
    }

    Ok(())
}
