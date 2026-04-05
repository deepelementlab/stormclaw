//! 消息格式转换器
//!
//! 在不同渠道的消息格式之间进行转换

use serde::{Deserialize, Serialize};

fn escape_telegram_md_v2_plain(s: &str) -> String {
    s.chars()
        .flat_map(|c| {
            if matches!(
                c,
                '_' | '*'
                    | '['
                    | ']'
                    | '('
                    | ')'
                    | '~'
                    | '`'
                    | '>'
                    | '#'
                    | '+'
                    | '-'
                    | '='
                    | '|'
                    | '{'
                    | '}'
                    | '.'
                    | '!'
            ) {
                vec!['\\', c]
            } else {
                vec![c]
            }
        })
        .collect()
}

/// `**片段**` 转为 Telegram MarkdownV2 粗体 `*...*`，片段内外均做 V2 转义
fn md_to_telegram_md_v2(md: &str) -> String {
    let parts: Vec<&str> = md.split("**").collect();
    let mut out = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i % 2 == 0 {
            out.push_str(&escape_telegram_md_v2_plain(p));
        } else {
            out.push('*');
            out.push_str(&escape_telegram_md_v2_plain(p));
            out.push('*');
        }
    }
    out
}

/// 通用消息格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniversalMessage {
    pub id: String,
    pub channel_type: String,
    pub sender_id: String,
    pub sender_name: Option<String>,
    pub chat_id: String,
    pub chat_name: Option<String>,
    pub content: String,
    pub timestamp: i64,
    pub attachments: Vec<Attachment>,
    pub reply_to: Option<String>,
    pub metadata: serde_json::Value,
}

/// 附件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub r#type: String, // "image", "video", "audio", "file", "url"
    pub url: Option<String>,
    pub name: Option<String>,
    pub mime_type: Option<String>,
    pub size: Option<u64>,
}

/// Markdown 格式转换器
pub struct MarkdownConverter;

impl MarkdownConverter {
    /// 转换为 Telegram MarkdownV2（`**粗体**` 映射为 `*粗体*`，并转义 V2 特殊字符）
    pub fn to_telegram(md: &str) -> String {
        md_to_telegram_md_v2(md)
    }

    /// Discord：对易触发格式的字符加反斜杠
    pub fn to_discord(md: &str) -> String {
        md.chars()
            .flat_map(|c| match c {
                '\\' | '*' | '_' | '~' | '`' | '|' => vec!['\\', c],
                c => vec![c],
            })
            .collect()
    }

    /// Slack mrkdwn：基础 HTML 实体转义（与部分客户端兼容）
    pub fn to_slack(md: &str) -> String {
        md.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }

    /// 从通用格式转换为渠道特定格式
    pub fn from_universal(msg: &UniversalMessage, target: &str) -> String {
        match target {
            "telegram" => Self::to_telegram(&msg.content),
            "discord" => Self::to_discord(&msg.content),
            "slack" => Self::to_slack(&msg.content),
            "whatsapp" => {
                // WhatsApp 使用简单的文本格式
                msg.content.replace("**", "*") // 粗体转换为斜体
                    .replace("__", "*") // 粗体转换为斜体
            }
            _ => msg.content.clone(),
        }
    }
}

/// 富文本消息元素
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageElement {
    Text {
        content: String,
        bold: bool,
        italic: bool,
        code: bool,
    },
    Mention {
        user_id: String,
        username: Option<String>,
    },
    Hashtag {
        tag: String,
    },
    Url {
        url: String,
        display: Option<String>,
    },
    LineBreak,
    CodeBlock {
        language: Option<String>,
        code: String,
    },
}

/// 消息解析器
pub struct MessageParser;

impl MessageParser {
    /// 解析消息为元素列表
    pub fn parse(content: &str) -> Vec<MessageElement> {
        // 简化版本：按行分割
        let mut elements = Vec::new();
        let mut current_text = String::new();
        let mut in_code_block = false;
        let mut code_block_language: Option<String> = None;
        let mut code_block_content = Vec::new();

        for line in content.lines() {
            if line.trim().starts_with("```") {
                if in_code_block {
                    // 结束代码块
                    elements.push(MessageElement::CodeBlock {
                        language: code_block_language,
                        code: code_block_content.join("\n"),
                    });
                    code_block_content.clear();
                    code_block_language = None;
                    in_code_block = false;
                } else {
                    // 开始代码块
                    if !current_text.is_empty() {
                        elements.push(MessageElement::Text {
                            content: current_text.clone(),
                            bold: false,
                            italic: false,
                            code: false,
                        });
                        current_text.clear();
                    }

                    let lang = line.trim()[3..].trim().to_string();
                    code_block_language = if lang.is_empty() { None } else { Some(lang) };
                    in_code_block = true;
                }
            } else if in_code_block {
                code_block_content.push(line.to_string());
            } else {
                if !current_text.is_empty() {
                    current_text.push('\n');
                }
                current_text.push_str(line);
            }
        }

        if !current_text.is_empty() {
            elements.push(MessageElement::Text {
                content: current_text,
                bold: false,
                italic: false,
                code: false,
            });
        }

        elements
    }

    /// 从元素列表重建文本
    pub fn stringify(elements: &[MessageElement], target: &str) -> String {
        let mut result = String::new();

        for element in elements {
            match element {
                MessageElement::Text { content, bold, italic, code } => {
                    let formatted = match target {
                        "telegram" => {
                            let mut s = content.clone();
                            if *bold { s = format!("*{}*", s); }
                            if *italic { s = format!("_{}_", s); }
                            if *code { s = format!("`{}`", s); }
                            s
                        }
                        "discord" => {
                            let mut s = content.clone();
                            if *bold { s = format!("**{}**", s); }
                            if *italic { s = format!("*{}*", s); }
                            if *code { s = format!("`{}`", s); }
                            s
                        }
                        _ => content.clone(),
                    };
                    result.push_str(&formatted);
                }
                MessageElement::CodeBlock { language, code } => {
                    result.push_str("```");
                    if let Some(lang) = language {
                        result.push_str(lang);
                    }
                    result.push('\n');
                    result.push_str(code);
                    result.push_str("\n```\n");
                }
                MessageElement::LineBreak => {
                    result.push('\n');
                }
                _ => {}
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_parsing() {
        let content = "Hello **world**!\n\n```rust\nfn main() {}\n```\n\nEnd.";
        let elements = MessageParser::parse(content);
        assert!(!elements.is_empty());
    }

    #[test]
    fn test_markdown_conversion_telegram_bold() {
        let md = "pre**bold**post";
        let telegram = MarkdownConverter::to_telegram(md);
        assert!(telegram.contains("*bold*"));
    }

    #[test]
    fn test_markdown_conversion_discord_escapes() {
        let d = MarkdownConverter::to_discord("a*b_c");
        assert!(d.contains("\\*"));
        assert!(d.contains("\\_"));
    }

    #[test]
    fn test_markdown_conversion_slack_entities() {
        let s = MarkdownConverter::to_slack("a<b>&c");
        assert!(s.contains("&lt;"));
        assert!(s.contains("&amp;"));
    }
}
