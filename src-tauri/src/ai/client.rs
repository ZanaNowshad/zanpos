use crate::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";

// ── Anthropic API request/response types ──────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AnthropicRequest {
    pub model: String,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    pub system: String,
    pub messages: Vec<AnthropicMessage>,
    pub tools: Vec<ToolDef>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: Vec<ContentBlock>,
}

impl AnthropicMessage {
    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }

    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct AnthropicResponse {
    pub content: Vec<ContentBlock>,
    /// Deserialized from API but used only for logging; not needed by call sites.
    #[allow(dead_code)]
    pub stop_reason: String,
}

// ── Client ─────────────────────────────────────────────────────────────────────

pub struct AnthropicClient {
    api_key: String,
    model: String,
    max_tokens: u32,
    temperature: Option<f32>,
    http: reqwest::Client,
}

impl AnthropicClient {
    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn new(
        api_key: impl Into<String>,
        model: impl Into<String>,
        max_tokens: u32,
        temperature: f32,
    ) -> Self {
        // Bound the request lifecycle without cutting off long model reasoning.
        // Connect failures remain fast; a genuinely idle request gets 30 minutes.
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(1800))
            .build()
            .unwrap_or_default();
        Self {
            api_key: api_key.into(),
            model: model.into(),
            max_tokens,
            temperature: Some(temperature),
            http,
        }
    }

    pub async fn send(
        &self,
        system: &str,
        messages: Vec<AnthropicMessage>,
        tools: Vec<ToolDef>,
    ) -> AppResult<AnthropicResponse> {
        let req = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            temperature: self.temperature,
            system: system.into(),
            messages,
            tools,
        };

        let resp = self
            .http
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&req)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("API request failed: {}", e)))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::Internal(format!(
                "API error {}: {}",
                status, body
            )));
        }

        resp.json::<AnthropicResponse>()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to parse API response: {}", e)))
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

pub fn extract_text(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|b| {
            if let ContentBlock::Text { text } = b {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

pub fn extract_tool_uses(blocks: &[ContentBlock]) -> Vec<(String, String, serde_json::Value)> {
    blocks
        .iter()
        .filter_map(|b| {
            if let ContentBlock::ToolUse { id, name, input } = b {
                Some((id.clone(), name.clone(), input.clone()))
            } else {
                None
            }
        })
        .collect()
}
