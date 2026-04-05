//! 列出所有会话

use stormclaw_utils::data_dir;
use chrono::{DateTime, Utc};
use comfy_table::{Table, presets::UTF8_FULL};
use std::collections::HashMap;

pub async fn run(verbose: bool, sort: Option<String>, limit: Option<usize>) -> anyhow::Result<()> {
    let sessions_dir = data_dir().join("sessions");

    if !sessions_dir.exists() {
        println!("📋 没有会话");
        println!("\n提示: 使用 `stormclaw agent` 开始对话创建会话");
        return Ok(());
    }

    // 读取所有会话
    let mut sessions = Vec::new();
    let mut entries = tokio::fs::read_dir(&sessions_dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }

        let id = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        // 读取会话元数据
        if let Ok(session) = read_session_meta(&path).await {
            sessions.push((id, session));
        }
    }

    if sessions.is_empty() {
        println!("📋 没有会话");
        return Ok(());
    }

    // 排序
    match sort.as_deref() {
        Some("asc") => {
            sessions.sort_by(|a, b| a.1.updated_at.cmp(&b.1.updated_at));
        }
        Some("desc") | None => {
            sessions.sort_by(|a, b| b.1.updated_at.cmp(&a.1.updated_at));
        }
        Some(s) => {
            println!("⚠️  未知的排序方式: {}，使用默认排序", s);
            sessions.sort_by(|a, b| b.1.updated_at.cmp(&a.1.updated_at));
        }
    }

    // 应用限制
    if let Some(limit) = limit {
        sessions.truncate(limit);
    }

    // 显示
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_header(vec!["会话 ID", "创建时间", "更新时间", "消息数"]);

    for (id, session) in &sessions {
        let created = session.created_at.format("%Y-%m-%d %H:%M").to_string();
        let updated = format_time_ago(session.updated_at);
        let count = session.message_count.to_string();

        table.add_row(vec![
            if id.len() > 20 { &id[..20] } else { id },
            &created,
            &updated,
            &count,
        ]);

        if verbose {
            if let Some(metadata) = &session.metadata {
                let meta_str: String = metadata.iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>()
                    .join(", ");
                table.add_row(vec!["", "", "", &meta_str]);
            }
        }
    }

    println!("\n{}", table);
    println!("\n总计: {} 个会话", sessions.len());
    println!("提示: 使用 `stormclaw session show <id>` 查看详情");

    Ok(())
}

struct SessionMeta {
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    message_count: usize,
    metadata: Option<HashMap<String, String>>,
}

async fn read_session_meta(path: &std::path::Path) -> anyhow::Result<SessionMeta> {
    let content = tokio::fs::read_to_string(path).await?;

    let mut created_at = Utc::now();
    let mut updated_at = Utc::now();
    let mut message_count = 0;
    let mut metadata = None;

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(t) = value.get("_type").and_then(|v| v.as_str()) {
                if t == "metadata" {
                    if let Some(ca) = value.get("created_at").and_then(|v| v.as_str()) {
                        created_at = DateTime::parse_from_rfc3339(ca)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or(created_at);
                    }
                    if let Some(ua) = value.get("updated_at").and_then(|v| v.as_str()) {
                        updated_at = DateTime::parse_from_rfc3339(ua)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or(updated_at);
                    }
                    if let Some(meta) = value.get("metadata") {
                        if let Ok(map) = serde_json::from_value::<HashMap<String, String>>(meta.clone()) {
                            metadata = Some(map);
                        }
                    }
                }
            } else {
                message_count += 1;
            }
        }
    }

    Ok(SessionMeta {
        created_at,
        updated_at,
        message_count,
        metadata,
    })
}

fn format_time_ago(dt: DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(dt);

    if duration.num_days() > 0 {
        format!("{}天前", duration.num_days())
    } else if duration.num_hours() > 0 {
        format!("{}小时前", duration.num_hours())
    } else if duration.num_minutes() > 0 {
        format!("{}分钟前", duration.num_minutes())
    } else {
        "刚刚".to_string()
    }
}
