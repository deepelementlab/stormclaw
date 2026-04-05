//! 查看会话详情

use stormclaw_utils::data_dir;
use comfy_table::{Table, presets::UTF8_FULL};

pub async fn run(id: String, show_messages: bool, count: usize) -> anyhow::Result<()> {
    let session_path = data_dir().join("sessions").join(format!("{}.jsonl", id));

    if !session_path.exists() {
        println!("❌ 会话不存在: {}", id);
        println!("\n提示: 使用 `stormclaw session list` 查看所有会话");
        return Ok(());
    }

    let content = tokio::fs::read_to_string(&session_path).await?;

    println!("📋 会话详情: {}\n", id);

    let mut created_at = None;
    let mut updated_at = None;
    let mut messages = Vec::new();

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(t) = value.get("_type").and_then(|v| v.as_str()) {
                if t == "metadata" {
                    if let Some(ca) = value.get("created_at").and_then(|v| v.as_str()) {
                        created_at = Some(ca.to_string());
                    }
                    if let Some(ua) = value.get("updated_at").and_then(|v| v.as_str()) {
                        updated_at = Some(ua.to_string());
                    }
                }
            } else {
                messages.push(value);
            }
        }
    }

    // 显示元数据
    println!("创建时间: {}", created_at.as_deref().unwrap_or("未知"));
    println!("更新时间: {}", updated_at.as_deref().unwrap_or("未知"));
    println!("消息数量: {}\n", messages.len());

    // 显示消息
    if show_messages {
        let display_count = count.min(messages.len());

        if display_count < messages.len() {
            println!("最近 {} 条消息 (共 {} 条):\n", display_count, messages.len());
        } else {
            println!("消息:\n");
        }

        let start = messages.len().saturating_sub(display_count);

        for msg in &messages[start..] {
            let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("unknown");
            let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");

            let role_icon = match role {
                "user" => "👤",
                "assistant" => "🤖",
                "system" => "⚙️",
                _ => "💬",
            };

            println!("{} {}", role_icon, role);
            println!("{}\n", content);
        }
    } else {
        println!("提示: 使用 --messages 显示消息内容");
        println!("提示: 使用 --count <n> 指定显示的消息数量");
    }

    Ok(())
}
