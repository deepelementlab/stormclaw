//! Config 命令 - 管理配置

pub mod show;
pub mod set;
pub mod validate;
pub mod edit;

use clap::Subcommand;

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// 显示当前配置
    Show {
        /// 显示完整配置（包括敏感信息）
        #[arg(long)]
        full: bool,
        /// 只显示指定路径的配置（如 agents.defaults.model）
        path: Option<String>,
    },
    /// 设置配置项
    Set {
        /// 配置路径 (如 agents.defaults.model)
        #[arg(required = true)]
        path: String,
        /// 配置值
        #[arg(required = true)]
        value: String,
    },
    /// 验证配置
    Validate,
    /// 编辑配置文件
    Edit {
        /// 使用指定的编辑器
        #[arg(short, long)]
        editor: Option<String>,
    },
    /// 重置配置到默认值
    Reset {
        /// 确认重置
        #[arg(long)]
        confirm: bool,
    },
}

pub async fn run(command: ConfigCommands) -> anyhow::Result<()> {
    match command {
        ConfigCommands::Show { full, path } => show::run(full, path).await?,
        ConfigCommands::Set { path, value } => set::run(path, value).await?,
        ConfigCommands::Validate => validate::run().await?,
        ConfigCommands::Edit { editor } => edit::run(editor).await?,
        ConfigCommands::Reset { confirm } => {
            if !confirm {
                println!("⚠️  重置配置将删除所有自定义配置");
                println!("请使用 --confirm 确认此操作");
                return Ok(());
            }
            let cfg = stormclaw_config::Config::default();
            stormclaw_config::save_config(&cfg)?;
            println!(
                "已写入默认配置: {}",
                stormclaw_config::get_config_path().display()
            );
        }
    }
    Ok(())
}
