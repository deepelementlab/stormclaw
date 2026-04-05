//! 显示配置

use stormclaw_config::{load_config, get_config_path};
use serde_json::Value;

pub async fn run(full: bool, path: Option<String>) -> anyhow::Result<()> {
    let config_path = get_config_path();

    if !config_path.exists() {
        println!("❌ 配置文件不存在: {}", config_path.display());
        println!("\n提示: 使用 `stormclaw onboard` 初始化配置");
        return Ok(());
    }

    let config = load_config()?;
    let config_json = serde_json::to_value(&config)?;

    println!("📁 配置文件: {}", config_path.display());

    if let Some(path_str) = path {
        // 显示指定路径的配置
        let parts: Vec<&str> = path_str.split('.').collect();
        let mut current = &config_json;

        for part in &parts {
            match current {
                Value::Object(map) => {
                    current = map.get(*part).ok_or_else(|| {
                        anyhow::anyhow!("配置路径不存在: {} (部分: {})", path_str, part)
                    })?;
                }
                Value::Array(arr) => {
                    let index = part.parse::<usize>().map_err(|_| {
                        anyhow::anyhow!("无效的数组索引: {}", part)
                    })?;
                    current = arr.get(index).ok_or_else(|| {
                        anyhow::anyhow!("数组索引超出范围: {}", part)
                    })?;
                }
                _ => {
                    anyhow::bail!("无法访问路径部分: {}", part);
                }
            }
        }

        println!("{} =", path_str);
        println!("{}", serde_json::to_string_pretty(current)?);
    } else {
        // 显示完整配置（隐藏敏感信息）
        println!();

        let mut masked_config = config_json.clone();
        mask_sensitive_fields(&mut masked_config);

        if full {
            println!("{}", serde_json::to_string_pretty(&config_json)?);
        } else {
            // 按类别显示
            display_config_summary(&masked_config);
        }
    }

    Ok(())
}

fn mask_sensitive_fields(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                if is_sensitive_field(key) {
                    *val = mask_value(val);
                } else {
                    mask_sensitive_fields(val);
                }
            }
        }
        Value::Array(arr) => {
            for val in arr.iter_mut() {
                mask_sensitive_fields(val);
            }
        }
        _ => {}
    }
}

fn is_sensitive_field(key: &str) -> bool {
    matches!(key.to_lowercase().as_str(),
        "apikey" | "api_key" | "token" | "password" | "secret"
    )
}

fn mask_value(value: &Value) -> Value {
    match value {
        Value::String(s) if s.len() > 8 => {
            let visible = s.chars().take(8).collect::<String>();
            Value::String(format!("{}***", visible))
        }
        _ => Value::String("***".to_string()),
    }
}

fn display_config_summary(config: &Value) {
    if let Some(obj) = config.as_object() {
        // Agents 配置
        if let Some(agents) = obj.get("agents") {
            println!("🤖 Agent 配置:");
            if let Some(defaults) = agents.get("defaults") {
                if let Some(model) = defaults.get("model") {
                    println!("  模型: {}", model.as_str().unwrap_or("未设置"));
                }
                if let Some(workspace) = defaults.get("workspace") {
                    println!("  工作区: {}", workspace.as_str().unwrap_or("未设置"));
                }
            }
        }

        // 渠道配置
        if let Some(channels) = obj.get("channels") {
            println!("\n📡 渠道配置:");

            if let Some(tg) = channels.get("telegram") {
                let enabled = tg.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
                println!("  Telegram: {}", if enabled { "✓ 启用" } else { "✗ 禁用" });
            }

            if let Some(wa) = channels.get("whatsapp") {
                let enabled = wa.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
                println!("  WhatsApp: {}", if enabled { "✓ 启用" } else { "✗ 禁用" });
            }

            if let Some(dc) = channels.get("discord") {
                let enabled = dc.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
                println!("  Discord: {}", if enabled { "✓ 启用" } else { "✗ 禁用" });
            }
        }

        // 提供商配置
        if let Some(providers) = obj.get("providers") {
            println!("\n🔌 提供商配置:");

            for (name, provider) in providers.as_object().unwrap_or(&serde_json::Map::default()) {
                let has_key = provider.get("apiKey")
                    .and_then(|v| v.as_str())
                    .map(|s| !s.is_empty())
                    .unwrap_or(false);

                let status = if has_key { "✓ 已配置" } else { "✗ 未配置" };
                println!("  {}: {}", capitalize(name), status);
            }
        }

        // 网关配置
        if let Some(gateway) = obj.get("gateway") {
            println!("\n🌐 网关配置:");

            if let Some(port) = gateway.get("port") {
                println!("  端口: {}", port.as_u64().unwrap_or(18789));
            }
        }
    }

    println!("\n提示: 使用 --full 显示完整配置");
    println!("提示: 使用 --path <key> 显示特定配置项");
}

fn capitalize(s: &str) -> String {
    s.chars()
        .enumerate()
        .map(|(i, c)| if i == 0 { c.to_ascii_uppercase() } else { c })
        .collect()
}
