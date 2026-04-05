//! 技能系统模块

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// 技能加载器
pub struct SkillsLoader {
    workspace: PathBuf,
    workspace_skills: PathBuf,
    builtin_skills: PathBuf,
}

impl SkillsLoader {
    /// 创建新的技能加载器
    pub fn new(workspace: PathBuf, builtin_skills: Option<PathBuf>) -> Self {
        let workspace_skills = workspace.join("skills");
        let builtin_skills = builtin_skills.unwrap_or_else(|| {
            PathBuf::from("skills")
        });

        Self {
            workspace,
            workspace_skills,
            builtin_skills,
        }
    }

    /// 列出所有可用技能
    pub fn list_skills(&self, filter_unavailable: bool) -> Vec<SkillInfo> {
        let mut skills = Vec::new();

        // 工作区技能（优先级更高）
        if self.workspace_skills.exists() {
            for entry in WalkDir::new(&self.workspace_skills)
                .min_depth(1)
                .max_depth(1)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.file_type().is_dir() {
                    let skill_file = entry.path().join("SKILL.md");
                    if skill_file.exists() {
                        skills.push(SkillInfo {
                            name: entry.file_name().to_string_lossy().to_string(),
                            path: skill_file.to_string_lossy().to_string(),
                            source: "workspace".to_string(),
                        });
                    }
                }
            }
        }

