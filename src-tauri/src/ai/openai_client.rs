use serde::{Deserialize, Serialize};
use crate::ai::client::ToolDef;
use crate::errors::{AppError, AppResult};

// ── Request types ──────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct OpenAIChatRequest<'a> {
    model: &'a str,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OpenAITool<'a>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum OpenAIMessage {
    System {
        content: String,
    },
    User {
        content: String,
    },
    Assistant {
        content: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<OpenAIToolCall>,
    },
    Tool {
        content: String,
        tool_call_id: String,
    },
}

#[derive(Serialize)]
struct OpenAITool<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    function: OpenAIFunction<'a>,
}

#[derive(Serialize)]
struct OpenAIFunction<'a> {
    name: &'a str,
    description: &'a str,
    parameters: &'a serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OpenAIToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: OpenAIToolCallFunction,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OpenAIToolCallFunction {
    pub name: String,
    pub arguments: String,
}

// ── Response types ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct OpenAIChatResponse {
    choices: Vec<OpenAIChoice>,
}

#[derive(Deserialize)]
struct OpenAIChoice {
    message: OpenAIResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenAIResponseMessage {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<OpenAIToolCall>,
}

// ── Models list ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct OpenAIModelsResponse {
    data: Vec<OpenAIModelEntry>,
}

#[derive(Deserialize)]
struct OpenAIModelEntry {
    id: String,
}

// ── Client ─────────────────────────────────────────────────────────────────────

pub struct OpenAIClient {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    http: reqwest::Client,
}

impl OpenAIClient {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>, model: impl Into<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        Self { base_url, api_key: api_key.into(), model: model.into(), http: reqwest::Client::new() }
    }

    fn chat_url(&self) -> String { format!("{}/chat/completions", self.base_url) }
    fn models_url(&self) -> String { format!("{}/models", self.base_url) }

    pub async fn list_models(&self) -> AppResult<Vec<String>> {
        let resp = self.http
            .get(self.models_url())
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Models request failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::Internal(format!("Models error {}: {}", status, body)));
        }

        let parsed: OpenAIModelsResponse = resp.json().await
            .map_err(|e| AppError::Internal(format!("Failed to parse models: {}", e)))?;

        let mut ids: Vec<String> = parsed.data.into_iter().map(|m| m.id).collect();
        ids.sort();
        Ok(ids)
    }

    pub async fn send(
        &self,
        system: &str,
        messages: Vec<OpenAIMessage>,
        tools: &[ToolDef],
    ) -> AppResult<OpenAIChatResult> {
        let oai_tools: Vec<OpenAITool<'_>> = tools.iter().map(|t| OpenAITool {
            kind: "function",
            function: OpenAIFunction {
                name: &t.name,
                description: &t.description,
                parameters: &t.input_schema,
            },
        }).collect();

        let mut all_messages = vec![OpenAIMessage::System { content: system.to_string() }];
        all_messages.extend(messages);

        let req = OpenAIChatRequest {
            model: &self.model,
            messages: all_messages,
            tools: oai_tools,
        };

        let resp = self.http
            .post(self.chat_url())
            .bearer_auth(&self.api_key)
            .json(&req)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Chat request failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::Internal(format!("Chat error {}: {}", status, body)));
        }

        let parsed: OpenAIChatResponse = resp.json().await
            .map_err(|e| AppError::Internal(format!("Failed to parse chat response: {}", e)))?;

        let choice = parsed.choices.into_iter().next()
            .ok_or_else(|| AppError::Internal("Empty choices from API".into()))?;

        let finish_reason = choice.finish_reason.unwrap_or_default();
        let msg = choice.message;

        if finish_reason == "tool_calls" || !msg.tool_calls.is_empty() {
            let tc = msg.tool_calls.into_iter().next()
                .ok_or_else(|| AppError::Internal("tool_calls finish_reason but no tool calls".into()))?;
            let input: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                .unwrap_or(serde_json::Value::Object(Default::default()));
            return Ok(OpenAIChatResult {
                text: msg.content.unwrap_or_default(),
                tool_call: Some(OpenAIToolCallResult {
                    id: tc.id,
                    name: tc.function.name,
                    input,
                }),
                finish_reason,
            });
        }

        Ok(OpenAIChatResult {
            text: msg.content.unwrap_or_default(),
            tool_call: None,
            finish_reason,
        })
    }
}

pub struct OpenAIChatResult {
    pub text: String,
    pub tool_call: Option<OpenAIToolCallResult>,
    /// Deserialized from API response; retained for future logging/retry logic.
    #[allow(dead_code)]
    pub finish_reason: String,
}

pub struct OpenAIToolCallResult {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

// ── Message builders ───────────────────────────────────────────────────────────

pub fn user_msg(text: impl Into<String>) -> OpenAIMessage {
    OpenAIMessage::User { content: text.into() }
}

pub fn assistant_msg(text: impl Into<String>) -> OpenAIMessage {
    OpenAIMessage::Assistant { content: Some(text.into()), tool_calls: vec![] }
}

pub fn assistant_tool_call_msg(text: Option<String>, tc: &OpenAIToolCallResult) -> OpenAIMessage {
    OpenAIMessage::Assistant {
        content: text,
        tool_calls: vec![OpenAIToolCall {
            id: tc.id.clone(),
            kind: "function".into(),
            function: OpenAIToolCallFunction {
                name: tc.name.clone(),
                arguments: tc.input.to_string(),
            },
        }],
    }
}

pub fn tool_result_msg(tool_call_id: impl Into<String>, content: impl Into<String>) -> OpenAIMessage {
    OpenAIMessage::Tool { content: content.into(), tool_call_id: tool_call_id.into() }
}
