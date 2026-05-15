use serde::{Deserialize, Serialize};
use crate::errors::{AppError, AppResult};

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const MODEL: &str = "claude-sonnet-4-6";
const MAX_TOKENS: u32 = 1024;

// ── Anthropic API request/response types ──────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AnthropicRequest {
    pub model: String,
    pub max_tokens: u32,
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
        Self { role: "user".into(), content: vec![ContentBlock::Text { text: text.into() }] }
    }

    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self { role: "assistant".into(), content: vec![ContentBlock::Text { text: text.into() }] }
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
    pub stop_reason: String,
}

// ── Client ─────────────────────────────────────────────────────────────────────

pub struct AnthropicClient {
    api_key: String,
    http: reqwest::Client,
}

impl AnthropicClient {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            http: reqwest::Client::new(),
        }
    }

    pub async fn send(
        &self,
        system: &str,
        messages: Vec<AnthropicMessage>,
        tools: Vec<ToolDef>,
    ) -> AppResult<AnthropicResponse> {
        let req = AnthropicRequest {
            model: MODEL.into(),
            max_tokens: MAX_TOKENS,
            system: system.into(),
            messages,
            tools,
        };

        let resp = self.http
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
            return Err(AppError::Internal(format!("API error {}: {}", status, body)));
        }

        resp.json::<AnthropicResponse>()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to parse API response: {}", e)))
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

pub fn extract_text(blocks: &[ContentBlock]) -> String {
    blocks.iter()
        .filter_map(|b| if let ContentBlock::Text { text } = b { Some(text.as_str()) } else { None })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

pub fn extract_tool_use(blocks: &[ContentBlock]) -> Option<(String, String, serde_json::Value)> {
    blocks.iter().find_map(|b| {
        if let ContentBlock::ToolUse { id, name, input } = b {
            Some((id.clone(), name.clone(), input.clone()))
        } else {
            None
        }
    })
}
