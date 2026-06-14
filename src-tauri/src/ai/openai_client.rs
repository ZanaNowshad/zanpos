use crate::ai::client::ToolDef;
use crate::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};

// ── Request types ──────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct OpenAIChatRequest<'a> {
    model: &'a str,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning_content: Option<String>,
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
    /// DeepSeek R1 and similar reasoning models return chain-of-thought here.
    /// Must be passed back in the assistant message on subsequent turns or the
    /// API rejects the request with a 400 error.
    #[serde(default)]
    reasoning_content: Option<String>,
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
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        // R-03: bound request lifecycle to prevent indefinite hangs.
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_default();
        Self {
            base_url,
            api_key: api_key.into(),
            model: model.into(),
            http,
        }
    }

    fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }
    fn models_url(&self) -> String {
        format!("{}/models", self.base_url)
    }

    pub async fn list_models(&self) -> AppResult<Vec<String>> {
        let resp = self
            .http
            .get(self.models_url())
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Models request failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::Internal(format!(
                "Models error {}: {}",
                status, body
            )));
        }

        let parsed: OpenAIModelsResponse = resp
            .json()
            .await
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
        let oai_tools: Vec<OpenAITool<'_>> = tools
            .iter()
            .map(|t| OpenAITool {
                kind: "function",
                function: OpenAIFunction {
                    name: &t.name,
                    description: &t.description,
                    parameters: &t.input_schema,
                },
            })
            .collect();

        let mut all_messages = vec![OpenAIMessage::System {
            content: system.to_string(),
        }];
        all_messages.extend(messages);

        let req = OpenAIChatRequest {
            model: &self.model,
            messages: all_messages,
            temperature: Some(0.0),
            tools: oai_tools,
        };

        let resp = self
            .http
            .post(self.chat_url())
            .bearer_auth(&self.api_key)
            .json(&req)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Chat request failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::Internal(format!(
                "Chat error {}: {}",
                status, body
            )));
        }

        let parsed: OpenAIChatResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to parse chat response: {}", e)))?;

        let choice = parsed
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| AppError::Internal("Empty choices from API".into()))?;

        let finish_reason = choice.finish_reason.unwrap_or_default();
        let msg = choice.message;

        if finish_reason == "tool_calls" || !msg.tool_calls.is_empty() {
            let tc = msg.tool_calls.into_iter().next().ok_or_else(|| {
                AppError::Internal("tool_calls finish_reason but no tool calls".into())
            })?;
            // L15: Log malformed tool-call JSON instead of silently treating it as {}
            let input: serde_json::Value = match serde_json::from_str(&tc.function.arguments) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(
                        "OpenAI tool-call '{}' has malformed args JSON: {} — args: {}",
                        tc.function.name,
                        e,
                        tc.function.arguments
                    );
                    serde_json::Value::Object(Default::default())
                }
            };
            return Ok(OpenAIChatResult {
                text: msg.content.unwrap_or_default(),
                tool_call: Some(OpenAIToolCallResult {
                    id: tc.id,
                    name: tc.function.name,
                    input,
                }),
                reasoning_content: msg.reasoning_content,
                finish_reason,
            });
        }

        // Fallback: some models (e.g. nemotron, hermes) output tool calls as XML text
        // instead of structured function call fields. Parse it here.
        let raw_text = msg.content.unwrap_or_default();
        if let Some(tc) = try_parse_xml_tool_call(&raw_text) {
            let clean_text = strip_xml_tool_call(&raw_text);
            return Ok(OpenAIChatResult {
                text: clean_text,
                tool_call: Some(tc),
                reasoning_content: msg.reasoning_content,
                finish_reason,
            });
        }

        Ok(OpenAIChatResult {
            text: raw_text,
            tool_call: None,
            reasoning_content: msg.reasoning_content,
            finish_reason,
        })
    }
}

pub struct OpenAIChatResult {
    pub text: String,
    pub tool_call: Option<OpenAIToolCallResult>,
    pub reasoning_content: Option<String>,
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
    OpenAIMessage::User {
        content: text.into(),
    }
}

