//! stormclaw CLI - 个人 AI 助手命令行工具

use clap::{Parser, Subcommand};
use tracing_subscriber;

mod commands;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const NAME: &str = "stormclaw 🐈";

/// stormclaw - 个人 AI 助手
#[derive(Parser)]
#[command(name = "stormclaw")]
#[command(version = VERSION)]
#[command(arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

// NOTE: clap 的 derive 宏要求 version/long_version 为静态字符串；这里使用 `version = VERSION`。

#[derive(Subcommand)]
enum Commands {
    /// 初始化 stormclaw 配置和工作区
    Onboard(commands::onboard::OnboardArgs),

    /// 与 Agent 交互
    Agent(commands::agent::AgentArgs),

    /// 启动 stormclaw 网关
    Gateway(commands::gateway::GatewayArgs),

    /// 显示状态
    Status,

    /// 管理聊天渠道
    Channels {
        #[command(subcommand)]
        command: commands::channels::ChannelCommands,
    },

    /// 管理定时任务
    Cron {
        #[command(subcommand)]
        command: commands::cron::CronCommands,
    },

    /// 管理配置
    Config {
        #[command(subcommand)]
        command: commands::config::ConfigCommands,
    },

    /// 管理会话
    Session {
        #[command(subcommand)]
        command: commands::session::SessionCommands,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into())
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Onboard(args) => commands::onboard::run(args).await?,
        Commands::Agent(args) => commands::agent::run(args).await?,
        Commands::Gateway(args) => commands::gateway::run(args).await?,
        Commands::Status => commands::status::run().await?,
        Commands::Channels { command } => commands::channels::run(command).await?,
        Commands::Cron { command } => commands::cron::run(command).await?,
        Commands::Config { command } => commands::config::run(command).await?,
        Commands::Session { command } => commands::session::run(command).await?,
    }

    Ok(())
}
