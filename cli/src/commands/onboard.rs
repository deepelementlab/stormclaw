//! Onboard 命令 - 初始化配置和工作区

use clap::Args;
use dialoguer::{theme::ColorfulTheme, Confirm};
use indicatif::{ProgressBar, ProgressStyle};

/// Onboard 命令参数
#[derive(Args, Clone)]
pub struct OnboardArgs {
    /// 跳过交互式确认
    #[arg(long)]
    pub yes: bool,
}

pub async fn run(args: OnboardArgs) -> anyhow::Result<()> {
    use stormclaw_config::{get_config_path, save_config};
    use stormclaw_utils::{config_dir, default_workspace, ensure_dir};

    let config_path = get_config_path();

    println!("🐈 stormclaw 初始化向导\n");

    // 检查现有配置
    if config_path.exists() && !args.yes {
        let confirm = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("配置文件已存在，是否覆盖？")
            .default(false)
            .interact()?;

        if !confirm {
            println!("已取消");
            return Ok(());
        }
    }

    // 创建配置
    let pb = ProgressBar::new(3);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
        .unwrap());

    pb.set_message("创建配置文件...");
    let config = stormclaw_config::Config::default();
    save_config(&config)?;
    pb.inc(1);
    println!("✓ 配置文件: {}", config_path.display());

    // 创建工作区
    pb.set_message("创建工作区...");
    let workspace = default_workspace();
    ensure_dir(&workspace)?;
    pb.inc(1);
    println!("✓ 工作区: {}", workspace.display());

    // 创建模板文件
    pb.set_message("创建模板文件...");
    create_workspace_templates(&workspace)?;
    pb.inc(1);

    pb.finish_with_message("初始化完成！");

    println!("\n🐈 stormclaw 已就绪！\n");
    println!("接下来：");
    println!("  1. 添加 API Key 到配置文件:");
    println!("     获取密钥: https://openrouter.ai/keys");
    println!("     配置文件: {}", config_path.display());
    println!("  2. 开始聊天:");
    println!("     stormclaw agent -m \"你好！\"");
    println!("\n提示: 如需 Telegram/WhatsApp 支持，请查看文档");

    Ok(())
}

fn create_workspace_templates(workspace: &std::path::Path) -> anyhow::Result<()> {
    use stormclaw_utils::{write_file};

    let agents_md = r#"# Agent 指令

你是一个有用的 AI 助手。请保持简洁、准确、友好。

## 指南

- 在采取行动前先解释你在做什么
- 请求模糊时询问澄清
- 使用工具来帮助完成任务
- 将重要信息记录到你的记忆文件中
"#;

    let soul_md = r#"# 灵魂

我是 stormclaw，一个轻量级 AI 助手。

## 个性

- 乐于助人且友好
- 简洁直接
- 好奇且渴望学习

## 价值观

- 准确优于速度
- 用户隐私和安全
- 行动透明
"#;

    let user_md = r#"# 用户

用户信息写在这里。

## 偏好

- 沟通风格: (随意/正式)
- 时区: (你的时区)
- 语言: (你的首选语言)
"#;

    let tools_md = r#"# 工具说明

## 可用工具

### 文件工具
- `read_file`: 读取文件内容
- `write_file`: 写入文件
- `edit_file`: 编辑文件（替换文本）
- `list_dir`: 列出目录内容

### Shell 工具
- `exec`: 执行 shell 命令

### Web 工具
- `web_search`: 搜索网络信息
- `web_fetch`: 获取网页内容

### 通信工具
- `message`: 发送消息到聊天渠道

### 子代理工具
- `spawn`: 生成子代理处理后台任务
"#;

    write_file(&workspace.join("AGENTS.md"), agents_md)?;
    write_file(&workspace.join("SOUL.md"), soul_md)?;
    write_file(&workspace.join("USER.md"), user_md)?;
    write_file(&workspace.join("TOOLS.md"), tools_md)?;

    // 创建记忆目录
    let memory_dir = workspace.join("memory");
    std::fs::create_dir_all(&memory_dir)?;

    let memory_md = r#"# 长期记忆

此文件存储应跨会话持久保存的重要信息。

## 用户信息

（关于用户的重要事实）

## 偏好

（随时间了解到的用户偏好）

## 重要笔记

（需要记住的事情）
"#;

    write_file(&memory_dir.join("MEMORY.md"), memory_md)?;

    Ok(())
}
