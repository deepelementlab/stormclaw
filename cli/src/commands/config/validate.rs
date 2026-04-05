//! 验证配置

use stormclaw_config::{load_config, get_config_path};

pub async fn run() -> anyhow::Result<()> {
    let config_path = get_config_path();

    if !config_path.exists() {
        println!("❌ 配置文件不存在: {}", config_path.display());
        return Ok(()); // 不是错误，只是未初始化
    }

    println!("🔍 验证配置: {}\n", config_path.display());

    let config = load_config()?;

    let mut has_errors = false;
    let mut has_warnings = false;

    // 验证 Agent 配置
    println!("🤖 Agent 配置:");
    if config.agents.defaults.model.is_empty() {
        println!("  ⚠️  未设置默认模型");
        has_warnings = true;
    } else {
        println!("  ✓ 模型: {}", config.agents.defaults.model);
    }

    // 验证提供商配置
    println!("\n🔌 提供商配置:");

    let providers = vec![
        ("Anthropic", &config.providers.anthropic),
        ("OpenAI", &config.providers.openai),
        ("OpenRouter", &config.providers.openrouter),
    ];

    let mut has_api_key = false;
    for (name, provider) in providers {
        match provider {
            p if p.api_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false) => {
                println!("  ✓ {}: 已配置 API Key", name);
                has_api_key = true;
            }
            _ => {
                println!("  - {}: 未配置 API Key", name);
            }
        }
    }

    if !has_api_key {
        println!("  ⚠️  未配置任何 API Key");
        has_warnings = true;
    }

    // 验证渠道配置
    println!("\n📡 渠道配置:");

    if config.channels.telegram.enabled {
        if config.channels.telegram.token.is_empty() {
            println!("  ❌ Telegram: 已启用但未设置 token");
            has_errors = true;
        } else {
            println!("  ✓ Telegram: 已配置");
        }
    }

    if config.channels.whatsapp.enabled {
        println!("  ✓ WhatsApp: 已启用");
    }

    // 验证工作区
    println!("\n📁 工作区:");
    let workspace = config.workspace_path();
    if workspace.exists() {
        println!("  ✓ 工作区存在: {}", workspace.display());

        // 检查关键文件
        let files = vec!["USER.md", "AGENTS.md", "TOOLS.md", "MEMORY.md"];
        for file in files {
            let path = workspace.join(file);
            if path.exists() {
                println!("    ✓ {}", file);
            } else {
                println!("    - {} (不存在)", file);
            }
        }
    } else {
        println!("  ⚠️  工作区不存在: {}", workspace.display());
        println!("    提示: 使用 `stormclaw onboard` 初始化工作区");
        has_warnings = true;
    }

    // 验证数据目录
    println!("\n💾 数据目录:");
    let data_dir = stormclaw_utils::data_dir();
    if data_dir.exists() {
        println!("  ✓ 数据目录存在: {}", data_dir.display());
    } else {
        println!("  - 数据目录不存在 (将在需要时创建)");
    }

    // 总结
    println!("\n{}", "═".repeat(40));
    if has_errors {
        println!("❌ 配置验证失败");
        println!("\n请修复上述错误后重试");
        std::process::exit(1);
    } else if has_warnings {
        println!("⚠️  配置验证通过，但有警告");
    } else {
        println!("✅ 配置验证通过");
    }

    Ok(())
}