        // 内置技能
        if self.builtin_skills.exists() {
            for entry in WalkDir::new(&self.builtin_skills)
                .min_depth(1)
                .max_depth(1)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.file_type().is_dir() {
                    let skill_name = entry.file_name().to_string_lossy().to_string();
                    let skill_file = entry.path().join("SKILL.md");

                    // 避免重复
                    if !skills.iter().any(|s| s.name == skill_name) && skill_file.exists() {
                        skills.push(SkillInfo {
                            name: skill_name,
                            path: skill_file.to_string_lossy().to_string(),
                            source: "builtin".to_string(),
                        });
                    }
                }
            }
        }

        if filter_unavailable {
            let known: HashSet<String> = skills.iter().map(|s| s.name.clone()).collect();
            skills.retain(|s| self.skill_deps_satisfied(&s.name, &known));
        }

        skills
    }

    /// 加载技能内容
    pub fn load_skill(&self, name: &str) -> Option<String> {
        // 检查工作区
        let workspace_skill = self.workspace_skills.join(name).join("SKILL.md");
        if workspace_skill.exists() {
            if let Ok(content) = std::fs::read_to_string(&workspace_skill) {
                return Some(self.strip_frontmatter(&content));
            }
        }

        // 检查内置
        let builtin_skill = self.builtin_skills.join(name).join("SKILL.md");
        if builtin_skill.exists() {
            if let Ok(content) = std::fs::read_to_string(&builtin_skill) {
                return Some(self.strip_frontmatter(&content));
            }
        }

        None
    }

    /// 构建技能摘要（全部技能目录）
    pub fn build_skills_summary(&self) -> String {
        self.build_skills_summary_filtered(None)
    }

    /// 构建技能摘要；`only_names` 为 `Some` 时仅包含这些名称（仍要求目录存在）。
    pub fn build_skills_summary_filtered(&self, only_names: Option<&HashSet<String>>) -> String {
        let mut skills = self.list_skills(false);
        if let Some(allow) = only_names {
            skills.retain(|s| allow.contains(&s.name));
        }

        if skills.is_empty() {
            return String::new();
        }

        let known: HashSet<String> = self
            .list_skills(false)
            .into_iter()
            .map(|s| s.name)
            .collect();

        let mut lines = vec!["<skills>".to_string()];

        for skill in skills {
            let desc = self.get_skill_description(&skill.name);
            let available = self.skill_deps_satisfied(&skill.name, &known);

            lines.push(format!(
                "  <skill available=\"{}\">",
                if available { "true" } else { "false" }
            ));
            lines.push(format!("    <name>{}</name>", html_escape(&skill.name)));
            lines.push(format!("    <description>{}</description>", html_escape(&desc)));
            lines.push(format!("    <location>{}</location>", html_escape(&skill.path)));
            lines.push("  </skill>".to_string());
        }

        lines.push("</skills>".to_string());
        lines.join("\n")
    }

    /// 获取始终加载的技能
    pub fn get_always_skills(&self) -> Vec<String> {
        let skills = self.list_skills(false);
        let known: HashSet<String> = skills.iter().map(|s| s.name.clone()).collect();
        skills
            .into_iter()
            .filter(|s| self.is_always_skill(&s.name))
            .filter(|s| self.skill_deps_satisfied(&s.name, &known))
            .map(|s| s.name)
            .collect()
    }

    /// 读取 SKILL.md 原文（含前言），供元数据解析
    fn read_skill_raw(&self, name: &str) -> Option<String> {
        let w = self.workspace_skills.join(name).join("SKILL.md");
        if w.exists() {
            return std::fs::read_to_string(&w).ok();
        }
        let b = self.builtin_skills.join(name).join("SKILL.md");
        if b.exists() {
            return std::fs::read_to_string(&b).ok();
        }
        None
    }

    fn skill_deps_satisfied(&self, name: &str, known: &HashSet<String>) -> bool {
        self.get_skill_metadata(name)
            .map(|m| m.dependencies.iter().all(|d| known.contains(d)))
            .unwrap_or(false)
    }

    fn parse_dependencies_field(&self, yaml: &str) -> Vec<String> {
        let Some(raw) = self.extract_field(yaml, "dependencies") else {
            return vec![];
        };
        let raw = raw.trim();
        if raw.starts_with('[') && raw.ends_with(']') {
            raw.trim_start_matches('[')
                .trim_end_matches(']')
                .split(',')
                .map(|s| {
                    s.trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string()
                })
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            raw.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        }
    }

    /// 获取技能元数据
    pub fn get_skill_metadata(&self, name: &str) -> Option<SkillMetadata> {
        let raw = self.read_skill_raw(name)?;

        if raw.starts_with("---") {
            if let Some(end_idx) = raw[3..].find("\n---") {
                let yaml_content = &raw[3..3 + end_idx];
                return Some(SkillMetadata {
                    name: name.to_string(),
                    description: self.extract_field(yaml_content, "description").unwrap_or_default(),
                    always: self.extract_field(yaml_content, "always").as_deref() == Some("true"),
                    dependencies: self.parse_dependencies_field(yaml_content),
                });
            }
        }

        Some(SkillMetadata {
            name: name.to_string(),
            description: name.to_string(),
            always: false,
            dependencies: vec![],
        })
    }

    /// 检查是否为始终加载的技能
    fn is_always_skill(&self, name: &str) -> bool {
        self.get_skill_metadata(name)
            .map(|m| m.always)
            .unwrap_or(false)
    }

    /// 获取技能描述
    fn get_skill_description(&self, name: &str) -> String {
        self.get_skill_metadata(name)
            .map(|m| m.description)
            .unwrap_or_else(|| name.to_string())
    }

    /// 提取 YAML 字段
    fn extract_field(&self, yaml: &str, field: &str) -> Option<String> {
        for line in yaml.lines() {
            if let Some(idx) = line.find(':') {
                let key = &line[..idx];
                let value = &line[idx + 1..];

                if key.trim() == field {
                    return Some(value.trim().trim_matches('"').to_string());
                }
            }
        }
        None
    }

    /// 移除 YAML 前言
    fn strip_frontmatter(&self, content: &str) -> String {
        if content.starts_with("---") {
            if let Some(end_idx) = content[3..].find("\n---") {
                return content[3 + end_idx + 4..].trim().to_string();
            }
        }
        content.to_string()
    }
}

/// 技能信息
#[derive(Debug, Clone)]
pub struct SkillInfo {
    pub name: String,
    pub path: String,
    pub source: String,
}

/// 技能元数据
#[derive(Debug, Clone)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    pub always: bool,
    /// 依赖的其它技能目录名（须存在于 `list_skills` 结果中）
    pub dependencies: Vec<String>,
}

/// HTML 转义
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
