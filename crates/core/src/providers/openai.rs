//! OpenAI 兼容 LLM 提供商实现

use async_trait::async_trait;
use std::sync::{Arc, RwLock};
use anyhow::Context;
use super::base::*;

#[derive(Clone)]
struct OpenAICreds {
    api_key: String,
    api_base: String,
    default_model: String,
}

/// OpenAI 兼容的 LLM 提供商
///
/// 支持 OpenAI、Anthropic（通过 API）、OpenRouter 等
#[derive(Clone)]
pub struct OpenAIProvider {
    client: Arc<reqwest::Client>,
    creds: Arc<RwLock<OpenAICreds>>,
}

impl OpenAIProvider {
    /// 创建新的 OpenAI 提供商
    pub fn new(api_key: String, api_base: Option<String>, default_model: String) -> Self {
        let client = Arc::new(
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("Failed to create HTTP client")
        );

        Self {
            client,
            creds: Arc::new(RwLock::new(OpenAICreds {
                api_key,
                api_base: api_base.unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
                default_model,
            })),
        }
    }

    /// 热更新 API 凭据与默认模型（Gateway 配置重载）
    pub fn apply_credentials(
        &self,
        api_key: String,
        api_base: Option<String>,
        default_model: String,
    ) {
        let mut c = self.creds.write().expect("OpenAI creds lock poisoned");
        c.api_key = api_key;
        c.api_base = api_base.unwrap_or_else(|| "https://api.openai.com/v1".to_string());
        c.default_model = default_model;
    }

    /// 发送 HTTP 请求到 LLM API
    async fn request_llm(
        &self,
        messages: Vec<ChatMessage>,
        tools: Option<Vec<ToolDefinition>>,
        model: &str,
        api_key: &str,
        api_base: &str,
    ) -> anyhow::Result<LLMResponse> {
        let mut request_body = serde_json::json!({
            "model": model,
            "messages": messages,
        });

        // 添加工具定义
        if let Some(tools) = tools {
            request_body["tools"] = serde_json::to_value(tools)?;
        }

        let response = self.client
            .post(format!("{}/chat/completions", api_base))
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .context("Failed to send request to LLM API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("LLM API error ({}): {}", status, error_text);
        }

        let response_json: serde_json::Value = response.json().await?;
        self.parse_response(response_json)
    }

    /// 解析 LLM API 响应
    fn parse_response(&self, response: serde_json::Value) -> anyhow::Result<LLMResponse> {
        let choice = response["choices"]
            .get(0)
            .ok_or_else(|| anyhow::anyhow!("No choices in response"))?;

        let message = &choice["message"];

        // 解析内容
        let content = message["content"]
            .as_str()
            .map(|s| s.to_string());

        // 解析工具调用
        let mut tool_calls = Vec::new();
        if let Some(calls) = message["tool_calls"].as_array() {
            for call in calls {
                if let (Some(id), Some(function)) = (
                    call["id"].as_str(),
                    call.get("function")
                ) {
                    tool_calls.push(ToolCall {
                        id: id.to_string(),
                        name: function["name"].as_str().unwrap_or("").to_string(),
                        arguments: function["arguments"].clone(),
                    });
                }
            }
        }

        // 解析使用统计
        let usage = response.get("usage").map(|u| Usage {
            prompt_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as u32,
            completion_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as u32,
            total_tokens: u["total_tokens"].as_u64().unwrap_or(0) as u32,
        });

        let finish_reason = choice["finish_reason"]
            .as_str()
            .unwrap_or("stop")
            .to_string();

        Ok(LLMResponse {
            content,
            tool_calls,
            finish_reason,
            usage,
        })
    }
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    async fn chat(
        &self,
        messages: Vec<ChatMessage>,
        tools: Option<Vec<ToolDefinition>>,
        model: Option<&str>,
    ) -> anyhow::Result<LLMResponse> {
        let (api_key, api_base, default_model) = {
            let c = self.creds.read().expect("OpenAI creds lock poisoned");
            (
                c.api_key.clone(),
                c.api_base.clone(),
                c.default_model.clone(),
            )
        };
        let model = model.unwrap_or(&default_model);
        self.request_llm(messages, tools, model, &api_key, &api_base)
            .await
    }

    fn get_default_model(&self) -> String {
        self.creds
            .read()
            .expect("OpenAI creds lock poisoned")
            .default_model
            .clone()
    }
}