pub fn assistant_msg(text: impl Into<String>) -> OpenAIMessage {
    OpenAIMessage::Assistant {
        content: Some(text.into()),
        reasoning_content: None,
        tool_calls: vec![],
    }
}

/// Reserved for OpenAI o-series reasoning models.
#[allow(dead_code)]
pub fn assistant_msg_with_reasoning(
    text: impl Into<String>,
    reasoning: Option<String>,
) -> OpenAIMessage {
    OpenAIMessage::Assistant {
        content: Some(text.into()),
        reasoning_content: reasoning,
        tool_calls: vec![],
    }
}

pub fn assistant_tool_call_msg(text: Option<String>, tc: &OpenAIToolCallResult) -> OpenAIMessage {
    OpenAIMessage::Assistant {
        content: text,
        reasoning_content: None,
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

pub fn tool_result_msg(
    tool_call_id: impl Into<String>,
    content: impl Into<String>,
) -> OpenAIMessage {
    OpenAIMessage::Tool {
        content: content.into(),
        tool_call_id: tool_call_id.into(),
    }
}

// ── XML tool-call fallback parser ─────────────────────────────────────────────
// Some open-source models (Nemotron, Hermes, Functionary) emit tool calls as
// XML text rather than structured `tool_calls` fields. We detect and parse them
// here so the chat loop works regardless of model capability.

fn try_parse_xml_tool_call(text: &str) -> Option<OpenAIToolCallResult> {
    let tag_start = text.find("<tool_call>")?;
    let content_start = tag_start + "<tool_call>".len();
    let tag_end = text.find("</tool_call>")?;
    if tag_end <= content_start {
        return None;
    }
    let inner = &text[content_start..tag_end];

    // Extract function name: <function=NAME>
    let func_marker = "<function=";
    let fs = inner.find(func_marker)? + func_marker.len();
    let fe = inner[fs..].find('>')?;
    let func_name = inner[fs..fs + fe].trim().to_string();
    if func_name.is_empty() {
        return None;
    }

    // Extract parameters: <parameter=KEY> VALUE </parameter>
    let mut params = serde_json::Map::new();
    let mut pos = fs + fe + 1;
    loop {
        let pm = "<parameter=";
        let Some(rel) = inner[pos..].find(pm) else {
            break;
        };
        let pname_start = pos + rel + pm.len();
        let Some(pname_end_rel) = inner[pname_start..].find('>') else {
            break;
        };
        let pname_end = pname_start + pname_end_rel;
        let param_name = inner[pname_start..pname_end].trim().to_string();

        let val_start = pname_end + 1;
        let end_tag = "</parameter>";
        let Some(val_end_rel) = inner[val_start..].find(end_tag) else {
            break;
        };
        let val_end = val_start + val_end_rel;
        let param_val = inner[val_start..val_end].trim().to_string();

        let json_val = if let Ok(n) = param_val.parse::<i64>() {
            serde_json::Value::Number(n.into())
        } else if let Ok(f) = param_val.parse::<f64>() {
            serde_json::json!(f)
        } else if param_val == "true" {
            serde_json::Value::Bool(true)
        } else if param_val == "false" {
            serde_json::Value::Bool(false)
        } else {
            serde_json::Value::String(param_val)
        };

        params.insert(param_name, json_val);
        pos = val_end + end_tag.len();
    }

    Some(OpenAIToolCallResult {
        id: format!(
            "xml_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0)
        ),
        name: func_name,
        input: serde_json::Value::Object(params),
    })
}

/// Remove the `<tool_call>…</tool_call>` block from displayed text.
pub fn strip_xml_tool_call(text: &str) -> String {
    if let (Some(start), Some(end)) = (text.find("<tool_call>"), text.find("</tool_call>")) {
        let after = end + "</tool_call>".len();
        let before = text[..start].trim_end().to_string();
        let rest = text[after..].trim_start().to_string();
        format!(
            "{}{}",
            before,
            if rest.is_empty() {
                String::new()
            } else {
                format!(" {}", rest)
            }
        )
    } else {
        text.to_string()
    }
}
