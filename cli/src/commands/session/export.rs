//! 导出会话

use stormclaw_utils::data_dir;
use std::path::PathBuf;

pub async fn run(id: String, output: Option<PathBuf>, format: String) -> anyhow::Result<()> {
    let sessions_dir = data_dir().join("sessions");

    let sessions_to_export = if id == "all" {
        // 导出所有会话
        let mut sessions = Vec::new();
        let mut entries = tokio::fs::read_dir(&sessions_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                continue;
            }
            sessions.push(path);
        }

        if sessions.is_empty() {
            println!("📋 没有会话");
            return Ok(());
        }

        println!("导出 {} 个会话", sessions.len());
        sessions
    } else {
        // 导出指定会话
        let session_path = sessions_dir.join(format!("{}.jsonl", id));

        if !session_path.exists() {
            println!("❌ 会话不存在: {}", id);
            return Ok(());
        }

        vec![session_path]
    };

    // 确定输出文件
    let output_path = output.unwrap_or_else(|| {
        if id == "all" {
            PathBuf::from(format!("sessions_export_{}.json",
                chrono::Utc::now().format("%Y%m%d_%H%M%S")))
        } else {
            PathBuf::from(format!("session_{}.{}", id, format))
        }
    });

    // 读取会话内容
    let mut all_sessions = Vec::new();

    for session_path in sessions_to_export {
        let content = tokio::fs::read_to_string(&session_path).await?;
        let session_id = session_path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");

        match format.as_str() {
            "json" => {
                let messages: Vec<serde_json::Value> = content
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .filter_map(|line| serde_json::from_str(line).ok())
                    .collect();

                all_sessions.push(serde_json::json!({
                    "id": session_id,
                    "messages": messages,
                }));
            }
            "markdown" => {
                let md = convert_to_markdown(&content, session_id);
                all_sessions.push(serde_json::json!({"content": md}));
            }
            "txt" => {
                let txt = convert_to_text(&content, session_id);
                all_sessions.push(serde_json::json!({"content": txt}));
            }
            _ => {
                anyhow::bail!("未知的格式: {}", format);
            }
        }
    }

    // 写入输出文件
    let output_content = match format.as_str() {
        "json" => serde_json::to_string_pretty(&serde_json::Value::Array(all_sessions))?,
        "markdown" | "txt" => {
            all_sessions.iter()
                .filter_map(|v| v.get("content").and_then(|c| c.as_str()))
                .collect::<Vec<_>>()
                .join("\n\n---\n\n")
        }
        _ => unreachable!(),
    };

    tokio::fs::write(&output_path, output_content).await?;

    println!("✅ 已导出到: {}", output_path.display());

    Ok(())
}

fn convert_to_markdown(content: &str, session_id: &str) -> String {
    let mut lines = vec![format!("# 会话: {}", session_id)];
    let mut in_code_block = false;

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(t) = value.get("_type").and_then(|v| v.as_str()) {
                if t == "metadata" {
                    continue;
                }
            }

            let role = value.get("role").and_then(|v| v.as_str()).unwrap_or("unknown");
            let content = value.get("content").and_then(|v| v.as_str()).unwrap_or("");

            let role_name = match role {
                "user" => "用户",
                "assistant" => "助手",
                "system" => "系统",
                _ => role,
            };

            lines.push(format!("\n## {}\n", role_name));
            lines.push(content.to_string());
        }
    }

    lines.join("\n")
}

fn convert_to_text(content: &str, session_id: &str) -> String {
    let mut lines = vec![format!("会话: {}", session_id)];
    lines.push("=".repeat(40));

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(t) = value.get("_type").and_then(|v| v.as_str()) {
                if t == "metadata" {
                    continue;
                }
            }

            let role = value.get("role").and_then(|v| v.as_str()).unwrap_or("unknown");
            let content = value.get("content").and_then(|v| v.as_str()).unwrap_or("");

            lines.push(format!("[{}]", role));
            lines.push(content.to_string());
            lines.push("-".repeat(20).to_string());
        }
    }

    lines.join("\n")
}
