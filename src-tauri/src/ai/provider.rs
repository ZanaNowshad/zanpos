use crate::ai::client::{
    extract_text, extract_tool_uses, AnthropicClient, AnthropicMessage, ContentBlock, ToolDef,
};
use crate::ai::config::load_ai_params;
use crate::ai::openai_client::{
    assistant_msg, assistant_tool_call_msg, tool_result_msg, user_msg, user_msg_with_image,
    OpenAIClient, OpenAIMessage,
};
use crate::db::repositories::ai_admin_repo;
use crate::domain::ai_admin::ChatMessage;
use crate::errors::AppResult;
use crate::secure_store;
use serde_json::Value;
use sqlx::SqlitePool;

fn validate_model_identifier(model: &str) -> AppResult<()> {
    if model.trim().is_empty() || model.chars().count() > 200 || model.chars().any(char::is_control)
    {
        return Err(crate::errors::AppError::Validation(
            "Invalid configured AI model name".into(),
        ));
    }
    Ok(())
}

// ── Context window guard ───────────────────────────────────────────────────────

/// Rough token estimator: 1 token ≈ 4 characters.
/// Keeps the history within a safe limit so a large conversation (e.g. "list all
/// 28k products") never overflows the model's context window.
///
/// The max_chars threshold is read from app_config (ai_context_window_chars); the
/// system prompt and new user message are always small so they are not counted.
/// If the history exceeds the limit the oldest message pairs are dropped from the
/// front until it fits.
///
/// We always drop in pairs (user + assistant) to keep Anthropic/OpenAI's
/// alternating-role requirement satisfied.
fn truncate_history(history: &[ChatMessage], max_chars: usize) -> &[ChatMessage] {
    let total: usize = history.iter().map(|m| m.content.len()).sum();
    if total <= max_chars {
        return history;
    }
    let mut start = 0;
    let mut running = total;
    while start + 2 <= history.len() {
        let removed = history[start].content.len() + history[start + 1].content.len();
        if running - removed <= max_chars {
            start += 2;
            break;
        }
        running -= removed;
        start += 2;
    }
    // If still over budget with a remaining solo message at the tail, drop it too
    if start < history.len() && running > max_chars {
        start += 1;
    }
    &history[start..]
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations");
        pool
    }

    async fn set_config(pool: &SqlitePool, key: &str, value: &str) {
        sqlx::query(
            "INSERT INTO app_config(key,value,updated_at) VALUES (?,?,datetime('now'))
             ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
        )
        .bind(key)
        .bind(value)
        .execute(pool)
        .await
        .expect("set config");
    }

    #[tokio::test]
    async fn db_plaintext_anthropic_key_does_not_activate_provider() {
        let pool = test_pool().await;
        set_config(&pool, "ai_provider", "anthropic").await;
        set_config(&pool, "anthropic_api_key", "legacy-plaintext-key").await;

        let provider = Provider::from_db(&pool).await.unwrap();

        assert!(
            provider.is_none(),
            "legacy plaintext DB API keys must not activate AI providers"
        );
    }

    #[test]
    fn configured_model_identifiers_are_bounded() {
        assert!(validate_model_identifier("claude-sonnet-5").is_ok());
        assert!(validate_model_identifier("").is_err());
        assert!(validate_model_identifier("bad\nmodel").is_err());
        assert!(validate_model_identifier(&"m".repeat(201)).is_err());
    }
}

// ── Unified result types ───────────────────────────────────────────────────────

pub struct ChatResult {
    pub text: String,
    pub tool_calls: Vec<ToolCallResult>,
    /// Reasoning/chain-of-thought content from reasoning models (DeepSeek R1, etc.).
    /// Must be passed back in assistant messages on subsequent turns.
    pub reasoning_content: Option<String>,
}

pub struct ToolCallResult {
    pub id: String,
    pub name: String,
    pub input: Value,
}

/// One completed tool round-trip stored so subsequent calls build the full
/// accumulated context instead of reconstructing it from scratch each turn.
pub struct ToolTurn {
    pub tool_call: ToolCallResult,
    pub tool_result: String,
    pub reasoning_content: Option<String>,
}

