use crate::ai::client::{
    extract_text, extract_tool_use, AnthropicClient, AnthropicMessage, ContentBlock, ToolDef,
};
use crate::ai::openai_client::{
    assistant_msg, assistant_tool_call_msg, tool_result_msg, user_msg, OpenAIClient, OpenAIMessage,
};
use crate::db::repositories::ai_admin_repo;
use crate::domain::ai_admin::ChatMessage;
use crate::errors::AppResult;
use crate::secure_store;
use serde_json::Value;
use sqlx::SqlitePool;

// ── Context window guard ───────────────────────────────────────────────────────

/// Rough token estimator: 1 token ≈ 4 characters.
/// Keeps the history within a safe limit so a large conversation (e.g. "list all
/// 28k products") never overflows the model's context window.
///
/// Threshold: 80k tokens ≈ 320k chars for the history slice alone.
/// The system prompt (~2k tokens) and new user message (~0.5k) are not counted here
/// because they are always small.  If the history exceeds the limit the oldest
/// message pairs are dropped from the front until it fits.
///
/// Note: we always drop in pairs (user + assistant) to keep Anthropic/OpenAI's
/// alternating-role requirement satisfied.  A lone system/user at the very start
/// may remain if stripping it would leave an odd count — that is fine; the model
/// only requires that consecutive roles are different, not that every pair is symmetric.
const MAX_HISTORY_CHARS: usize = 320_000; // ~80k tokens

fn truncate_history(history: &[ChatMessage]) -> &[ChatMessage] {
    let total: usize = history.iter().map(|m| m.content.len()).sum();
    if total <= MAX_HISTORY_CHARS {
        return history;
    }
    // Drop oldest pairs from the front until we fit
    let mut start = 0;
    let mut running = total;
    while start + 2 <= history.len() {
        let removed = history[start].content.len() + history[start + 1].content.len();
        if running - removed <= MAX_HISTORY_CHARS {
            start += 2;
            break;
        }
        running -= removed;
        start += 2;
    }
    &history[start..]
}

// ── Unified result types ───────────────────────────────────────────────────────

pub struct ChatResult {
    pub text: String,
    pub tool_call: Option<ToolCallResult>,
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
        let kind = ai_admin_repo::get_config(pool, "ai_provider")
            .await?
            .unwrap_or_default();

