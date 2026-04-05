//! Status 命令 - 显示状态

pub async fn run() -> anyhow::Result<()> {
    use stormclaw_config::{load_config, get_config_path};
    use stormclaw_utils::{default_workspace, config_dir};

    println!("🐈 stormclaw 状态\n");

    let config_path = get_config_path();
    let workspace = default_workspace();

    println!("配置: {} {}",
        config_path.display(),
        if config_path.exists() { "✓" } else { "✗" }
    );
    println!("工作区: {} {}",
        workspace.display(),
        if workspace.exists() { "✓" } else { "✗" }
    );

    if config_path.exists() {
        if let Ok(config) = load_config() {
            println!("模型: {}", config.agents.defaults.model);

            let has_openrouter = config.providers.openrouter.api_key.is_some();
            let has_anthropic = config.providers.anthropic.api_key.is_some();
            let has_openai = config.providers.openai.api_key.is_some();

            println!("OpenRouter API: {}", if has_openrouter { "✓" } else { "-" });
            println!("Anthropic API: {}", if has_anthropic { "✓" } else { "-" });
            println!("OpenAI API: {}", if has_openai { "✓" } else { "-" });
        }
    }

    Ok(())
}
