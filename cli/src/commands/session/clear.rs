//! 清除会话

use stormclaw_utils::data_dir;
use dialoguer::Confirm;

pub async fn run(id: String, confirm: bool) -> anyhow::Result<()> {
    if id == "all" {
        // 清除所有会话
        let sessions_dir = data_dir().join("sessions");

        if !sessions_dir.exists() {
            println!("📋 没有会话");
            return Ok(());
        }

        let mut entries = tokio::fs::read_dir(&sessions_dir).await?;
        let mut count = 0usize;
        while entries.next_entry().await?.is_some() {
            count += 1;
        }

        if count == 0 {
            println!("📋 没有会话");
            return Ok(());
        }

        println!("⚠️  即将清除所有 {} 个会话", count);

        if !confirm && !Confirm::new().with_prompt("确认清除所有会话?").interact()? {
            println!("已取消");
            return Ok(());
        }

        tokio::fs::remove_dir_all(&sessions_dir).await?;
        tokio::fs::create_dir_all(&sessions_dir).await?;

        println!("✅ 已清除所有会话");
    } else {
        // 清除指定会话
        let session_path = data_dir().join("sessions").join(format!("{}.jsonl", id));

        if !session_path.exists() {
            println!("❌ 会话不存在: {}", id);
            return Ok(());
        }

        if !confirm && !Confirm::new()
            .with_prompt(&format!("确认清除会话 {}?", id))
            .interact()?
        {
            println!("已取消");
            return Ok(());
        }

        tokio::fs::remove_file(&session_path).await?;
        println!("✅ 已清除会话: {}", id);
    }

    Ok(())
}
