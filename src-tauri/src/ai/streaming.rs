//! Real-time streaming chat via Anthropic SSE API.
//! Drives the same multi-turn tool loop as ai_chat but emits StreamEvent
//! tokens through a Tauri Channel so the frontend can render word-by-word.

use crate::ai::tools;
use crate::db::repositories::ai_admin_repo;
use crate::domain::ai_admin::{AiChatInput, ChatMessage, StreamEvent, ToolPreview};
use crate::errors::{AppError, AppResult};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;
use tauri::ipc::Channel;
use ulid::Ulid;

// ── Anthropic streaming request ────────────────────────────────────────────────

#[derive(Serialize)]
struct AnthropicStreamRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    system: String,
    messages: Vec<AnthropicMsg>,
    tools: Vec<crate::ai::client::ToolDef>,
    stream: bool,
}

#[derive(Serialize, Deserialize, Clone)]
struct AnthropicMsg {
    role: String,
    content: Vec<MsgContent>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
enum MsgContent {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

// ── SSE event variants we care about ─────────────────────────────────────────

#[allow(dead_code)] // fields exist for Debug/Deserialize compatibility
#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SseEvent {
    ContentBlockStart { content_block: SseBlock },
    ContentBlockDelta { delta: SseDelta },
    ContentBlockStop {},
    MessageDelta { delta: MsgDeltaData },
    MessageStart { message: Value },
    MessageStop {},
    Ping {},
    Error { error: Value },
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SseBlock {
    Text { text: String },
    ToolUse { id: String, name: String },
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SseDelta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct MsgDeltaData {
    stop_reason: Option<String>,
}

// ── Main streaming function ───────────────────────────────────────────────────

/// Valid Anthropic model alias — claude-sonnet-4-20250514 retires 2026-06-15.
const MODEL: &str = "claude-sonnet-4-6";
/// 8096 tokens accommodates tool-call JSON + multi-step reasoning loops.
const MAX_TOKENS: u32 = 8096;
const MAX_TURNS: usize = 8;
const API_URL: &str = "https://api.anthropic.com/v1/messages";

/// Drive a full multi-turn tool-loop, emitting StreamEvent tokens to the channel.
/// Returns the accumulated assistant text (for saving to history).
pub async fn run_streaming_chat(
    pool: &SqlitePool,
    api_key: &str,
    system: &str,
    input: &AiChatInput,
    tool_defs: &[crate::ai::client::ToolDef],
    on_event: &Channel<StreamEvent>,
) -> AppResult<String> {
    // R-02: SSE streams are long-lived — connect timeout fails fast on unreachable
    // hosts; an overall request timeout (120s) guards against mid-stream API hangs
    // that would otherwise keep the Tauri async task alive forever.
    let http = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .unwrap_or_default();
    let mut msgs = build_messages(&input.history, &input.message);
    let mut accumulated_text = String::new();

    for _turn in 0..MAX_TURNS {
        // ── POST with stream: true ─────────────────────────────────────────────
        let request_body = AnthropicStreamRequest {
            model: MODEL.into(),
            max_tokens: MAX_TOKENS,
            temperature: Some(0.0),
            system: system.into(),
            messages: msgs.clone(),
            tools: tool_defs.to_vec(),
            stream: true,
        };

        let response = http
            .post(API_URL)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Stream request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let msg = format!("API error {status}: {body}");
            let _ = on_event.send(StreamEvent::Error {
                message: msg.clone(),
            });
            return Err(AppError::Internal(msg));
        }

        // ── Parse SSE line-by-line ─────────────────────────────────────────────
        let mut byte_stream = response.bytes_stream();
        let mut line_buf = String::new();
        let mut turn_text = String::new();
        let mut tool_id = String::new();
        let mut tool_name = String::new();
        let mut tool_json = String::new();
        let mut is_tool_turn = false;

        'sse: while let Some(chunk) = byte_stream.next().await {
            let chunk = chunk.map_err(|e| AppError::Internal(format!("Stream read error: {e}")))?;
            line_buf.push_str(&String::from_utf8_lossy(&chunk));

            // Process all complete lines in the buffer
            loop {
                match line_buf.find('\n') {
                    None => break,
                    Some(pos) => {
                        let raw = line_buf[..pos].trim_end_matches('\r').to_string();
                        line_buf = line_buf[pos + 1..].to_string();

                        if raw.is_empty() {
                            continue;
                        }

                        let data = match raw.strip_prefix("data: ") {
                            None => continue,
                            Some(d) => d,
                        };

                        if data == "[DONE]" {
                            break 'sse;
                        }

                        let event: SseEvent = match serde_json::from_str(data) {
                            Ok(e) => e,
                            Err(_) => continue,
                        };

                        match event {
                            SseEvent::ContentBlockStart {
                                content_block: SseBlock::ToolUse { id, name },
                            } => {
                                is_tool_turn = true;
                                tool_id = id;
                                tool_name = name.clone();
                                let _ =
                                    on_event.send(StreamEvent::ToolStart { name: name.clone() });
                            }
                            SseEvent::ContentBlockStart {
                                content_block: SseBlock::Text { .. },
                            } => {}
                            SseEvent::ContentBlockDelta {
                                delta: SseDelta::TextDelta { text },
                            } => {
                                turn_text.push_str(&text);
                                accumulated_text.push_str(&text);
                                let _ = on_event.send(StreamEvent::Token { text });
                            }
                            SseEvent::ContentBlockDelta {
                                delta: SseDelta::InputJsonDelta { partial_json },
                            } => {
                                tool_json.push_str(&partial_json);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // ── End of this turn ──────────────────────────────────────────────────
        if !is_tool_turn {
            // Pure text response — done
            let _ = on_event.send(StreamEvent::Done);
            return Ok(accumulated_text);
        }

        // R-04: Parse the accumulated tool input JSON. If it is malformed, do NOT
        // silently proceed with an empty object — that would let a mutation execute
        // with all-default values. Log the raw JSON and surface an error instead.
        let tool_input: Value = match serde_json::from_str(&tool_json) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(
                    "AI tool '{}' returned malformed input JSON: {} — raw: {}",
                    tool_name,
                    e,
                    tool_json
                );
                let msg = format!(
                    "The AI produced invalid parameters for '{}' and the action was stopped. Please retry.",
                    tool_name
                );
                let _ = on_event.send(StreamEvent::Error {
                    message: msg.clone(),
                });
                return Err(AppError::Internal(msg));
            }
        };

        let _ = on_event.send(StreamEvent::ToolDone {
            name: tool_name.clone(),
        });

        // ── Run Engine dispatch ─────────────────────────────────────────────────
        // Engine ops bypass the normal mutation_pending/confirm flow: they create
        // a run record in 'previewing' state, emit RunPreview so the UI can show
        // a count + confirm button, then return. The user triggers execution via
        // the separate ai_run_execute Tauri command.
        const ENGINE_OPS: &[&str] = &["bulk_price_adjust"];
        if ENGINE_OPS.contains(&tool_name.as_str()) {
            use crate::ai::engine::{runs, selector::Selector, PriceOp};
            let selector: Selector = serde_json::from_value(
                tool_input.get("selector").cloned().unwrap_or_default(),
            )
            .unwrap_or_default();
            let price_op: PriceOp = serde_json::from_value(
                tool_input.get("adjustment").cloned().unwrap_or_default(),
            )
            .unwrap_or(PriceOp::Percent(0.0));
            let count = selector.count(pool).await.unwrap_or(0);
            let selector_json = serde_json::to_string(&selector).unwrap_or_default();
            let params_json = serde_json::to_string(&price_op).unwrap_or_default();
            let run_id = runs::create_run(
                pool,
                &tool_name,
                &selector_json,
                &params_json,
                count,
                &input.user_id,
            )
            .await?;
            let description = match &price_op {
                PriceOp::Percent(p) if *p >= 0.0 =>
                    format!("Increase prices by {p}% for {count} products"),
                PriceOp::Percent(p) =>
                    format!("Decrease prices by {}% for {count} products", p.abs()),
                PriceOp::Absolute(d) if *d >= 0 =>
                    format!("Add {d} fils to {count} products"),
                PriceOp::Absolute(d) =>
                    format!("Subtract {} fils from {count} products", d.abs()),
                PriceOp::Set(v) =>
                    format!("Set price to {v} fils for {count} products"),
            };
            let _ = on_event.send(StreamEvent::RunPreview {
                run_id,
                op_id: tool_name,
                description,
                count,
                samples: vec![],
            });
            let _ = on_event.send(StreamEvent::Done);
            return Ok(accumulated_text);
        }

        // ── Intent Engine dispatch (ZanAI v2) ──────────────────────────
        if crate::ai::intent_engine::INTENT_NAMES.contains(&tool_name.as_str()) {
            if crate::ai::intent_engine::is_mutation_intent(&tool_name) {
                // Mutation intent — needs confirmation
                let preview_text = format!("{} with params: {}", tool_name, tool_input);
                let action = ai_admin_repo::create_action(
                    pool,
                    &input.user_id,
                    &tool_name,
                    &tool_input.to_string(),
                    &hash_str(&tool_input.to_string()),
                    &preview_text,
                    &Ulid::new().to_string(),
                )
                .await?;
                let _ = on_event.send(StreamEvent::MutationPending {
                    action_id: action.action_id,
                    tool_name: tool_name.clone(),
                    preview: crate::domain::ai_admin::ToolPreview {
                        tool_name: tool_name.clone(),
                        description: preview_text,
                        fields: vec![],
                    },
                    expires_at: action.expires_at,
                    assistant_text: turn_text,
                });
                return Ok(accumulated_text);
            }
            // Read intent — execute directly
            let result = crate::ai::intent_engine::execute_intent(
                pool,
                &tool_name,
                &tool_input,
                &input.branch_id,
            )
            .await;
            match result {
                Ok(r) => {
                    let result_text =
                        serde_json::to_string(&r.data).unwrap_or_else(|_| "{}".into());
                    // Append result as tool response
                    msgs.push(AnthropicMsg {
                        role: "assistant".into(),
                        content: vec![
                            MsgContent::Text {
                                text: turn_text.clone(),
                            },
                            MsgContent::ToolUse {
                                id: tool_id.clone(),
                                name: tool_name.clone(),
                                input: tool_input.clone(),
                            },
                        ],
                    });
                    msgs.push(AnthropicMsg {
                        role: "user".into(),
                        content: vec![MsgContent::ToolResult {
                            tool_use_id: tool_id.clone(),
                            content: result_text,
                        }],
                    });
                    // Emit Navigate for open_tab
                    if tool_name == "open_tab" {
                        if let Some(tab) = tool_input.get("tab").and_then(|v| v.as_str()) {
                            let _ = on_event.send(StreamEvent::Navigate {
                                tab: tab.to_string(),
                            });
                        }
                    }
                    continue;
                }
                Err(e) => {
                    let msg = format!("Intent '{}' failed: {e}", tool_name);
                    let _ = on_event.send(StreamEvent::Error {
                        message: msg.clone(),
                    });
                    return Err(AppError::Internal(msg));
                }
            }
        }

        // Mutation → needs confirmation, emit pending event and exit
        if tools::is_mutation_tool(&tool_name) {
            let preview =
                tools::dry_run_mutation(pool, &tool_name, &tool_input, input.currency_exponent)
                    .await?;

            let tool_input_json = tool_input.to_string();
            let preview_text = preview_to_text(&preview);

            let action = ai_admin_repo::create_action(
                pool,
                &input.user_id,
                &tool_name,
                &tool_input_json,
                &hash_str(&tool_input_json),
                &preview_text,
                &Ulid::new().to_string(),
            )
            .await?;

            let _ = on_event.send(StreamEvent::MutationPending {
                action_id: action.action_id,
                tool_name,
                preview,
                expires_at: action.expires_at,
                assistant_text: turn_text,
            });
            return Ok(accumulated_text);
        }

        let tool_result = match tools::execute_read_tool(
            pool,
            &tool_name,
            &tool_input,
            &input.branch_id,
            input.currency_exponent,
        )
        .await
        {
            Ok(result) => result,
            Err(e) => {
                let msg = format!("Tool '{}' failed: {e}", tool_name);
                tracing::error!("{msg}");
                msg
            }
        };

        // Emit Navigate event for open_tab tool results (office-ai workspace)
        if tool_name == "open_tab" {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&tool_result) {
                if let Some(tab) = parsed.get("tab").and_then(|v| v.as_str()) {
                    let _ = on_event.send(StreamEvent::Navigate {
                        tab: tab.to_string(),
                    });
                }
            }
        }

        // Append assistant turn + tool result to message list
        msgs.push(AnthropicMsg {
            role: "assistant".into(),
            content: vec![
                MsgContent::Text {
                    text: turn_text.clone(),
                },
                MsgContent::ToolUse {
                    id: tool_id.clone(),
                    name: tool_name.clone(),
                    input: tool_input,
                },
            ],
        });
        msgs.push(AnthropicMsg {
            role: "user".into(),
            content: vec![MsgContent::ToolResult {
                tool_use_id: tool_id.clone(),
                content: tool_result,
            }],
        });

        // Reset for next turn
        turn_text.clear();
        tool_id.clear();
        tool_name.clear();
        tool_json.clear();
    }

    // R-08: We exhausted MAX_TURNS without the model producing a final text answer.
    // Tell the user the conversation was cut short instead of ending silently.
    tracing::warn!("AI streaming hit MAX_TURNS ({MAX_TURNS}) tool-loop limit — truncated");
    let _ = on_event.send(StreamEvent::Error {
        message: format!(
            "Reached the {MAX_TURNS}-step limit for one request. The answer may be incomplete — \
             try narrowing your request or asking again."
        ),
    });
    let _ = on_event.send(StreamEvent::Done);
    Ok(accumulated_text)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn build_messages(history: &[ChatMessage], user_message: &str) -> Vec<AnthropicMsg> {
    // C-05: apply sliding-window guard — drop oldest pairs if history is too large
    let history = truncate_history(history);
    let mut msgs: Vec<AnthropicMsg> = history
        .iter()
        .map(|m| AnthropicMsg {
            role: m.role.clone(),
            content: vec![MsgContent::Text {
                text: m.content.clone(),
            }],
        })
        .collect();
    msgs.push(AnthropicMsg {
        role: "user".into(),
        content: vec![MsgContent::Text {
            text: user_message.into(),
        }],
    });
    msgs
}

/// Rough token estimator: 1 token ≈ 4 characters.
/// Keeps history within ~80k tokens so a large conversation never overflows
/// the model's context window. Drops oldest pairs (user+assistant) from front.
const MAX_HISTORY_CHARS: usize = 320_000;

fn truncate_history(history: &[ChatMessage]) -> &[ChatMessage] {
    let total: usize = history.iter().map(|m| m.content.len()).sum();
    if total <= MAX_HISTORY_CHARS {
        return history;
    }
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

fn preview_to_text(p: &ToolPreview) -> String {
    format!(
        "{}: {}",
        p.description,
        p.fields
            .iter()
            .map(|f| format!("{} = {}", f.label, f.value))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn hash_str(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}
