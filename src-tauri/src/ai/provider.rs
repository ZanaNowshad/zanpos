use serde_json::Value;
use sqlx::SqlitePool;
use crate::ai::client::{
    AnthropicClient, AnthropicMessage, ContentBlock, ToolDef,
    extract_text, extract_tool_use,
};
use crate::ai::openai_client::{
    OpenAIClient, OpenAIMessage,
    user_msg, assistant_msg, assistant_tool_call_msg, tool_result_msg,
};
use crate::db::repositories::ai_admin_repo;
use crate::domain::ai_admin::ChatMessage;
use crate::errors::AppResult;

// ── Unified result types ───────────────────────────────────────────────────────

pub struct ChatResult {
    pub text: String,
    pub tool_call: Option<ToolCallResult>,
}

pub struct ToolCallResult {
    pub id: String,
    pub name: String,
    pub input: Value,
}

// ── Provider enum ──────────────────────────────────────────────────────────────

pub enum Provider {
    Anthropic(AnthropicClient),
    OpenAI(OpenAIClient),
}

impl Provider {
    /// Load from app_config. Returns None if no provider is configured.
    pub async fn from_db(pool: &SqlitePool) -> AppResult<Option<Self>> {
        let kind = ai_admin_repo::get_config(pool, "ai_provider").await?
            .unwrap_or_default();

        match kind.as_str() {
            "anthropic" => {
                let key = ai_admin_repo::get_config(pool, "anthropic_api_key").await?
                    .unwrap_or_default();
                if key.is_empty() { return Ok(None); }
                Ok(Some(Provider::Anthropic(AnthropicClient::new(key))))
            }
            "openai" => {
                let key = ai_admin_repo::get_config(pool, "openai_api_key").await?
                    .unwrap_or_default();
                let base_url = ai_admin_repo::get_config(pool, "openai_base_url").await?
                    .unwrap_or_default();
                let model = ai_admin_repo::get_config(pool, "openai_model").await?
                    .unwrap_or_default();
                if key.is_empty() || base_url.is_empty() || model.is_empty() { return Ok(None); }
                Ok(Some(Provider::OpenAI(OpenAIClient::new(base_url, key, model))))
            }
            _ => Ok(None),
        }
    }

    /// Send a chat turn. Returns unified ChatResult.
    pub async fn send_chat(
        &self,
        system: &str,
        history: &[ChatMessage],
        user_message: &str,
        tools: &[ToolDef],
    ) -> AppResult<ChatResult> {
        match self {
            Provider::Anthropic(c) => {
                let mut msgs: Vec<AnthropicMessage> = history.iter().map(|m| {
                    if m.role == "user" { AnthropicMessage::user_text(&m.content) }
                    else { AnthropicMessage::assistant_text(&m.content) }
                }).collect();
                msgs.push(AnthropicMessage::user_text(user_message));

                let resp = c.send(system, msgs, tools.to_vec()).await?;
                if let Some((id, name, input)) = extract_tool_use(&resp.content) {
                    return Ok(ChatResult {
                        text: extract_text(&resp.content),
                        tool_call: Some(ToolCallResult { id, name, input }),
                    });
                }
                Ok(ChatResult { text: extract_text(&resp.content), tool_call: None })
            }

            Provider::OpenAI(c) => {
                let mut msgs: Vec<OpenAIMessage> = history.iter().map(|m| {
                    if m.role == "user" { user_msg(&m.content) }
                    else { assistant_msg(&m.content) }
                }).collect();
                msgs.push(user_msg(user_message));

                let resp = c.send(system, msgs, tools).await?;
                if let Some(tc) = resp.tool_call {
                    return Ok(ChatResult {
                        text: resp.text,
                        tool_call: Some(ToolCallResult { id: tc.id, name: tc.name, input: tc.input }),
                    });
                }
                Ok(ChatResult { text: resp.text, tool_call: None })
            }
        }
    }

    /// Continue after executing a read-only tool — append tool result and get follow-up.
    pub async fn continue_with_tool_result(
        &self,
        system: &str,
        history: &[ChatMessage],
        user_message: &str,
        tool_call: &ToolCallResult,
        tool_result: String,
        _tools: &[ToolDef],
    ) -> AppResult<String> {
        match self {
            Provider::Anthropic(c) => {
                let mut msgs: Vec<AnthropicMessage> = history.iter().map(|m| {
                    if m.role == "user" { AnthropicMessage::user_text(&m.content) }
                    else { AnthropicMessage::assistant_text(&m.content) }
                }).collect();
                msgs.push(AnthropicMessage::user_text(user_message));
                // assistant message that contains the tool_use block
                msgs.push(AnthropicMessage {
                    role: "assistant".into(),
                    content: vec![ContentBlock::ToolUse {
                        id: tool_call.id.clone(),
                        name: tool_call.name.clone(),
                        input: tool_call.input.clone(),
                    }],
                });
                // tool result
                msgs.push(AnthropicMessage {
                    role: "user".into(),
                    content: vec![ContentBlock::ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: tool_result,
                    }],
                });
                let resp = c.send(system, msgs, vec![]).await?;
                Ok(extract_text(&resp.content))
            }

            Provider::OpenAI(c) => {
                let mut msgs: Vec<OpenAIMessage> = history.iter().map(|m| {
                    if m.role == "user" { user_msg(&m.content) }
                    else { assistant_msg(&m.content) }
                }).collect();
                msgs.push(user_msg(user_message));
                // The assistant message that made the tool call
                msgs.push(assistant_tool_call_msg(
                    if tool_call.name.is_empty() { None } else { Some(String::new()) },
                    &crate::ai::openai_client::OpenAIToolCallResult {
                        id: tool_call.id.clone(),
                        name: tool_call.name.clone(),
                        input: tool_call.input.clone(),
                    },
                ));
                // Tool result message
                msgs.push(tool_result_msg(&tool_call.id, tool_result));
                let resp = c.send(system, msgs, &[]).await?;
                Ok(resp.text)
            }
        }
    }

    /// Get a follow-up after a mutation was executed (no tools needed).
    pub async fn get_followup(
        &self,
        system: &str,
        history: &[ChatMessage],
        assistant_text: &str,
        tool_name: &str,
        description: &str,
    ) -> AppResult<String> {
        let notification = format!(
            "[System: the mutation '{}' was confirmed by the admin and executed successfully. {}]",
            tool_name, description
        );
        match self {
            Provider::Anthropic(c) => {
                let mut msgs: Vec<AnthropicMessage> = history.iter().map(|m| {
                    if m.role == "user" { AnthropicMessage::user_text(&m.content) }
                    else { AnthropicMessage::assistant_text(&m.content) }
                }).collect();
                if !assistant_text.is_empty() {
                    msgs.push(AnthropicMessage::assistant_text(assistant_text));
                }
                msgs.push(AnthropicMessage::user_text(&notification));
                let resp = c.send(system, msgs, vec![]).await?;
                Ok(extract_text(&resp.content))
            }
            Provider::OpenAI(c) => {
                let mut msgs: Vec<OpenAIMessage> = history.iter().map(|m| {
                    if m.role == "user" { user_msg(&m.content) }
                    else { assistant_msg(&m.content) }
                }).collect();
                if !assistant_text.is_empty() {
                    msgs.push(assistant_msg(assistant_text));
                }
                msgs.push(user_msg(&notification));
                let resp = c.send(system, msgs, &[]).await?;
                Ok(resp.text)
            }
        }
    }
}
