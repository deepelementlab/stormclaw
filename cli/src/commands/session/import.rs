//! 导入会话

use stormclaw_utils::data_dir;
use std::path::PathBuf;

pub async fn run(file: PathBuf, overwrite: bool) -> anyhow::Result<()> {
    if !file.exists() {
        println!("❌ 文件不存在: {}", file.display());
        return Ok(());
    }

    let sessions_dir = data_dir().join("sessions");
    tokio::fs::create_dir_all(&sessions_dir).await?;

    let content = tokio::fs::read_to_string(&file).await?;

    // 尝试解析为 JSON
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
        if let Some(arr) = value.as_array() {
            // 多个会话
            println!("导入 {} 个会话", arr.len());

            for session in arr {
                if let Err(e) = import_session(session.clone(), &sessions_dir, overwrite).await {
                    println!("⚠️  导入会话失败: {}", e);
                }
            }
        } else {
            // 单个会话
            import_session(value, &sessions_dir, overwrite).await?;
        }
    } else {
        anyhow::bail!("无法解析文件，确保是有效的 JSON 格式");
    }

    Ok(())
}

async fn import_session(
    session: serde_json::Value,
    sessions_dir: &PathBuf,
    overwrite: bool,
) -> anyhow::Result<()> {
    let session_id = session.get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("缺少会话 ID"))?;

    let messages = session.get("messages")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("缺少会话消息"))?;

    let session_path = sessions_dir.join(format!("{}.jsonl", session_id));

    if session_path.exists() && !overwrite {
        println!("⚠️  会话已存在: {} (使用 --overwrite 覆盖)", session_id);
        return Ok(());
    }

    // 写入会话
    let mut lines = Vec::new();

    // 添加元数据
    let metadata = serde_json::json!({
        "_type": "metadata",
        "created_at": chrono::Utc::now().to_rfc3339(),
        "updated_at": chrono::Utc::now().to_rfc3339(),
        "metadata": {
            "imported": "true"
        }
    });
    lines.push(metadata.to_string());

    // 添加消息
    for msg in messages {
        lines.push(serde_json::to_string(msg)?);
    }

    tokio::fs::write(&session_path, lines.join("\n")).await?;

    println!("✅ 已导入会话: {}", session_id);

    Ok(())
}