// ── Provider enum ──────────────────────────────────────────────────────────────

/// Google Gemini exposes an OpenAI-compatible REST surface, so the Gemini
/// provider reuses `OpenAIClient` pointed at this endpoint. Kept as a fixed
/// constant so users only supply an API key + model, never the URL.
pub const GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/openai";

/// Sensible default Gemini model if the user doesn't pick one.
pub const GEMINI_DEFAULT_MODEL: &str = "gemini-2.0-flash";

pub enum Provider {
    Anthropic(AnthropicClient),
    OpenAI(OpenAIClient),
    /// Google Gemini — wraps an OpenAIClient configured for Gemini's
    /// OpenAI-compatible endpoint. Behaves identically to OpenAI at the
    /// protocol level, so it shares all the OpenAI match arms below.
    Gemini(OpenAIClient),
}

impl Provider {
    /// Load from app_config. Returns None if no provider is configured.
    pub async fn from_db(pool: &SqlitePool) -> AppResult<Option<Self>> {
        let params = load_ai_params(pool).await;
        let kind = ai_admin_repo::get_config(pool, "ai_provider")
            .await?
            .unwrap_or_default();

        match kind.as_str() {
            "anthropic" => {
                let key = secure_store::get_secret("anthropic_api_key").unwrap_or_default();
                if key.is_empty() {
                    return Ok(None);
                }
                let model = ai_admin_repo::get_config(pool, "ai_anthropic_model")
                    .await?
                    .filter(|m| !m.is_empty())
                    .unwrap_or_else(|| "claude-sonnet-5".into());
                validate_model_identifier(&model)?;
                Ok(Some(Provider::Anthropic(AnthropicClient::new(
                    key,
                    model,
                    params.anthropic_max_tokens,
                    params.temperature,
                ))))
            }
            // ── OpenAI (and any OpenAI-compatible) provider ─────────────────────────
            // Preset base URLs for common providers:
            //   https://api.openai.com/v1          – OpenAI
            //   https://api.groq.com/openai/v1     – Groq
            //   https://openrouter.ai/api/v1       – OpenRouter
            //   https://api.deepseek.com/v1        – DeepSeek
            //   http://localhost:11434/v1           – Ollama (local)
            "openai" => {
                let key = secure_store::get_secret("openai_api_key").unwrap_or_default();
                let base_url = ai_admin_repo::get_config(pool, "openai_base_url")
                    .await?
                    .unwrap_or_default();
                let model = ai_admin_repo::get_config(pool, "openai_model")
                    .await?
                    .unwrap_or_default();
                if key.is_empty() || base_url.is_empty() || model.is_empty() {
                    return Ok(None);
                }
                crate::ai::openai_client::validate_provider_base_url(&base_url)?;
                if model.chars().count() > 200 || model.chars().any(char::is_control) {
                    return Err(crate::errors::AppError::Validation(
                        "Invalid OpenAI model name".into(),
                    ));
                }
                Ok(Some(Provider::OpenAI(OpenAIClient::new(
                    base_url,
                    key,
                    model,
                    params.openai_max_tokens,
                    params.temperature,
                ))))
            }
            "gemini" => {
                let key = secure_store::get_secret("gemini_api_key").unwrap_or_default();
                let model = ai_admin_repo::get_config(pool, "gemini_model")
                    .await?
                    .filter(|m| !m.is_empty())
                    .unwrap_or_else(|| GEMINI_DEFAULT_MODEL.to_string());
                validate_model_identifier(&model)?;
                if key.is_empty() {
                    return Ok(None);
                }
                Ok(Some(Provider::Gemini(OpenAIClient::new(
                    GEMINI_BASE_URL.to_string(),
                    key,
                    model,
                    params.openai_max_tokens,
                    params.temperature,
                ))))
            }
            _ => Ok(None),
        }
    }

    /// Returns true if this is the Anthropic provider.
    pub fn is_anthropic(&self) -> bool {
        matches!(self, Provider::Anthropic(_))
    }

