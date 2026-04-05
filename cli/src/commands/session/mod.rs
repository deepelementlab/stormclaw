//! Session 命令 - 管理会话

pub mod list;
pub mod show;
pub mod clear;
pub mod export;
pub mod import;

use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum SessionCommands {
    /// 列出所有会话
    List {
        /// 显示详细消息数量
        #[arg(short, long)]
        verbose: bool,
        /// 按更新时间排序 (asc, desc)
        #[arg(long)]
        sort: Option<String>,
        /// 限制显示数量
        #[arg(short, long)]
        limit: Option<usize>,
    },
    /// 查看会话详情
    Show {
        /// 会话 ID (使用 `list` 查看)
        #[arg(required = true)]
        id: String,
        /// 显示消息内容
        #[arg(short, long)]
        messages: bool,
        /// 显示的消息数量 (默认: 10)
        #[arg(long, default_value = "10")]
        count: usize,
    },
    /// 清除会话
    Clear {
        /// 会话 ID (使用 `all` 清除所有)
        #[arg(required = true)]
        id: String,
        /// 确认清除
        #[arg(short, long)]
        confirm: bool,
    },
    /// 导出会话
    Export {
        /// 会话 ID (使用 `all` 导出所有)
        #[arg(required = true)]
        id: String,
        /// 输出文件路径
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// 导出格式 (json, markdown, txt)
        #[arg(long, default_value = "json")]
        format: String,
    },
    /// 导入会话
    Import {
        /// 导入文件路径
        #[arg(required = true)]
        file: PathBuf,
        /// 覆盖已存在的会话
        #[arg(long)]
        overwrite: bool,
    },
}

pub async fn run(command: SessionCommands) -> anyhow::Result<()> {
    match command {
        SessionCommands::List { verbose, sort, limit } => {
            list::run(verbose, sort, limit).await?
        }
        SessionCommands::Show { id, messages, count } => {
            show::run(id, messages, count).await?
        }
        SessionCommands::Clear { id, confirm } => {
            clear::run(id, confirm).await?
        }
        SessionCommands::Export { id, output, format } => {
            export::run(id, output, format).await?
        }
        SessionCommands::Import { file, overwrite } => {
            import::run(file, overwrite).await?
        }
    }
    Ok(())
}
