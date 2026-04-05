//! 上下文构建器

use std::collections::{HashMap, HashSet};
use crate::skills::SkillsLoader;
use crate::memory::MemoryStore;

/// 上下文构建器
///
/// 负责构建系统提示词和消息列表
pub struct ContextBuilder {
    workspace: std::path::PathBuf,
    skills: SkillsLoader,
    memory: MemoryStore,
}

impl ContextBuilder {
    /// 创建新的上下文构建器
    pub fn new(workspace: std::path::PathBuf) -> Self {
        let skills = SkillsLoader::new(workspace.clone(), None);
        let memory = MemoryStore::new(workspace.clone());
        Self { workspace, skills, memory }
    }

    /// 构建系统提示词
    pub fn build_system_prompt(&self, skill_names: Option<Vec<String>>) -> String {
        let mut parts = Vec::new();

        // 核心身份
        parts.push(self.get_identity());

        // 引导文件
        let bootstrap = self.load_bootstrap_files();
        if !bootstrap.is_empty() {
            parts.push(bootstrap);
        }

        // 技能
        let skills_summary = self.build_skills_summary(skill_names);
        if !skills_summary.is_empty() {
            parts.push(format!(
                "# Skills\n\nThe following skills extend your capabilities. To use a skill, read its SKILL.md file using the read_file tool.\n\n{}",
                skills_summary
            ));
        }

        // 记忆
        if let Ok(mem) = self.memory.get_memory_context() {
            if !mem.trim().is_empty() {
                parts.push(format!("# Memory\n\n{}", mem));
            }
        }

        parts.join("\n\n---\n\n")
    }

    /// 构建消息列表
    pub fn build_messages(
        &self,
        history: Vec<HashMap<String, String>>,
        current_message: &str,
        skill_names: Option<Vec<String>>,
    ) -> Vec<HashMap<String, String>> {
        let mut messages = Vec::new();

        // 系统提示词
        messages.push({
            let mut m = HashMap::new();
            m.insert("role".to_string(), "system".to_string());
            m.insert(
                "content".to_string(),
                self.build_system_prompt(skill_names.clone()),
            );
            m
        });

        // 历史消息
        for msg in history {
            messages.push(msg);
        }

        // 当前消息
        messages.push({
            let mut m = HashMap::new();
            m.insert("role".to_string(), "user".to_string());
            m.insert("content".to_string(), current_message.to_string());
            m
        });

        messages
    }

    /// 添加工具结果到消息列表
    pub fn add_tool_result(
        &self,
        messages: &mut Vec<HashMap<String, String>>,
        tool_call_id: &str,
        tool_name: &str,
        result: &str,
    ) {
        let mut msg = HashMap::new();
        msg.insert("role".to_string(), "tool".to_string());
        msg.insert("content".to_string(), result.to_string());
        msg.insert("tool_call_id".to_string(), tool_call_id.to_string());
        msg.insert("name".to_string(), tool_name.to_string());
        messages.push(msg);
    }

    /// 添加助手消息到消息列表
    pub fn add_assistant_message(
        &self,
        messages: &mut Vec<HashMap<String, String>>,
        content: Option<&str>,
        tool_calls: Option<Vec<HashMap<String, String>>>,
    ) {
        let mut msg = HashMap::new();
        msg.insert("role".to_string(), "assistant".to_string());
        msg.insert("content".to_string(), content.unwrap_or("").to_string());

        if let Some(calls) = tool_calls {
            // NOTE: 目前 Provider 侧使用结构化 tool_calls；这里保留一个可读表示，避免丢失信息
            if let Ok(s) = serde_json::to_string(&calls) {
                msg.insert("tool_calls".to_string(), s);
            }
        }

        messages.push(msg);
    }

    /// 获取身份定义
    fn get_identity(&self) -> String {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M (%A)");
        let workspace_path = self.workspace.display();

        format!(
            r#"# stormclaw 🐈

You are stormclaw, a helpful AI assistant. You have access to tools that allow you to:
- Read, write, and edit files
- Execute shell commands
- Search the web and fetch web pages
- Send messages to users on chat channels
- Spawn subagents for complex background tasks

## Current Time
{}

## Workspace
Your workspace is at: {}
- Memory files: {}/memory/MEMORY.md
- Daily notes: {}/memory/YYYY-MM-DD.md
- Custom skills: {}/skills/{{skill-name}}/SKILL.md

IMPORTANT: When responding to direct questions or conversations, reply directly with your text response.
Only use the 'message' tool when you need to send a message to a specific chat channel (like WhatsApp).
For normal conversation, just respond with text - do not call the message tool.

Always be helpful, accurate, and concise. When using tools, explain what you're doing.
When remembering something, write to {}/memory/MEMORY.md"#,
            now,
            workspace_path,
            workspace_path,
            workspace_path,
            workspace_path,
            workspace_path
        )
    }

    /// 加载引导文件
    fn load_bootstrap_files(&self) -> String {
        let bootstrap_files = ["AGENTS.md", "SOUL.md", "USER.md", "TOOLS.md", "IDENTITY.md"];
        let mut parts = Vec::new();

        for filename in bootstrap_files {
            let file_path = self.workspace.join(filename);
            if file_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&file_path) {
                    parts.push(format!("## {}\n\n{}", filename, content));
                }
            }
        }

        parts.join("\n\n")
    }

    /// 构建技能摘要
    fn build_skills_summary(&self, skill_names: Option<Vec<String>>) -> String {
        match skill_names {
            None => self.skills.build_skills_summary(),
            Some(names) => {
                let set: HashSet<String> = names.into_iter().collect();
                self.skills.build_skills_summary_filtered(Some(&set))
            }
        }
    }
}