    pub fn provider_name(&self) -> &str {
        match self {
            Provider::Anthropic(_) => "anthropic",
            Provider::OpenAI(_) => "openai",
            Provider::Gemini(_) => "gemini",
        }
    }

    pub fn model_name(&self) -> &str {
        match self {
            Provider::Anthropic(c) => c.model(),
            Provider::OpenAI(c) | Provider::Gemini(c) => &c.model,
        }
    }

    /// Returns the raw API key string for streaming use.
    pub(crate) fn api_key(&self) -> &str {
        match self {
            Provider::Anthropic(c) => c.api_key(),
            Provider::OpenAI(c) | Provider::Gemini(c) => &c.api_key,
        }
    }

    /// Send a chat turn. Returns unified ChatResult.
    pub async fn send_chat(
        &self,
        system: &str,
        history: &[ChatMessage],
        user_message: &str,
        tools: &[ToolDef],
        max_history_chars: usize,
        // Optional (base64, media_type) image attached to the user's message.
        image: Option<(&str, &str)>,
    ) -> AppResult<ChatResult> {
        // C-05: apply sliding-window guard before building messages
        let history = truncate_history(history, max_history_chars);
        match self {
            Provider::Anthropic(c) => {
                let mut msgs: Vec<AnthropicMessage> = history
                    .iter()
                    .map(|m| {
                        if m.role == "user" {
                            AnthropicMessage::user_text(&m.content)
                        } else {
                            AnthropicMessage::assistant_text(&m.content)
                        }
                    })
                    .collect();
                msgs.push(AnthropicMessage::user_text(user_message));

                let resp = c.send(system, msgs, tools.to_vec()).await?;
                let tool_calls: Vec<ToolCallResult> = extract_tool_uses(&resp.content)
                    .into_iter()
                    .map(|(id, name, input)| ToolCallResult { id, name, input })
                    .collect();
                Ok(ChatResult {
                    text: extract_text(&resp.content),
                    tool_calls,
                    reasoning_content: None,
                })
            }

            // Gemini speaks the OpenAI protocol — identical handling.
            Provider::OpenAI(c) | Provider::Gemini(c) => {
                let mut msgs: Vec<OpenAIMessage> = history
                    .iter()
                    .map(|m| {
                        if m.role == "user" {
                            user_msg(&m.content)
                        } else {
                            assistant_msg(&m.content)
                        }
                    })
                    .collect();
                // Attach the image to the final user turn (vision request) if present.
                msgs.push(match image {
                    Some((data, mt)) if !data.is_empty() => {
                        user_msg_with_image(user_message, format!("data:{mt};base64,{data}"))
                    }
                    _ => user_msg(user_message),
                });

                let resp = c.send(system, msgs, tools).await?;
                let reasoning = resp.reasoning_content.clone();
                let tool_calls: Vec<ToolCallResult> = resp
                    .tool_calls
                    .into_iter()
                    .map(|tc| ToolCallResult {
                        id: tc.id,
                        name: tc.name,
                        input: tc.input,
                    })
                    .collect();
                Ok(ChatResult {
                    text: resp.text,
                    tool_calls,
                    reasoning_content: reasoning,
                })
            }
        }
    }

    /// Continue after executing a read-only tool — thin wrapper around
    /// `continue_with_tool_turns` for single-turn callers (e.g. migration_commands.rs).
    pub async fn continue_with_tool_result(
        &self,
        system: &str,
        history: &[ChatMessage],
        user_message: &str,
        tool_call: &ToolCallResult,
        tool_result: String,
        tools: &[ToolDef],
        prev_reasoning: Option<String>,
        max_history_chars: usize,
    ) -> AppResult<ChatResult> {
        let turn = ToolTurn {
            tool_call: ToolCallResult {
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                input: tool_call.input.clone(),
            },
            tool_result,
            reasoning_content: prev_reasoning,
        };
        self.continue_with_tool_turns(
            system,
            history,
            user_message,
            &[turn],
            tools,
            max_history_chars,
        )
        .await
    }

