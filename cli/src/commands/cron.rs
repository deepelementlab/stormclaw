//! Cron 命令 - 管理定时任务

use clap::Subcommand;
use stormclaw_utils::data_dir as get_data_dir;
use stormclaw_services::cron::{CronService, CronSchedule, CronPayload, CronJobState, every_job};
use std::sync::Arc;
use chrono::{DateTime, Utc, TimeZone};
use comfy_table::{Table, presets::UTF8_FULL};
use uuid::Uuid;

#[derive(Subcommand)]
pub enum CronCommands {
    /// 列出定时任务
    List {
        /// 包括禁用的任务
        #[arg(short, long)]
        all: bool,
    },
    /// 添加定时任务
    Add {
        /// 任务名称
        #[arg(long)]
        name: String,
        /// 消息内容
        #[arg(long)]
        message: String,
        /// 每 N 秒运行一次
        #[arg(long)]
        every: Option<u64>,
        /// Cron 表达式
        #[arg(long)]
        cron: Option<String>,
        /// 在指定时间运行一次 (ISO 8601 格式，如 2024-01-01T12:00:00Z)
        #[arg(long)]
        at: Option<String>,
        /// 发送响应到渠道
        #[arg(short, long)]
        deliver: bool,
        /// 接收者 (格式: channel:chat_id，如 telegram:123456789)
        #[arg(short, long)]
        to: Option<String>,
    },
    /// 删除定时任务
    Remove {
        /// 任务 ID
        #[arg(required = true)]
        job_id: String,
    },
    /// 启用/禁用任务
    Enable {
        /// 任务 ID
        #[arg(required = true)]
        job_id: String,
        /// 禁用而非启用
        #[arg(short, long)]
        disable: bool,
    },
    /// 手动运行任务
    Run {
        /// 任务 ID
        #[arg(required = true)]
        job_id: String,
        /// 强制运行（即使已禁用）
        #[arg(short, long)]
        force: bool,
    },
    /// 查看任务详情
    Show {
        /// 任务 ID
        #[arg(required = true)]
        job_id: String,
    },
    /// 查看任务执行历史
    History {
        /// 任务 ID (可选，不指定则显示所有任务的历史)
        job_id: Option<String>,
        /// 显示的条目数量
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
}

pub async fn run(command: CronCommands) -> anyhow::Result<()> {
    match command {
        CronCommands::List { all } => run_list(all).await?,
        CronCommands::Add { name, message, every, cron, at, deliver, to } => {
            run_add(name, message, every, cron, at, deliver, to).await?
        }
        CronCommands::Remove { job_id } => run_remove(job_id).await?,
        CronCommands::Enable { job_id, disable } => run_enable(job_id, !disable).await?,
        CronCommands::Run { job_id, force } => run_run(job_id, force).await?,
        CronCommands::Show { job_id } => run_show(job_id).await?,
        CronCommands::History { job_id, limit } => run_history(job_id, limit).await?,
    }
    Ok(())
}

async fn run_list(all: bool) -> anyhow::Result<()> {
    let store_path = get_data_dir().join("cron").join("jobs.json");

    if !store_path.exists() {
        println!("📋 没有定时任务");
        println!("\n提示: 使用 `stormclaw cron add` 添加新任务");
        return Ok(());
    }

    let cron_service = CronService::new(store_path).await?;
    cron_service.start().await?;

    let jobs = cron_service.list_jobs(all).await;

    if jobs.is_empty() {
        println!("📋 没有定时任务");
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_header(vec!["ID", "名称", "调度", "状态", "下次运行"]);

    for job in &jobs {
        let schedule = match job.schedule.kind.as_str() {
            "every" => format!("每 {}s", job.schedule.every_ms.unwrap_or(0) / 1000),
            "cron" => job.schedule.expr.clone().unwrap_or_else(|| "(missing expr)".to_string()),
            "at" => {
                if let Some(ms) = job.schedule.at_ms {
                    format!("一次 {}", format_timestamp(ms))
                } else {
                    "一次 (未设置)".to_string()
                }
            }
            _ => "(unknown)".to_string(),
        };

        let status = if job.enabled { "✓ 启用" } else { "✗ 禁用" };
        let next_run = job.state.next_run_at_ms
            .map(format_timestamp)
            .unwrap_or_else(|| "-".to_string());

        table.add_row(vec![
            &job.id[..8.min(job.id.len())],
            &job.name,
            &schedule,
            status,
            &next_run,
        ]);
    }

    println!("\n{}", table);
    println!("\n总计: {} 个任务 ({} 个启用)",
        jobs.len(),
        jobs.iter().filter(|j| j.enabled).count()
    );

    Ok(())
}

async fn run_add(
    name: String,
    message: String,
    every: Option<u64>,
    cron: Option<String>,
    at: Option<String>,
    deliver: bool,
    to: Option<String>,
) -> anyhow::Result<()> {
    // 解析接收者
    let (channel, chat_id) = if let Some(to) = to {
        let parts: Vec<&str> = to.splitn(2, ':').collect();
        if parts.len() != 2 {
            anyhow::bail!("无效的接收者格式，应为: channel:chat_id (如 telegram:123456789)");
        }
        (Some(parts[0].to_string()), Some(parts[1].to_string()))
    } else {
        (None, None)
    };

    // 创建任务
    let job = if let Some(interval) = every {
        every_job(name, message, i64::try_from(interval).unwrap_or(i64::MAX))
    } else if let Some(expr) = cron {
        let schedule = CronSchedule {
            kind: "cron".to_string(),
            at_ms: None,
            every_ms: None,
            expr: Some(expr),
            tz: None,
        };
        stormclaw_services::cron::CronJob {
            id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
            name,
            enabled: true,
            schedule,
            payload: CronPayload {
                kind: "agent_turn".to_string(),
                message,
                deliver: false,
                channel: None,
                to: None,
            },
            state: CronJobState::default(),
            created_at_ms: chrono::Utc::now().timestamp_millis(),
            updated_at_ms: chrono::Utc::now().timestamp_millis(),
            delete_after_run: false,
        }
    } else if let Some(at_str) = at {
        // 解析时间
        let at_time = at_str.parse::<DateTime<Utc>>()
            .map_err(|_| anyhow::anyhow!("无效的时间格式，使用 ISO 8601 格式 (如 2024-01-01T12:00:00Z)"))?;

        let schedule = CronSchedule {
            kind: "at".to_string(),
            at_ms: Some(at_time.timestamp_millis()),
            every_ms: None,
            expr: None,
            tz: None,
        };
        stormclaw_services::cron::CronJob {
            id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
            name,
            enabled: true,
            schedule,
            payload: CronPayload {
                kind: "agent_turn".to_string(),
                message,
                deliver: false,
                channel: None,
                to: None,
            },
            state: CronJobState::default(),
            created_at_ms: chrono::Utc::now().timestamp_millis(),
            updated_at_ms: chrono::Utc::now().timestamp_millis(),
            delete_after_run: true,
        }
    } else {
        anyhow::bail!("必须指定 --every, --cron 或 --at");
    };

    // 更新任务配置
    let mut job = job;
    if deliver {
        job.payload.deliver = true;
        job.payload.channel = channel;
        job.payload.to = chat_id;
    }

    let store_path = get_data_dir().join("cron").join("jobs.json");
    let cron_service = CronService::new(store_path).await?;
    cron_service.start().await?;

    let added = cron_service.add_job(job).await?;

    println!("✅ 任务已添加: {} ({})", added.name, &added.id[..8.min(added.id.len())]);

    if added.payload.deliver {
        println!("   发送到: {}:{}",
            added.payload.channel.as_ref().unwrap(),
            added.payload.to.as_ref().unwrap()
        );
    }

    Ok(())
}

async fn run_remove(job_id: String) -> anyhow::Result<()> {
    let store_path = get_data_dir().join("cron").join("jobs.json");
    let cron_service = CronService::new(store_path).await?;
    cron_service.start().await?;

    let removed = cron_service.remove_job(&job_id).await?;

    if removed {
        println!("✅ 任务已删除: {}", job_id);
    } else {
        println!("❌ 任务未找到: {}", job_id);
        std::process::exit(1);
    }

    Ok(())
}

async fn run_enable(job_id: String, enabled: bool) -> anyhow::Result<()> {
    let store_path = get_data_dir().join("cron").join("jobs.json");
    let cron_service = CronService::new(store_path).await?;
    cron_service.start().await?;

    let found = cron_service.enable_job(&job_id, enabled).await?;

    if found.is_some() {
        let status = if enabled { "启用" } else { "禁用" };
        println!("✅ 任务已{}: {}", status, job_id);
    } else {
        println!("❌ 任务未找到: {}", job_id);
        std::process::exit(1);
    }

    Ok(())
}

async fn run_run(job_id: String, force: bool) -> anyhow::Result<()> {
    let store_path = get_data_dir().join("cron").join("jobs.json");
    let cron_service = CronService::new(store_path).await?;
    cron_service.start().await?;

    // 设置回调
    let callback = {
        let job_id_clone = job_id.clone();
        Arc::new(move |job: &stormclaw_services::cron::CronJob| {
            let name = job.name.clone();
            let message = job.payload.message.clone();
            let jid = job_id_clone.clone();
            let fut: std::pin::Pin<
                Box<dyn std::future::Future<Output = anyhow::Result<Option<String>>> + Send>,
            > = Box::pin(async move {
                println!("🔄 执行任务: {} ({})", name, jid);
                println!("   消息: {}", message);
                Ok(Some("任务已执行".to_string()))
            });
            fut
        })
    };

    cron_service.set_callback(callback).await;

    match cron_service.run_job(&job_id, force).await {
        Ok(true) => {
            println!("✅ 任务执行完成: {}", job_id);
        }
        Ok(false) => {
            println!("❌ 任务执行失败: {}", job_id);
        }
        Err(e) => {
            println!("❌ 错误: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}

async fn run_show(job_id: String) -> anyhow::Result<()> {
    let store_path = get_data_dir().join("cron").join("jobs.json");

    if !store_path.exists() {
        println!("❌ 任务存储文件不存在");
        std::process::exit(1);
    }

    // 读取任务
    let content: String = tokio::fs::read_to_string(&store_path).await?;
    let store: stormclaw_services::cron::CronStore = serde_json::from_str(&content)?;

    let job = store.jobs.iter()
        .find(|j| j.id.starts_with(&job_id) || j.id == job_id)
        .ok_or_else(|| anyhow::anyhow!("任务未找到: {}", job_id))?;

    println!("📋 任务详情\n");
    println!("ID:        {}", job.id);
    println!("名称:      {}", job.name);
    println!("状态:      {}", if job.enabled { "启用" } else { "禁用" });
    println!("创建时间:  {}", format_timestamp(job.created_at_ms));
    println!("更新时间:  {}", format_timestamp(job.updated_at_ms));

    println!("\n调度:");
    match job.schedule.kind.as_str() {
        "every" => {
            println!("  类型: 间隔");
            if let Some(ms) = job.schedule.every_ms {
                println!("  间隔: {} 秒", ms / 1000);
            }
        }
        "cron" => {
            println!("  类型: Cron");
            if let Some(expr) = &job.schedule.expr {
                println!("  表达式: {}", expr);
            }
            if let Some(tz) = &job.schedule.tz {
                println!("  时区: {}", tz);
            }
        }
        "at" => {
            println!("  类型: 一次性");
            if let Some(ms) = job.schedule.at_ms {
                println!("  时间: {}", format_timestamp(ms));
            }
        }
        other => println!("  类型: {}", other),
    }

    println!("\n负载:");
    println!("  类型: {}", job.payload.kind);
    println!("  消息: {}", job.payload.message);
    println!("  发送到渠道: {}", if job.payload.deliver { "是" } else { "否" });
    if job.payload.deliver {
        println!("  渠道: {:?}", job.payload.channel);
        println!("  接收者: {:?}", job.payload.to);
    }

    println!("\n状态:");
    println!(
        "  最后状态: {}",
        job.state.last_status.as_deref().unwrap_or("-")
    );
    if let Some(ms) = job.state.next_run_at_ms {
        println!("  下次运行: {}", format_timestamp(ms));
    }
    if let Some(ms) = job.state.last_run_at_ms {
        println!("  最后运行: {}", format_timestamp(ms));
    }
    if let Some(err) = &job.state.last_error {
        println!("  最后错误: {}", err);
    }

    Ok(())
}

async fn run_history(job_id: Option<String>, limit: usize) -> anyhow::Result<()> {
    println!("📋 任务执行历史\n");
    println!("[历史记录功能开发中]");
    println!("提示: 使用 `stormclaw cron show <id>` 查看任务详情");

    if let Some(id) = job_id {
        println!("\n任务 ID: {}", id);
    }

    Ok(())
}

fn format_timestamp(ms: i64) -> String {
    if let Some(dt) = Utc.timestamp_millis_opt(ms).single() {
        dt.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        format!("无效时间戳: {}", ms)
    }
}
