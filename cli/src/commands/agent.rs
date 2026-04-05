//! Agent 命令 - 与 Agent 交互

use clap::Args;
use std::sync::Arc;
use rustyline::{DefaultEditor, error::ReadlineError};
use comfy_table::{Table, presets::UTF8_FULL};
use stormclaw_core::{AgentLoop, MessageBus, InboundMessage};

/// Agent 命令参数
#[derive(Args)]
pub struct AgentArgs {
    /// 要发送的消息
    #[arg(short, long)]
    pub message: Option<String>,

    /// 会话 ID
    #[arg(short, long, default_value = "cli:default")]
    pub session: String,

    /// 跳过响应的流式输出
    #[arg(short, long)]
    pub no_stream: bool,

    /// 包含系统消息在输出中
    #[arg(short, long)]
    pub verbose: bool,
}

pub async fn run(args: AgentArgs) -> anyhow::Result<()> {
    use stormclaw_config::load_config;
    use stormclaw_core::providers::OpenAIProvider;

    println!("🐈 stormclaw AI 助手\n");

    let config = load_config()?;
    let api_key = config.get_api_key()
        .ok_or_else(|| anyhow::anyhow!("未配置 API Key，请先运行 `stormclaw onboard`"))?;
    let api_base = config.get_api_base();
    let workspace = config.workspace_path();
    let model = config.agents.defaults.model.clone();
    let max_iterations = config.agents.defaults.max_tool_iterations;

    let brave_api_key = config.tools.web.search.api_key.clone();
    let brave_api_key = if brave_api_key.is_empty() { None } else { Some(brave_api_key) };

    // 创建组件
    let bus = Arc::new(MessageBus::new(100));
    let provider = Arc::new(OpenAIProvider::new(api_key, api_base, model));

    // 创建 Agent 循环
    let agent = AgentLoop::new(
        bus.clone(),
        provider.clone(),
        workspace.clone(),
        Some(config.agents.defaults.model),
        max_iterations,
        brave_api_key,
    ).await?;

    if let Some(message) = args.message {
        // 单次消息模式
        process_single_message(
            &agent,
            &bus,
            message,
            &args.session,
            args.verbose,
        ).await?;
    } else {
        // 交互模式
        run_interactive_mode(
            &agent,
            &bus,
            &args.session,
            args.verbose,
        ).await?;
    }

    Ok(())
}

/// 处理单条消息
async fn process_single_message<P>(
    agent: &AgentLoop<P>,
    bus: &Arc<MessageBus>,
    message: String,
    session_id: &str,
    verbose: bool,
) -> anyhow::Result<()>
where
    P: stormclaw_core::LLMProvider + 'static,
{
    println!("用户: {}", message);
    println!("\n处理中...\n");

    // 创建入站消息
    let (channel, chat_id) = parse_session_id(session_id);
    let inbound = InboundMessage::new(channel, "cli", chat_id, message);

    // 发送到消息总线
    bus.publish_inbound(inbound).await?;

    // 等待响应（简化版本，实际应该从总线接收）
    // 这里我们直接调用 Agent 处理
    println!("🐈 stormclaw: [Agent 处理中 - 功能开发中]\n");

    if verbose {
        print_system_info();
    }

    Ok(())
}

/// 运行交互模式
async fn run_interactive_mode<P>(
    agent: &AgentLoop<P>,
    bus: &Arc<MessageBus>,
    session_id: &str,
    verbose: bool,
) -> anyhow::Result<()>
where
    P: stormclaw_core::LLMProvider + 'static,
{
    println!("交互模式 (输入 /quit 退出, /help 查看帮助)\n");

    let mut rl = DefaultEditor::new()?;

    // 加载历史
    let history_path = stormclaw_utils::config_dir().join("history.txt");
    if let Ok(_) = std::fs::metadata(&history_path) {
        let _ = rl.load_history(&history_path);
    }

    let (channel, chat_id) = parse_session_id(session_id);
    let mut running = true;

    while running {
        let readline = rl.readline("你: ");

        match readline {
            Ok(line) => {
                let line = line.trim();

                // 保存历史
                let _ = rl.add_history_entry(line);

                if line.is_empty() {
                    continue;
                }

                // 处理命令
                if line.starts_with('/') {
                    running = handle_command(line, &mut rl, verbose)?;
                    continue;
                }

                println!("\n处理中...\n");

                // 创建入站消息
                let inbound = InboundMessage::new(channel.clone(), "cli", chat_id.clone(), line);
                bus.publish_inbound(inbound).await?;

                // 等待响应（简化版本）
                println!("🐈 stormclaw: [Agent 处理中 - 功能开发中]\n");

                if verbose {
                    print_system_info();
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
                continue;
            }
            Err(ReadlineError::Eof) => {
                println!("退出");
                break;
            }
            Err(err) => {
                anyhow::bail!("读取输入错误: {}", err);
            }
        }
    }

    // 保存历史
    let _ = rl.save_history(&history_path);

    Ok(())
}

/// 解析会话 ID
fn parse_session_id(session_id: &str) -> (String, String) {
    if let Some(idx) = session_id.find(':') {
        let channel = &session_id[..idx];
        let chat_id = &session_id[idx + 1..];
        (channel.to_string(), chat_id.to_string())
    } else {
        ("cli".to_string(), session_id.to_string())
    }
}

/// 处理命令
fn handle_command(cmd: &str, rl: &mut DefaultEditor, verbose: bool) -> anyhow::Result<bool> {
    let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
    let command = parts[0];
    let args = if parts.len() > 1 { Some(parts[1]) } else { None };

    match command {
        "/quit" | "/exit" => {
            return Ok(false);
        }
        "/help" => {
            print_help();
        }
        "/clear" => {
            // 清屏
            print!("\x1B[2J\x1B[1;1H");
            println!("🐈 stormclaw AI 助手\n");
        }
        "/info" => {
            print_system_info();
        }
        "/verbose" => {
            println!("详细模式: {}", if verbose { "开" } else { "关" });
        }
        _ => {
            println!("未知命令: {}。输入 /help 查看帮助。", command);
        }
    }

    Ok(true)
}

/// 打印帮助信息
fn print_help() {
    println!("可用命令:");
    println!("  /help, /?      显示此帮助");
    println!("  /quit, /exit   退出交互模式");
    println!("  /clear         清屏");
    println!("  /info          显示系统信息");
    println!();
}

/// 打印系统信息
fn print_system_info() {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["项目", "值"]);

    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
    table.add_row(vec!["当前时间", &now.to_string()]);

    let workspace = stormclaw_utils::default_workspace();
    table.add_row(vec!["工作区", &workspace.display().to_string()]);

    let config_path = stormclaw_config::get_config_path();
    table.add_row(vec!["配置文件", &config_path.display().to_string()]);

    println!("\n{}\n", table);
}