    /// Continue after N tool calls, building the FULL accumulated context so the
    /// model sees every previous attempt and doesn't repeat the same call blindly.
    /// Uses `content: null` (not `""`) for tool-call assistant messages per the
    /// OpenAI spec — empty string confuses some models (e.g. DeepSeek flash).
    pub async fn continue_with_tool_turns(
        &self,
        system: &str,
        history: &[ChatMessage],
        user_message: &str,
        turns: &[ToolTurn],
        tools: &[ToolDef],
        max_history_chars: usize,
    ) -> AppResult<ChatResult> {
        let history = truncate_history(history, max_history_chars);
        match self {
            Provider::Anthropic(c) => {
                let mut msgs: Vec<AnthropicMessage> = history
                    .iter()
                    .map(|m| {
                        if m.role == "user" {
                            AnthropicMessage::user_text(&m.content)
                        } else {
                            AnthropicMessage::assistant_text(&m.content)
                        }
                    })
                    .collect();
                msgs.push(AnthropicMessage::user_text(user_message));
                for turn in turns {
                    msgs.push(AnthropicMessage {
                        role: "assistant".into(),
                        content: vec![ContentBlock::ToolUse {
                            id: turn.tool_call.id.clone(),
                            name: turn.tool_call.name.clone(),
                            input: turn.tool_call.input.clone(),
                        }],
                    });
                    msgs.push(AnthropicMessage {
                        role: "user".into(),
                        content: vec![ContentBlock::ToolResult {
                            tool_use_id: turn.tool_call.id.clone(),
                            content: turn.tool_result.clone(),
                        }],
                    });
                }
                let resp = c.send(system, msgs, tools.to_vec()).await?;
                let tool_calls: Vec<ToolCallResult> = extract_tool_uses(&resp.content)
                    .into_iter()
                    .map(|(id, name, input)| ToolCallResult { id, name, input })
                    .collect();
                Ok(ChatResult {
                    text: extract_text(&resp.content),
                    tool_calls,
                    reasoning_content: None,
                })
            }
            Provider::OpenAI(c) | Provider::Gemini(c) => {
                let mut msgs: Vec<OpenAIMessage> = history
                    .iter()
                    .map(|m| {
                        if m.role == "user" {
                            user_msg(&m.content)
                        } else {
                            assistant_msg(&m.content)
                        }
                    })
                    .collect();
                msgs.push(user_msg(user_message));
                for turn in turns {
                    let mut tc_msg = assistant_tool_call_msg(
                        None, // null content, not "" — correct per OpenAI spec
                        &crate::ai::openai_client::OpenAIToolCallResult {
                            id: turn.tool_call.id.clone(),
                            name: turn.tool_call.name.clone(),
                            input: turn.tool_call.input.clone(),
                            parse_error: None,
                        },
                    );
                    if let Some(ref reason) = turn.reasoning_content {
                        if let OpenAIMessage::Assistant {
                            ref mut reasoning_content,
                            ..
                        } = tc_msg
                        {
                            *reasoning_content = Some(reason.clone());
                        }
                    }
                    msgs.push(tc_msg);
                    msgs.push(tool_result_msg(
                        &turn.tool_call.id,
                        turn.tool_result.clone(),
                    ));
                }
                let resp = c.send(system, msgs, tools).await?;
                let reasoning = resp.reasoning_content.clone();
                let tool_calls: Vec<ToolCallResult> = resp
                    .tool_calls
                    .into_iter()
                    .map(|tc| ToolCallResult {
                        id: tc.id,
                        name: tc.name,
                        input: tc.input,
                    })
                    .collect();
                Ok(ChatResult {
                    text: resp.text,
                    tool_calls,
                    reasoning_content: reasoning,
                })
            }
        }
    }

