//! 设置配置项

use stormclaw_config::{load_config, save_config, get_config_path};
use serde_json::Value;

pub async fn run(path: String, value: String) -> anyhow::Result<()> {
    let config_path = get_config_path();

    if !config_path.exists() {
        println!("❌ 配置文件不存在: {}", config_path.display());
        println!("\n提示: 使用 `stormclaw onboard` 初始化配置");
        return Ok(());
    }

    let mut config = load_config()?;
    let mut config_json = serde_json::to_value(&config)?;

    // 解析路径并设置值
    let parts: Vec<&str> = path.split('.').collect();
    let value_json = parse_value(&value)?;

    let current = &mut config_json;
    set_nested_value(current, &parts, value_json)?;

    // 转换回配置
    config = serde_json::from_value(config_json)?;

    // 保存配置
    save_config(&config)?;

    println!("✅ 配置已更新: {} = {}", path, value);

    Ok(())
}

fn parse_value(s: &str) -> anyhow::Result<Value> {
    // 尝试解析为 JSON
    if let Ok(json) = serde_json::from_str::<Value>(s) {
        return Ok(json);
    }

    // 尝试解析为布尔值
    match s.to_lowercase().as_str() {
        "true" => return Ok(Value::Bool(true)),
        "false" => return Ok(Value::Bool(false)),
        _ => {}
    }

    // 尝试解析为数字
    if let Ok(n) = s.parse::<i64>() {
        return Ok(Value::Number(n.into()));
    }

    if let Ok(n) = s.parse::<f64>() {
        return Ok(Value::Number(serde_json::Number::from_f64(n).unwrap()));
    }

    // 默认作为字符串
    Ok(Value::String(s.to_string()))
}

fn set_nested_value(current: &mut Value, parts: &[&str], value: Value) -> anyhow::Result<()> {
    if parts.len() == 1 {
        match current {
            Value::Object(map) => {
                map.insert(parts[0].to_string(), value);
                Ok(())
            }
            _ => anyhow::bail!("当前值不是对象，无法设置属性"),
        }
    } else {
        match current {
            Value::Object(map) => {
                let key = parts[0].to_string();
                let next = map.entry(key.clone()).or_insert_with(|| Value::Object(serde_json::Map::new()));
                set_nested_value(next, &parts[1..], value)
            }
            _ => anyhow::bail!("路径 {} 不存在", parts[0]),
        }
    }
}
