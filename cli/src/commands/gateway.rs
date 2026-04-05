//! Gateway 命令 - 启动网关服务

use clap::Args;
use tokio::signal::ctrl_c;
use tracing::{info, warn, error};

/// Gateway 命令参数
#[derive(Args)]
pub struct GatewayArgs {
    /// 网关端口
    #[arg(short, long, default_value = "18789")]
    pub port: u16,

    /// 绑定地址
    #[arg(short, long, default_value = "0.0.0.0")]
    pub host: String,

    /// 详细输出
    #[arg(short, long)]
    pub verbose: bool,

    /// 禁用心跳服务
    #[arg(long)]
    pub no_heartbeat: bool,

    /// 禁用定时任务
    #[arg(long)]
    pub no_cron: bool,

    /// 仅启用指定渠道
    #[arg(short, long, value_delimiter = ',')]
    pub channels: Option<Vec<String>>,
}

pub async fn run(args: GatewayArgs) -> anyhow::Result<()> {
    use stormclaw_config::load_config;
    use stormclaw_services::{GatewayConfig, GatewayService};

    println!("🐈 启动 stormclaw 网关\n");

    // 加载配置
    let config = load_config()?;
    let gateway_config = GatewayConfig {
        host: args.host.clone(),
        port: args.port,
        http_enabled: true,
        metrics_enabled: true,
    };

    print_gateway_config(&args, &determine_channels(&args.channels, &config), &config);

    let gateway = GatewayService::new(config, gateway_config).await?;

    println!("\n按 Ctrl+C 停止\n");
    let handle = tokio::spawn(async move {
        if let Err(e) = gateway.start().await {
            error!("Gateway error: {}", e);
        }
    });

    ctrl_c().await?;
    println!("\n正在关闭...");
    // NOTE: 网关 start() 内部已包含 shutdown_signal；这里等待 task 退出即可
    let _ = tokio::time::timeout(tokio::time::Duration::from_secs(10), handle).await;
    println!("已关闭");

    Ok(())
}

/// 确定要启用的渠道
fn determine_channels(
    cli_channels: &Option<Vec<String>>,
    config: &stormclaw_config::Config,
) -> Vec<String> {
    if let Some(channels) = cli_channels {
        return channels.clone();
    }

    let mut enabled = Vec::new();

    if config.channels.telegram.enabled {
        enabled.push("telegram".to_string());
    }

    if config.channels.whatsapp.enabled {
        enabled.push("whatsapp".to_string());
    }

    // 默认启用 CLI 渠道
    enabled.push("cli".to_string());

    enabled
}

/// 打印网关配置信息
fn print_gateway_config(args: &GatewayArgs, channels: &[String], config: &stormclaw_config::Config) {
    use comfy_table::{Table, presets::UTF8_FULL};

    println!("配置信息:");

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["项目", "值"]);

    table.add_row(vec![
        "监听地址",
        &format!("{}:{}", args.host, args.port)
    ]);

    table.add_row(vec![
        "工作区",
        &config.workspace_path().display().to_string()
    ]);

    table.add_row(vec![
        "模型",
        &config.agents.defaults.model
    ]);

    table.add_row(vec![
        "最大迭代",
        &config.agents.defaults.max_tool_iterations.to_string()
    ]);

    println!("{}", table);

    println!("\n已启用渠道:");
    if channels.is_empty() {
        println!("  (无)");
    } else {
        for channel in channels {
            println!("  ✓ {}", channel);
        }
    }

    println!("\n服务状态:");
    println!("  ✓ Agent 循环");
    println!("  {} 心跳服务", if args.no_heartbeat { "✗" } else { "✓" });
    println!("  {} 定时任务", if args.no_cron { "✗" } else { "✓" });
}

/// 网关状态监控器
pub struct GatewayMonitor {
    start_time: chrono::DateTime<chrono::Utc>,
}

impl GatewayMonitor {
    pub fn new() -> Self {
        Self {
            start_time: chrono::Utc::now(),
        }
    }

    pub fn uptime(&self) -> chrono::Duration {
        chrono::Utc::now() - self.start_time
    }

    pub fn print_status(&self) {
        let uptime = self.uptime();
        let secs = uptime.num_seconds();

        let uptime_str = if secs < 60 {
            format!("{}秒", secs)
        } else if secs < 3600 {
            format!("{}分{}秒", secs / 60, secs % 60)
        } else {
            format!("{}小时{}分", secs / 3600, (secs % 3600) / 60)
        };

        println!("运行时间: {}", uptime_str);
    }
}
