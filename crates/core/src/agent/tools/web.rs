//! Web 工具 (搜索、抓取)

use async_trait::async_trait;
use serde_json::{json, Value};
use super::base::Tool;

/// Web 搜索工具
pub struct WebSearchTool {
    api_key: Option<String>,
}

impl WebSearchTool {
    /// 创建新的 Web 搜索工具
    pub fn new(api_key: Option<String>) -> Self {
        Self { api_key }
    }
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self::new(None)
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web for information"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query"
                },
                "count": {
                    "type": "number",
                    "description": "Number of results (default: 5)"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'query' argument"))?;

        let count = args["count"]
            .as_u64()
            .unwrap_or(5) as usize;

        // 如果有 Brave Search API key，使用 Brave Search
        // 否则返回提示信息
        if let Some(api_key) = &self.api_key {
            self.brave_search(query, count, api_key).await
        } else {
            Ok(format!(
                "Web search not configured. To enable web search, set the Brave Search API key in config.\n\nQuery: {}",
                query
            ))
        }
    }
}

impl WebSearchTool {
    async fn brave_search(&self, query: &str, count: usize, api_key: &str) -> anyhow::Result<String> {
        let client = reqwest::Client::new();

        let response = client
            .get("https://api.search.brave.com/res/v1/web/search")
            .header("Accept", "application/json")
            .header("Accept-Encoding", "gzip")
            .header("X-Subscription-Token", api_key)
            .query(&[("q", query), ("count", &count.to_string())])
            .send()
            .await?
            .error_for_status()?
            .json::<serde_json::Value>()
            .await?;

        let mut results = Vec::new();

        if let Some(web) = response["web"].as_object() {
            if let Some(results_array) = web.get("results").and_then(|r| r.as_array()) {
                for (i, item) in results_array.iter().enumerate() {
                    let title = item["title"].as_str().unwrap_or("");
                    let url = item["url"].as_str().unwrap_or("");
                    let snippet = item["description"].as_str().unwrap_or("");

                    results.push(format!(
                        "{}. {}\n   {}\n   {}",
                        i + 1,
                        title,
                        url,
                        snippet
                    ));
                }
            }
        }

        if results.is_empty() {
            Ok("No results found".to_string())
        } else {
            Ok(results.join("\n\n"))
        }
    }
}

/// Web 页面抓取工具
pub struct WebFetchTool;

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch and return the content of a web page"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL to fetch"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let url = args["url"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'url' argument"))?;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        let response = client.get(url).send().await?
            .error_for_status()?
            .text()
            .await?;

        // 简单处理：移除 HTML 标签（实际应用中应该使用更完整的 HTML 解析库）
        let text = html_to_text(&response);

        // 限制返回长度
        let max_length = 10000;
        if text.len() > max_length {
            Ok(format!("{}...\n\n[Content truncated, showing first {} characters]",
                &text[..max_length], max_length))
        } else {
            Ok(text)
        }
    }
}

/// 简单的 HTML 转文本
fn html_to_text(html: &str) -> String {
    use regex::Regex;

    // 移除 script 和 style 标签
    let re = Regex::new(r"<(script|style)[^>]*>.*?</\1>").unwrap();
    let html = re.replace_all(html, "");

    // 移除所有 HTML 标签
    let re = Regex::new(r"<[^>]+>").unwrap();
    let text = re.replace_all(&html, "");

    // 解码 HTML 实体
    let text = text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");

    // 清理多余的空白
    let re = Regex::new(r"\s+").unwrap();
    re.replace_all(&text, " ").trim().to_string()
}
