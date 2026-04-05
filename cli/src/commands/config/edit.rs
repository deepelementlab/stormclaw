//! 编辑配置文件

use stormclaw_config::get_config_path;
use std::process::Command;

pub async fn run(editor: Option<String>) -> anyhow::Result<()> {
    let config_path = get_config_path();

    if !config_path.exists() {
        println!("❌ 配置文件不存在: {}", config_path.display());
        println!("\n提示: 使用 `stormclaw onboard` 初始化配置");
        return Ok(());
    }

    // 确定编辑器
    let editor_cmd = editor.or_else(|| {
        // 按优先级检查环境变量
        if let Ok(e) = std::env::var("STORMCLAW_EDITOR") {
            Some(e)
        } else if let Ok(e) = std::env::var("EDITOR") {
            Some(e)
        } else if let Ok(e) = std::env::var("VISUAL") {
            Some(e)
        } else {
            // 平台特定的默认编辑器
            #[cfg(target_os = "windows")]
            { Some("notepad".to_string()) }
            #[cfg(target_os = "macos")]
            { Some("open".to_string()) }
            #[cfg(target_os = "linux")]
            {
                // 检查可用的编辑器
                if which::which("nano").is_ok() {
                    Some("nano".to_string())
                } else if which::which("vim").is_ok() {
                    Some("vim".to_string())
                } else if which::which("vi").is_ok() {
                    Some("vi".to_string())
                } else {
                    None
                }
            }
        }
    });

    let editor_cmd = editor_cmd.ok_or_else(|| {
        anyhow::anyhow!("未找到编辑器。请设置 EDITOR 环境变量或使用 --editor 参数")
    })?;

    println!("📝 打开配置文件: {}", config_path.display());
    println!("使用编辑器: {}\n", editor_cmd);

    // 启动编辑器
    let status = if editor_cmd == "open" {
        // macOS: 使用 open 命令
        Command::new("open")
            .arg("-t")
            .arg(&config_path)
            .status()?
    } else if editor_cmd == "notepad" {
        // Windows: 使用 notepad
        Command::new("notepad")
            .arg(&config_path)
            .status()?
    } else {
        // Unix: 使用指定的编辑器
        Command::new(&editor_cmd)
            .arg(&config_path)
            .status()?
    };

    if status.success() {
        println!("\n✅ 编辑完成");

        // 验证配置
        println!("🔍 验证配置...");
        if let Err(e) = stormclaw_config::load_config() {
            println!("⚠️  配置验证失败: {}", e);
            println!("提示: 使用 `stormclaw config validate` 检查配置");
        } else {
            println!("✅ 配置有效");
        }
    } else {
        println!("⚠️  编辑器退出时可能有问题");
    }

    Ok(())
}