        match kind.as_str() {
            "anthropic" => {
                // Prefer OS credential store; fall back to legacy plaintext SQLite.
                let key = {
                    let from_os = secure_store::get_secret("anthropic_api_key").unwrap_or_default();
                    if !from_os.is_empty() {
                        from_os
                    } else {
                        ai_admin_repo::get_config(pool, "anthropic_api_key")
                            .await?
                            .unwrap_or_default()
                    }
                };
                if key.is_empty() {
                    return Ok(None);
                }
                Ok(Some(Provider::Anthropic(AnthropicClient::new(key))))
            }
            // ── OpenAI (and any OpenAI-compatible) provider ─────────────────────────
            // Preset base URLs for common providers:
            //   https://api.openai.com/v1          – OpenAI
            //   https://api.groq.com/openai/v1     – Groq
            //   https://openrouter.ai/api/v1       – OpenRouter
            //   https://api.deepseek.com/v1        – DeepSeek
            //   http://localhost:11434/v1           – Ollama (local)
            "openai" => {
                // Prefer OS credential store; fall back to legacy plaintext SQLite.
                let key = {
                    let from_os = secure_store::get_secret("openai_api_key").unwrap_or_default();
                    if !from_os.is_empty() {
                        from_os
                    } else {
                        ai_admin_repo::get_config(pool, "openai_api_key")
                            .await?
                            .unwrap_or_default()
                    }
                };
                let base_url = ai_admin_repo::get_config(pool, "openai_base_url")
                    .await?
                    .unwrap_or_default();
                let model = ai_admin_repo::get_config(pool, "openai_model")
                    .await?
                    .unwrap_or_default();
                if key.is_empty() || base_url.is_empty() || model.is_empty() {
                    return Ok(None);
                }
                Ok(Some(Provider::OpenAI(OpenAIClient::new(
                    base_url, key, model,
                ))))
            }
            "gemini" => {
                // Gemini key: OS credential store first, then legacy DB fallback.
                let key = {
                    let from_os = secure_store::get_secret("gemini_api_key").unwrap_or_default();
                    if !from_os.is_empty() {
                        from_os
                    } else {
                        ai_admin_repo::get_config(pool, "gemini_api_key")
                            .await?
                            .unwrap_or_default()
                    }
                };
                let model = ai_admin_repo::get_config(pool, "gemini_model")
                    .await?
                    .filter(|m| !m.is_empty())
                    .unwrap_or_else(|| GEMINI_DEFAULT_MODEL.to_string());
                if key.is_empty() {
                    return Ok(None);
                }
                Ok(Some(Provider::Gemini(OpenAIClient::new(
                    GEMINI_BASE_URL.to_string(),
                    key,
                    model,
                ))))
            }
            _ => Ok(None),
        }
    }

    /// Returns true if this is the Anthropic provider.
    pub fn is_anthropic(&self) -> bool {
        matches!(self, Provider::Anthropic(_))
    }

    /// Returns the raw API key string for streaming use.
    pub(crate) fn api_key(&self) -> &str {
        match self {
            Provider::Anthropic(c) => c.api_key(),
            Provider::OpenAI(_) | Provider::Gemini(_) => "",
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
        // C-05: apply sliding-window guard before building messages
        let history = truncate_history(history);
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
                if let Some((id, name, input)) = extract_tool_use(&resp.content) {
                    return Ok(ChatResult {
                        text: extract_text(&resp.content),
                        tool_call: Some(ToolCallResult { id, name, input }),
                        reasoning_content: None,
                    });
                }
                Ok(ChatResult {
                    text: extract_text(&resp.content),
                    tool_call: None,
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
                msgs.push(user_msg(user_message));

                let resp = c.send(system, msgs, tools).await?;
                let reasoning = resp.reasoning_content.clone();
                if let Some(tc) = resp.tool_call {
                    return Ok(ChatResult {
                        text: resp.text,
                        tool_call: Some(ToolCallResult {
                            id: tc.id,
                            name: tc.name,
                            input: tc.input,
                        }),
                        reasoning_content: reasoning,
                    });
                }
                Ok(ChatResult {
                    text: resp.text,
                    tool_call: None,
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
        self.continue_with_tool_turns(system, history, user_message, &[turn], tools)
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
    ) -> AppResult<ChatResult> {
        let history = truncate_history(history);
        match self {
            Provider::Anthropic(c) => {
                let mut msgs: Vec<AnthropicMessage> = history
                    .iter()
                    .map(|m| if m.role == "user" {
                        AnthropicMessage::user_text(&m.content)
                    } else {
                        AnthropicMessage::assistant_text(&m.content)
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
                if let Some((id, name, input)) = extract_tool_use(&resp.content) {
                    return Ok(ChatResult {
                        text: extract_text(&resp.content),
                        tool_call: Some(ToolCallResult { id, name, input }),
                        reasoning_content: None,
                    });
                }
                Ok(ChatResult {
                    text: extract_text(&resp.content),
                    tool_call: None,
                    reasoning_content: None,
                })
            }
            Provider::OpenAI(c) | Provider::Gemini(c) => {
                let mut msgs: Vec<OpenAIMessage> = history
                    .iter()
                    .map(|m| if m.role == "user" {
                        user_msg(&m.content)
                    } else {
                        assistant_msg(&m.content)
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
                        },
                    );
                    if let Some(ref reason) = turn.reasoning_content {
                        if let OpenAIMessage::Assistant { ref mut reasoning_content, .. } = tc_msg {
                            *reasoning_content = Some(reason.clone());
                        }
                    }
                    msgs.push(tc_msg);
                    msgs.push(tool_result_msg(&turn.tool_call.id, turn.tool_result.clone()));
                }
                let resp = c.send(system, msgs, tools).await?;
                let reasoning = resp.reasoning_content.clone();
                if let Some(tc) = resp.tool_call {
                    return Ok(ChatResult {
                        text: resp.text,
                        tool_call: Some(ToolCallResult { id: tc.id, name: tc.name, input: tc.input }),
                        reasoning_content: reasoning,
                    });
                }
                Ok(ChatResult { text: resp.text, tool_call: None, reasoning_content: reasoning })
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
        // C-05: apply sliding-window guard before building messages
        let history = truncate_history(history);
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