    /// Load from app_config, trying providers in configured priority order.
    /// Falls back through Anthropic → OpenAI → Gemini until one is configured.
    pub async fn from_db_with_fallback(pool: &SqlitePool) -> AppResult<Option<Self>> {
        // Try primary provider first
        if let Ok(Some(p)) = Self::from_db(pool).await {
            return Ok(Some(p));
        }
        // Fallback order: try each provider kind
        for kind in &["anthropic", "openai", "gemini"] {
            // Temporarily check if this provider's key exists even if not configured as primary
            let key_name = match *kind {
                "anthropic" => "anthropic_api_key",
                "openai" => "openai_api_key",
                "gemini" => "gemini_api_key",
                _ => continue,
            };
            let has_key = !secure_store::get_secret(key_name)
                .unwrap_or_default()
                .is_empty();
            if has_key {
                let params = load_ai_params(pool).await;
                match *kind {
                    "anthropic" => {
                        let key = secure_store::get_secret("anthropic_api_key").unwrap_or_default();
                        if !key.is_empty() {
                            let model = ai_admin_repo::get_config(pool, "ai_anthropic_model")
                                .await?
                                .filter(|m| !m.is_empty())
                                .unwrap_or_else(|| "claude-sonnet-5".into());
                            validate_model_identifier(&model)?;
                            return Ok(Some(Provider::Anthropic(AnthropicClient::new(
                                key,
                                model,
                                params.anthropic_max_tokens,
                                params.temperature,
                            ))));
                        }
                    }
                    "openai" => {
                        let key = secure_store::get_secret("openai_api_key").unwrap_or_default();
                        let base_url = ai_admin_repo::get_config(pool, "openai_base_url")
                            .await?
                            .unwrap_or_default();
                        let model = ai_admin_repo::get_config(pool, "openai_model")
                            .await?
                            .unwrap_or_default();
                        if !key.is_empty() && !base_url.is_empty() && !model.is_empty() {
                            crate::ai::openai_client::validate_provider_base_url(&base_url)?;
                            validate_model_identifier(&model)?;
                            return Ok(Some(Provider::OpenAI(OpenAIClient::new(
                                base_url,
                                key,
                                model,
                                params.openai_max_tokens,
                                params.temperature,
                            ))));
                        }
                    }
                    "gemini" => {
                        let key = secure_store::get_secret("gemini_api_key").unwrap_or_default();
                        if !key.is_empty() {
                            let model = ai_admin_repo::get_config(pool, "gemini_model")
                                .await?
                                .filter(|m| !m.is_empty())
                                .unwrap_or_else(|| GEMINI_DEFAULT_MODEL.to_string());
                            validate_model_identifier(&model)?;
                            return Ok(Some(Provider::Gemini(OpenAIClient::new(
                                GEMINI_BASE_URL.to_string(),
                                key,
                                model,
                                params.openai_max_tokens,
                                params.temperature,
                            ))));
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(None)
    }

    /// Get a follow-up after a mutation was executed (no tools needed).
    pub async fn get_followup(
        &self,
        system: &str,
        history: &[ChatMessage],
        assistant_text: &str,
        tool_name: &str,
        description: &str,
        max_history_chars: usize,
    ) -> AppResult<String> {
        // C-05: apply sliding-window guard before building messages
        let history = truncate_history(history, max_history_chars);
        let notification = format!(
            "[System: the mutation '{}' was confirmed by the admin and executed successfully. {}]",
            tool_name, description
        );
        match self {
            Provider::Anthropic(c) => {
                let mut msgs: Vec<AnthropicMessage> = history
                    .iter()
                    .map(|m| {
                        if m.role == "user" {
                            AnthropicMessage::user_text(&m.content)
                        } else {
                            AnthropicMessage::assistant_text(&m.content)
                        }
                    })
                    .collect();
                if !assistant_text.is_empty() {
                    msgs.push(AnthropicMessage::assistant_text(assistant_text));
                }
                msgs.push(AnthropicMessage::user_text(&notification));
                let resp = c.send(system, msgs, vec![]).await?;
                Ok(extract_text(&resp.content))
            }
            Provider::OpenAI(c) | Provider::Gemini(c) => {
                let mut msgs: Vec<OpenAIMessage> = history
                    .iter()
                    .map(|m| {
                        if m.role == "user" {
                            user_msg(&m.content)
                        } else {
                            assistant_msg(&m.content)
                        }
                    })
                    .collect();
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
