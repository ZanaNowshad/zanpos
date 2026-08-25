//! Real-time streaming chat via Anthropic SSE API.
//! Drives the same multi-turn tool loop as ai_chat but emits StreamEvent
//! tokens through a Tauri Channel so the frontend can render word-by-word.

use crate::ai::config::load_ai_params;
use crate::ai::engine::ops::Preview;
use crate::ai::openai_client::{
    assistant_msg, tool_result_msg, user_msg, user_msg_with_image, OpenAIClient, OpenAIMessage,
};
use crate::ai::result_budget::ToolResultBudget;
use crate::ai::tools;
use crate::db::repositories::ai_admin_repo;
use crate::domain::ai_admin::{
    AiChatInput, BatchPendingAction, ChatMessage, StreamEvent, ToolPreview, ToolPreviewField,
};
use crate::errors::{AppError, AppResult};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;
use std::future::Future;
use std::time::Instant;
use tauri::ipc::Channel;
use ulid::Ulid;

// ── Helper ────────────────────────────────────────────────────────────────────────

/// Send a StreamEvent over a Tauri Channel, logging any error instead of silently discarding it.
macro_rules! send_or_log {
    ($chan:expr, $event:expr) => {
        if let Err(e) = ($chan).send($event) {
            tracing::error!("Channel send failed: {e}");
        }
    };
}

const READ_TOOL_CONCURRENCY: usize = 3;
type ParsedToolCall = (String, String, Value);

fn is_plain_read_tool(name: &str) -> bool {
    !tools::is_mutation_tool(name)
        && !crate::ai::engine::ops::is_registered_operation(name)
        && !crate::ai::intent_engine::INTENT_NAMES.contains(&name)
}

fn plain_read_run_end(calls: &[ParsedToolCall], start: usize) -> usize {
    calls[start..]
        .iter()
        .take_while(|(_, name, _)| is_plain_read_tool(name))
        .count()
        + start
}

/// Read tools whose point is an effect on the screen, not their return value.
///
/// `open_tab` steers the workspace and `request_input` draws a form in the
/// chat; in both the model's tool result is only an acknowledgement, and the
/// real output is the event raised here. A read result is consumed in four
/// places across the two provider loops, so the mapping lives in one function —
/// otherwise every UI tool has to remember all four, and the one that forgets
/// fails silently on whichever provider the shop happens to use.
fn ui_event_for_read_tool(
    tool_name: &str,
    tool_input: &Value,
    content: &str,
    is_error: Option<bool>,
) -> Option<StreamEvent> {
    if is_error.unwrap_or(false) {
        return None;
    }
    match tool_name {
        "open_tab" => serde_json::from_str::<Value>(content)
            .ok()?
            .get("tab")
            .and_then(Value::as_str)
            .map(|tab| StreamEvent::Navigate {
                tab: tab.to_string(),
            }),
        // Re-parsed from the input the model sent rather than echoed through the
        // tool result: the form can run to sixty rows, and paying for it twice
        // in context would make the feature expensive exactly when it is most
        // useful. `execute_read_tool` already accepted this same input, so the
        // parse cannot fail here.
        "request_input" => crate::ai::forms::parse_form_spec(tool_input)
            .ok()
            .map(|form| StreamEvent::FormRequest { form }),
        _ => None,
    }
}

/// A turn that asks the operator for values must not also write them.
///
/// The model calls `request_input` precisely because something is missing, so a
/// mutation queued in the same breath is acting on a guess. Nothing downstream
/// would catch it either: `update_product_price` is reversible and routine, so
/// the default confirmation policy executes it automatically. Refusing here is
/// recoverable — the model gets the operator's answer next turn and writes the
/// value it was actually given.
const FORM_TURN_BLOCKED: &str = "Blocked: you asked the operator for input in this same step, so you do not have the values this needs yet. Wait for their answer, then call this again.";

fn writes_data(tool_name: &str) -> bool {
    tools::is_mutation_tool(tool_name) || crate::ai::engine::ops::is_registered_operation(tool_name)
}

async fn collect_bounded_ordered<I, T, F, Fut, R>(items: I, limit: usize, operation: F) -> Vec<R>
where
    I: IntoIterator<Item = T>,
    F: Fn(T) -> Fut,
    Fut: Future<Output = R>,
{
    futures::stream::iter(items)
        .map(operation)
        .buffered(limit.max(1))
        .collect()
        .await
}

enum PreparedRead {
    AuthorizationError(String),
    Result {
        content: String,
        is_error: Option<bool>,
    },
}

fn budget_tool_result_if_success(
    budget: &mut ToolResultBudget,
    tool_name: &str,
    content: String,
    is_error: bool,
) -> String {
    if is_error {
        return content;
    }
    let result = budget.apply(tool_name, content);
    if result.truncated {
        tracing::warn!(
            tool = tool_name,
            original_chars = result.original_chars,
            emitted_chars = result.emitted_chars,
            reason = result.reason.as_deref().unwrap_or("unknown"),
            "ZanAI tool result truncated by context budget"
        );
    }
    result.content
}

async fn execute_plain_read_batch(
    pool: &SqlitePool,
    calls: Vec<ParsedToolCall>,
    branch_id: &str,
    currency_exponent: u32,
    actor_role: &str,
) -> Vec<(String, PreparedRead)> {
    collect_bounded_ordered(
        calls,
        READ_TOOL_CONCURRENCY,
        |(id, name, input)| async move {
            let role_result = crate::ai::tool_policy::require_role_allows_tool(actor_role, &name);
            let prepared = match role_result {
                Err(error) => PreparedRead::AuthorizationError(error.to_string()),
                Ok(()) => match crate::ai::tool_policy::authorize_plan(
                    pool,
                    &name,
                    &input,
                    &crate::ai::tool_policy::ProvenanceState::default(),
                )
                .await
                {
                    Err(error) => PreparedRead::AuthorizationError(error.to_string()),
                    Ok(_) => {
                        let (mut content, is_error) = match tools::execute_read_tool(
                            pool,
                            &name,
                            &input,
                            branch_id,
                            currency_exponent,
                        )
                        .await
                        {
                            Ok(result) => (result, None),
                            Err(error) => {
                                let message = format!("Tool '{}' failed: {error}", name);
                                tracing::error!("{message}");
                                (message, Some(true))
                            }
                        };
                        if is_error.is_none()
                            && crate::ai::tool_policy::is_external_content_tool(&name)
                        {
                            content = crate::ai::tool_policy::tag_external_result(content);
                        }
                        PreparedRead::Result { content, is_error }
                    }
                },
            };
            (id, prepared)
        },
    )
    .await
}

// ── Anthropic streaming request ────────────────────────────────────────────────

#[derive(Serialize, Clone, Copy)]
struct CacheControl {
    #[serde(rename = "type")]
    kind: &'static str,
    /// Omitted entirely for the 5-minute default. `Some("1h")` opts into the
    /// extended TTL: the write costs 2x instead of 1.25x and needs three reads
    /// to break even, which a back-office session clears easily.
    #[serde(skip_serializing_if = "Option::is_none")]
    ttl: Option<&'static str>,
}

/// Tools and the system prompt are both stable across a session, so they share
/// the long TTL.
const LONG_CACHE: CacheControl = CacheControl {
    kind: "ephemeral",
    ttl: Some("1h"),
};

/// Claude 4.7 and every later model **reject** `temperature`, `top_p`, and
/// `top_k` with a 400 — they are not merely ignored. The parameter has to be
/// omitted per-model rather than dropped wholesale, because the same request
/// builder still serves older models an operator may have configured.
fn model_rejects_sampling_params(model: &str) -> bool {
    const REJECTING: &[&str] = &[
        "claude-opus-5",
        "claude-sonnet-5",
        "claude-fable-5",
        "claude-mythos-5",
        "claude-opus-4-8",
        "claude-opus-4-7",
    ];
    REJECTING.iter().any(|m| model.starts_with(m))
}

#[derive(Serialize)]
struct AnthropicSystemBlock<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    text: &'a str,
    cache_control: CacheControl,
}

#[derive(Serialize)]
struct AnthropicToolDef<'a> {
    name: &'a str,
    description: &'a str,
    input_schema: &'a Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_control: Option<CacheControl>,
}

#[derive(Serialize)]
struct AnthropicStreamRequest<'a> {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_config: Option<AnthropicOutputConfig>,
    system: Vec<AnthropicSystemBlock<'a>>,
    messages: Vec<AnthropicMsg>,
    tools: Vec<AnthropicToolDef<'a>>,
    stream: bool,
}

/// `effort` trades thinking depth against tokens and latency. It is GA (no beta
/// header) on the current models and ignored by older ones.
#[derive(Serialize)]
struct AnthropicOutputConfig {
    effort: String,
}

fn anthropic_system_blocks(system: &str) -> Vec<AnthropicSystemBlock<'_>> {
    vec![AnthropicSystemBlock {
        kind: "text",
        text: system,
        cache_control: LONG_CACHE,
    }]
}

fn anthropic_tools_with_cache_control(
    tools: &[crate::ai::client::ToolDef],
) -> Vec<AnthropicToolDef<'_>> {
    let last_index = tools.len().checked_sub(1);
    tools
        .iter()
        .enumerate()
        .map(|(index, tool)| AnthropicToolDef {
            name: &tool.name,
            description: &tool.description,
            input_schema: &tool.input_schema,
            cache_control: (Some(index) == last_index).then_some(LONG_CACHE),
        })
        .collect()
}

#[derive(Serialize, Deserialize, Clone)]
struct AnthropicMsg {
    role: String,
    content: Vec<MsgContent>,
}

#[derive(Serialize, Deserialize, Clone)]
struct ImageSource {
    #[serde(rename = "type")]
    source_type: String,
    media_type: String,
    data: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
enum MsgContent {
    Text {
        text: String,
    },
    Image {
        source: ImageSource,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

fn recoverable_tool_error_message(tool_name: &str, error: &str) -> String {
    if tool_name == "create_product" && error.to_ascii_lowercase().contains("category") {
        return format!(
            "Tool '{tool_name}' could not run: {error}. Call list_categories to obtain a current category_id, then retry create_product once with that grounded ID."
        );
    }
    format!(
        "Tool '{tool_name}' blocked by execution policy: {error}. Correct the tool arguments and retry once."
    )
}

fn anthropic_recoverable_tool_error(
    tool_id: &str,
    tool_name: &str,
    tool_input: &Value,
    error: &str,
) -> (MsgContent, MsgContent) {
    (
        MsgContent::ToolUse {
            id: tool_id.to_string(),
            name: tool_name.to_string(),
            input: tool_input.clone(),
        },
        MsgContent::ToolResult {
            tool_use_id: tool_id.to_string(),
            content: recoverable_tool_error_message(tool_name, error),
            is_error: Some(true),
        },
    )
}

fn openai_recoverable_tool_error(
    tool_id: &str,
    tool_name: &str,
    tool_input: &Value,
    error: &str,
) -> (crate::ai::openai_client::OpenAIToolCall, OpenAIMessage) {
    (
        crate::ai::openai_client::OpenAIToolCall {
            id: tool_id.to_string(),
            kind: "function".into(),
            function: crate::ai::openai_client::OpenAIToolCallFunction {
                name: tool_name.to_string(),
                arguments: tool_input.to_string(),
            },
        },
        tool_result_msg(tool_id, recoverable_tool_error_message(tool_name, error)),
    )
}

fn automatic_execution_allowed(
    decision: crate::ai::tool_policy::PlanDecision,
    _total_tool_calls: usize,
) -> bool {
    decision == crate::ai::tool_policy::PlanDecision::AutomaticEligible
}

fn automatic_mutation_content(result: &crate::ai::tool_policy::AutomaticMutationResult) -> String {
    serde_json::json!({
        "status": "executed",
        "description": result.description,
        "undo_id": result.undo_id,
        "instruction": "Read back the completed change to the user."
    })
    .to_string()
}

fn anthropic_automatic_mutation_result(
    tool_id: &str,
    result: &crate::ai::tool_policy::AutomaticMutationResult,
) -> MsgContent {
    MsgContent::ToolResult {
        tool_use_id: tool_id.to_string(),
        content: automatic_mutation_content(result),
        is_error: None,
    }
}

fn openai_automatic_mutation_result(
    tool_id: &str,
    result: &crate::ai::tool_policy::AutomaticMutationResult,
) -> OpenAIMessage {
    tool_result_msg(tool_id, automatic_mutation_content(result))
}

#[derive(Debug, PartialEq, Eq)]
enum EngineTurnDecision {
    Execute,
    RecoverableError(String),
}

fn plan_engine_turn(
    validation_errors: &[Option<String>],
    total_tool_calls: usize,
) -> Vec<EngineTurnDecision> {
    if validation_errors.len() == 1 && total_tool_calls == 1 {
        return vec![match &validation_errors[0] {
            Some(error) => EngineTurnDecision::RecoverableError(error.clone()),
            None => EngineTurnDecision::Execute,
        }];
    }
    validation_errors
        .iter()
        .map(|error| {
            EngineTurnDecision::RecoverableError(error.clone().unwrap_or_else(|| {
                "Engine mutations must be requested one at a time. Retry this operation in a new turn."
                    .into()
            }))
        })
        .collect()
}

async fn preflight_engine_turn(
    pool: &SqlitePool,
    calls: Vec<(String, String, Value)>,
    total_tool_calls: usize,
) -> std::collections::HashMap<String, EngineTurnDecision> {
    let registry = crate::ai::engine::ops::operation_registry();
    let mut errors = Vec::with_capacity(calls.len());
    for (_, name, input) in &calls {
        let error = match registry.find(name) {
            Some(operation) => operation
                .validate(pool, input)
                .await
                .err()
                .map(|errors| format!("Validation failed: {}", errors.join("; "))),
            None => Some(format!("Unknown engine operation: {name}")),
        };
        errors.push(error);
    }
    calls
        .into_iter()
        .map(|(id, _, _)| id)
        .zip(plan_engine_turn(&errors, total_tool_calls))
        .collect()
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
    usage: Option<SseUsage>,
}

#[derive(Deserialize, Debug)]
struct SseUsage {
    output_tokens: u32,
}

// ── Main streaming function ───────────────────────────────────────────────────

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
    session_id: &str,
    provider_name: &str,
    model_name: &str,
    tool_subsetting_enabled: bool,
    actor_role: &str,
) -> AppResult<String> {
    let ev = on_event;
    let params = load_ai_params(pool).await;
    let model = model_name.to_string();
    let http = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(params.connect_timeout_secs))
        .timeout(std::time::Duration::from_secs(params.stream_timeout_secs))
        .build()
        .unwrap_or_default();
    let mut msgs = build_messages(input, params.context_window_chars);
    let mut accumulated_text = String::new();
    let mut provenance = crate::ai::tool_policy::ProvenanceState::default();
    let mut full_tool_access = false;
    let mut mutation_executed = false;
    // One user request may contain many internal model/tool rounds. Keep a
    // single budget across the whole request so the context cannot grow by
    // `max_turns * turn_tool_results_max_chars` on large datasets.
    let mut tool_result_budget = ToolResultBudget::new(
        params.tool_result_max_chars,
        params.turn_tool_results_max_chars,
    );

    // Fire-and-forget turn usage recorder — avoids adding .await at every exit point.
    let record_turn = |pool: &SqlitePool,
                       sid: &str,
                       turn: i32,
                       tok_in: i32,
                       tok_out: i32,
                       lat: i32,
                       pn: &str,
                       mn: &str| {
        let pool = pool.clone();
        let sid = sid.to_string();
        let pn = pn.to_string();
        let mn = mn.to_string();
        tokio::spawn(async move {
            if let Err(e) =
                ai_admin_repo::record_usage(&pool, &sid, turn, tok_in, tok_out, lat, &pn, &mn).await
            {
                tracing::error!(
                    "record_turn failed (Anthropic): session={sid} turn={turn} err={e}"
                );
            }
        });
    };

    for turn_num in 0..params.max_turns {
        let turn_start = Instant::now();
        let mut turn_output_tokens: u32 = 0;

        // ── POST with stream: true ─────────────────────────────────────────────
        let policy_definitions =
            crate::ai::tool_policy::definitions_for_request(tool_defs, &provenance)?;
        let subset = crate::ai::tool_subsetting::subset_for_message(
            &policy_definitions,
            &input.message,
            tool_subsetting_enabled,
            full_tool_access,
        )?;
        if tool_subsetting_enabled {
            tracing::info!(
                applied = subset.applied,
                omitted_mutations = subset.omitted_mutations,
                omitted_reads = subset.omitted_reads,
                kept = subset.definitions.len(),
                domains = ?subset.domains,
                widened = full_tool_access,
                "ZanAI mutation-tool subsetting decision"
            );
        }
        let turn_tool_defs = subset.definitions;
        let rejects_sampling = model_rejects_sampling_params(&model);
        let request_body = AnthropicStreamRequest {
            model: model.clone(),
            max_tokens: params.anthropic_max_tokens,
            // Sending this to a 4.7-or-later model is a 400, not a no-op.
            temperature: (!rejects_sampling).then_some(params.temperature),
            output_config: params.effort.as_ref().map(|effort| AnthropicOutputConfig {
                effort: effort.clone(),
            }),
            system: anthropic_system_blocks(system),
            messages: msgs.clone(),
            tools: anthropic_tools_with_cache_control(&turn_tool_defs),
            stream: true,
        };

        tracing::info!(turn=turn_num, model=%request_body.model, msgs=msgs.len(), "run_streaming_chat: POST to Anthropic");
        let mut attempt = 1_u8;
        let response = loop {
            match http
                .post(API_URL)
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&request_body)
                .send()
                .await
            {
                Ok(response)
                    if crate::ai::openai_client::should_retry_provider_request(
                        Some(response.status().as_u16()),
                        false,
                        mutation_executed,
                        attempt,
                    ) =>
                {
                    let delay = crate::ai::openai_client::retry_delay_for_response(
                        attempt,
                        response.headers(),
                    );
                    tracing::warn!(
                        attempt,
                        status = %response.status(),
                        delay_ms = delay.as_millis(),
                        "run_streaming_chat: transient provider response; retrying"
                    );
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                }
                Ok(response) => break response,
                Err(error)
                    if crate::ai::openai_client::should_retry_provider_request(
                        None,
                        error.is_connect(),
                        mutation_executed,
                        attempt,
                    ) =>
                {
                    let delay = crate::ai::openai_client::retry_delay_for_connection(attempt);
                    tracing::warn!(
                        attempt,
                        delay_ms = delay.as_millis(),
                        %error,
                        "run_streaming_chat: provider connection failed; retrying"
                    );
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                }
                Err(error) => {
                    return Err(AppError::Internal(format!(
                        "Stream request failed: {error}"
                    )));
                }
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let msg = format!("API error {status}: {body}");
            send_or_log!(
                ev,
                StreamEvent::Error {
                    message: msg.clone(),
                }
            );
            return Err(AppError::Internal(msg));
        }

        // ── Parse SSE line-by-line ─────────────────────────────────────────────
        let mut byte_stream = response.bytes_stream();
        let mut line_buf: Vec<u8> = Vec::new();
        let mut turn_text = String::new();
        let mut is_tool_turn = false;
        // Parallel tool-call support: accumulate every ToolUse content block
        // across one SSE turn. Each entry is (id, name, json).
        let mut tool_uses: Vec<(String, String, String)> = Vec::new();
        let mut cur_tool_id = String::new();
        let mut cur_tool_name = String::new();
        let mut cur_tool_json = String::new();

        'sse: loop {
            let idle_timeout = crate::ai::openai_client::provider_timeouts(
                std::time::Duration::from_secs(params.stream_timeout_secs),
            )
            .idle;
            let chunk = match tokio::time::timeout(idle_timeout, byte_stream.next()).await {
                Ok(Some(Ok(c))) => c,
                Ok(Some(Err(e))) => {
                    tracing::error!("run_streaming_chat: SSE read error: {e}");
                    return Err(AppError::Internal(format!("Stream read error: {e}")));
                }
                Ok(None) => break 'sse,
                Err(_elapsed) => {
                    tracing::error!(
                        idle_secs = idle_timeout.as_secs(),
                        "run_streaming_chat: SSE idle timeout"
                    );
                    let message = format!(
                        "The AI provider stopped responding for {} seconds. Retry the request.",
                        idle_timeout.as_secs()
                    );
                    send_or_log!(
                        ev,
                        StreamEvent::Error {
                            message: message.clone()
                        }
                    );
                    return Err(AppError::Internal(message));
                }
            };
            line_buf.extend_from_slice(&chunk);

            // Process all complete lines in the buffer
            loop {
                let nl = line_buf.iter().position(|&b| b == b'\n');
                match nl {
                    None => break,
                    Some(pos) => {
                        let raw = String::from_utf8_lossy(&line_buf[..pos])
                            .trim_end_matches('\r')
                            .to_string();
                        line_buf.drain(..pos + 1);

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
                                // Push the previous tool (if any) before starting a new one.
                                if !cur_tool_id.is_empty() {
                                    tool_uses.push((
                                        std::mem::take(&mut cur_tool_id),
                                        std::mem::take(&mut cur_tool_name),
                                        std::mem::take(&mut cur_tool_json),
                                    ));
                                }
                                is_tool_turn = true;
                                cur_tool_id = id;
                                cur_tool_name = name.clone();
                                send_or_log!(ev, StreamEvent::ToolStart { name: name.clone() });
                            }
                            SseEvent::ContentBlockStart {
                                content_block: SseBlock::Text { .. },
                            } => {}
                            SseEvent::ContentBlockDelta {
                                delta: SseDelta::TextDelta { text },
                            } => {
                                turn_text.push_str(&text);
                                accumulated_text.push_str(&text);
                                send_or_log!(ev, StreamEvent::Token { text });
                            }
                            SseEvent::ContentBlockDelta {
                                delta: SseDelta::InputJsonDelta { partial_json },
                            } => {
                                cur_tool_json.push_str(&partial_json);
                            }
                            SseEvent::MessageDelta { delta } => {
                                if let Some(u) = delta.usage {
                                    turn_output_tokens = u.output_tokens;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // Push the final tool block (if any) — SSE stream ended without another
        // ContentBlockStart to trigger the push in the ToolUse match arm above.
        if !cur_tool_id.is_empty() {
            tool_uses.push((
                std::mem::take(&mut cur_tool_id),
                std::mem::take(&mut cur_tool_name),
                std::mem::take(&mut cur_tool_json),
            ));
        }

        // ── End of this turn ──────────────────────────────────────────────────
        let turn_ms = turn_start.elapsed().as_millis() as i32;
        if !is_tool_turn {
            send_or_log!(ev, StreamEvent::Done);
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            record_turn(
                pool,
                session_id,
                turn_num as i32 + 1,
                0,
                turn_output_tokens as i32,
                turn_ms,
                provider_name,
                model_name,
            );
            return Ok(accumulated_text);
        }

        // R-04: Parse every accumulated tool input JSON. Malformed JSON for any
        // tool is surfaced as an error instead of silently defaulting to empty.
        let mut parsed_tools: Vec<ParsedToolCall> = Vec::with_capacity(tool_uses.len());
        for (id, name, json) in &tool_uses {
            match serde_json::from_str(json) {
                Ok(v) => {
                    send_or_log!(ev, StreamEvent::ToolDone { name: name.clone() });
                    parsed_tools.push((id.clone(), name.clone(), v));
                }
                Err(e) => {
                    tracing::error!(
                        "AI tool '{}' returned malformed input JSON: {} — raw: {}",
                        name,
                        e,
                        json
                    );
                    let msg = format!(
                        "The AI produced invalid parameters for '{}' and the action was stopped. Please retry.",
                        name
                    );
                    send_or_log!(
                        ev,
                        StreamEvent::Error {
                            message: msg.clone()
                        }
                    );
                    record_turn(
                        pool,
                        session_id,
                        turn_num as i32 + 1,
                        0,
                        turn_output_tokens as i32,
                        turn_ms,
                        provider_name,
                        model_name,
                    );
                    return Err(AppError::Internal(msg));
                }
            }
        }

        // Content blocks for the combined assistant + user message pair.
        // Text tokenises the model's thinking; each tool appends its block below.
        let mut assist_blocks: Vec<MsgContent> = vec![MsgContent::Text {
            text: turn_text.clone(),
        }];
        let mut result_blocks: Vec<MsgContent> = vec![];
        let mut pending_mutations: Vec<BatchPendingAction> = Vec::new();
        for (_, tool_name, _) in &parsed_tools {
            crate::ai::tool_policy::require_role_allows_tool(actor_role, tool_name)?;
        }
        // Decided before anything runs, so a mutation the model listed *first*
        // is still caught. `form_shown` is separate and set only once a form
        // actually reached the screen: a spec that failed validation must leave
        // the turn running so the model can correct it.
        let form_requested = parsed_tools
            .iter()
            .any(|(_, name, _)| name == "request_input");
        let mut form_shown = false;
        let engine_calls: Vec<_> = parsed_tools
            .iter()
            .filter(|(_, name, _)| crate::ai::engine::ops::is_registered_operation(name))
            .cloned()
            .collect();
        let mut engine_preflight =
            preflight_engine_turn(pool, engine_calls, parsed_tools.len()).await;

        if parsed_tools
            .iter()
            .any(|(_, name, _)| crate::ai::tool_policy::is_external_content_tool(name))
        {
            provenance.mark_external("external tool result in current request");
        }
        let mut prepared_reads = std::collections::HashMap::new();
        for (tool_index, (tool_id, tool_name, tool_input)) in parsed_tools.iter().enumerate() {
            if form_requested && writes_data(tool_name) {
                let (tool_use, tool_result) =
                    anthropic_recoverable_tool_error(tool_id, tool_name, tool_input, FORM_TURN_BLOCKED);
                assist_blocks.push(tool_use);
                result_blocks.push(tool_result);
                continue;
            }
            if let Some(prepared) = prepared_reads.remove(tool_id) {
                match prepared {
                    PreparedRead::AuthorizationError(message) => {
                        let (tool_use, tool_result) = anthropic_recoverable_tool_error(
                            tool_id, tool_name, tool_input, &message,
                        );
                        assist_blocks.push(tool_use);
                        result_blocks.push(tool_result);
                    }
                    PreparedRead::Result { content, is_error } => {
                        let content = budget_tool_result_if_success(
                            &mut tool_result_budget,
                            tool_name,
                            content,
                            is_error.unwrap_or(false),
                        );
                        if tool_name == "request_full_tool_access" {
                            full_tool_access = true;
                            tracing::info!("ZanAI tool catalogue widened for the next step");
                        }
                        if let Some(event) =
                            ui_event_for_read_tool(tool_name, tool_input, &content, is_error)
                        {
                            form_shown |= matches!(event, StreamEvent::FormRequest { .. });
                            send_or_log!(ev, event);
                        }
                        assist_blocks.push(MsgContent::ToolUse {
                            id: tool_id.clone(),
                            name: tool_name.clone(),
                            input: tool_input.clone(),
                        });
                        result_blocks.push(MsgContent::ToolResult {
                            tool_use_id: tool_id.clone(),
                            content,
                            is_error,
                        });
                    }
                }
                continue;
            }
            let plan_decision = match crate::ai::tool_policy::authorize_plan(
                pool,
                tool_name,
                tool_input,
                &provenance,
            )
            .await
            {
                Ok(decision) => decision,
                Err(e) => {
                    let (tool_use, tool_result) = anthropic_recoverable_tool_error(
                        tool_id,
                        tool_name,
                        tool_input,
                        &e.to_string(),
                    );
                    assist_blocks.push(tool_use);
                    result_blocks.push(tool_result);
                    continue;
                }
            };
            if automatic_execution_allowed(plan_decision, parsed_tools.len())
                && (tool_name == "create_product"
                    || !crate::ai::engine::ops::is_registered_operation(tool_name))
            {
                let context = crate::ai::tool_policy::MutationExecutionContext {
                    actor_user_id: input.user_id.clone(),
                    branch_id: input.branch_id.clone(),
                };
                let result = crate::ai::tool_policy::execute_automatic_mutation(
                    pool,
                    &context,
                    tool_name,
                    tool_input,
                    input.currency_exponent,
                    &provenance,
                )
                .await?;
                mutation_executed = true;
                assist_blocks.push(MsgContent::ToolUse {
                    id: tool_id.clone(),
                    name: tool_name.clone(),
                    input: tool_input.clone(),
                });
                result_blocks.push(anthropic_automatic_mutation_result(tool_id, &result));
                send_or_log!(
                    ev,
                    StreamEvent::MutationExecuted {
                        action_id: result.action_id,
                        tool_name: tool_name.clone(),
                        undo_id: result.undo_id,
                        description: result.description,
                    }
                );
                continue;
            }
            // ── Engine ops ────────────────────────────────────────────────────
            if crate::ai::engine::ops::is_registered_operation(tool_name) {
                if let Some(EngineTurnDecision::RecoverableError(message)) =
                    engine_preflight.remove(tool_id)
                {
                    let (tool_use, tool_result) =
                        anthropic_recoverable_tool_error(tool_id, tool_name, tool_input, &message);
                    assist_blocks.push(tool_use);
                    result_blocks.push(tool_result);
                    continue;
                }
                use crate::ai::engine::{runs, selector::Selector};
                let registry = crate::ai::engine::ops::operation_registry();

                if let Some(op) = registry.find(tool_name) {
                    match op.validate(pool, tool_input).await {
                        Err(errs) => {
                            let msg = format!("Validation failed: {}", errs.join("; "));
                            let (tool_use, tool_result) = anthropic_recoverable_tool_error(
                                tool_id, tool_name, tool_input, &msg,
                            );
                            assist_blocks.push(tool_use);
                            result_blocks.push(tool_result);
                            continue;
                        }
                        Ok(()) => {
                            let is_single = tool_name == "create_product";
                            let (preview, run_id, count): (Preview, Option<String>, i64) =
                                if is_single {
                                    let p = op.preview(pool, tool_input).await?;
                                    (p, None, 1)
                                } else {
                                    let selector: Selector = serde_json::from_value(
                                        tool_input.get("selector").cloned().unwrap_or_default(),
                                    )
                                    .map_err(|e| {
                                        let msg = format!("Invalid selector: {e}");
                                        send_or_log!(
                                            ev,
                                            StreamEvent::Error {
                                                message: msg.clone()
                                            }
                                        );
                                        AppError::Validation(msg)
                                    })?;
                                    selector.validate_for_mutation()?;
                                    let count = selector.count(pool).await?;
                                    let maximum =
                                        crate::ai::engine::batch::max_bulk_affected(tool_name);
                                    if count == 0 || count > maximum {
                                        return Err(AppError::Validation(format!(
                                        "Bulk mutation must match 1..={maximum} records; matched {count}"
                                    )));
                                    }
                                    let p = op.preview(pool, tool_input).await?;
                                    let selector_json =
                                        serde_json::to_string(&selector).unwrap_or_default();
                                    let params_json =
                                        serde_json::to_string(tool_input).unwrap_or_default();
                                    let rid = runs::create_run(
                                        pool,
                                        tool_name,
                                        &selector_json,
                                        &params_json,
                                        count,
                                        &input.user_id,
                                        &input.branch_id,
                                    )
                                    .await?;
                                    (p, Some(rid), count)
                                };
                            if let Some(rid) = run_id {
                                send_or_log!(
                                    ev,
                                    StreamEvent::RunPreview {
                                        run_id: rid,
                                        op_id: tool_name.clone(),
                                        description: preview.description,
                                        count,
                                        samples: preview.samples,
                                        requires_confirmation: plan_decision
                                            == crate::ai::tool_policy::PlanDecision::ConfirmationRequired,
                                    }
                                );
                            } else {
                                let tool_input_json = tool_input.to_string();
                                let action = ai_admin_repo::create_action(
                                    pool,
                                    &input.user_id,
                                    &input.branch_id,
                                    tool_name,
                                    &tool_input_json,
                                    &hash_str(&tool_input_json),
                                    &preview.description,
                                    &Ulid::new().to_string(),
                                    params.action_expiry_minutes,
                                )
                                .await?;
                                send_or_log!(
                                    ev,
                                    StreamEvent::MutationPending {
                                        action_id: action.action_id,
                                        tool_name: tool_name.clone(),
                                        preview: ToolPreview {
                                            tool_name: tool_name.into(),
                                            description: preview.description.clone(),
                                            fields: vec![ToolPreviewField {
                                                label: "Action".into(),
                                                value: preview.description.clone(),
                                            }],
                                        },
                                        expires_at: action.expires_at,
                                        assistant_text: format!(
                                            "I'd like to {}.",
                                            preview.description
                                        ),
                                    }
                                );
                            }
                        }
                    }
                } else {
                    let msg = format!("Unknown engine operation: {tool_name}");
                    send_or_log!(ev, StreamEvent::Error { message: msg });
                }
                send_or_log!(ev, StreamEvent::Done);
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                record_turn(
                    pool,
                    session_id,
                    turn_num as i32 + 1,
                    0,
                    turn_output_tokens as i32,
                    turn_ms,
                    provider_name,
                    model_name,
                );
                return Ok(accumulated_text);
            }

            // ── Intent Engine dispatch ─────────────────────────────────────
            if crate::ai::intent_engine::INTENT_NAMES.contains(&tool_name.as_str())
                && !crate::ai::intent_engine::is_mutation_intent(tool_name)
            {
                if crate::ai::intent_engine::is_mutation_intent(tool_name) {
                    let preview_text = format!("{} with params: {}", tool_name, tool_input);
                    let action = ai_admin_repo::create_action(
                        pool,
                        &input.user_id,
                        &input.branch_id,
                        tool_name,
                        &tool_input.to_string(),
                        &hash_str(&tool_input.to_string()),
                        &preview_text,
                        &Ulid::new().to_string(),
                        params.action_expiry_minutes,
                    )
                    .await?;
                    send_or_log!(
                        ev,
                        StreamEvent::MutationPending {
                            action_id: action.action_id,
                            tool_name: tool_name.clone(),
                            preview: crate::domain::ai_admin::ToolPreview {
                                tool_name: tool_name.clone(),
                                description: preview_text,
                                fields: vec![],
                            },
                            expires_at: action.expires_at,
                            assistant_text: turn_text.clone(),
                        }
                    );
                    record_turn(
                        pool,
                        session_id,
                        turn_num as i32 + 1,
                        0,
                        turn_output_tokens as i32,
                        turn_ms,
                        provider_name,
                        model_name,
                    );
                    return Ok(accumulated_text);
                }
                // Read intent — execute directly
                let result = crate::ai::intent_engine::execute_intent(
                    pool,
                    tool_name,
                    tool_input,
                    &input.branch_id,
                )
                .await;
                match result {
                    Ok(r) => {
                        let result_text =
                            serde_json::to_string(&r.data).unwrap_or_else(|_| "{}".into());
                        let result_text = budget_tool_result_if_success(
                            &mut tool_result_budget,
                            tool_name,
                            result_text,
                            false,
                        );
                        assist_blocks.push(MsgContent::ToolUse {
                            id: tool_id.clone(),
                            name: tool_name.clone(),
                            input: tool_input.clone(),
                        });
                        result_blocks.push(MsgContent::ToolResult {
                            tool_use_id: tool_id.clone(),
                            content: result_text,
                            is_error: None,
                        });
                        if tool_name == "open_tab" {
                            if let Some(tab) = tool_input.get("tab").and_then(|v| v.as_str()) {
                                send_or_log!(
                                    ev,
                                    StreamEvent::Navigate {
                                        tab: tab.to_string()
                                    }
                                );
                            }
                        }
                    }
                    Err(e) => {
                        let msg = format!("Intent '{}' failed: {e}", tool_name);
                        send_or_log!(
                            ev,
                            StreamEvent::Error {
                                message: msg.clone()
                            }
                        );
                        record_turn(
                            pool,
                            session_id,
                            turn_num as i32 + 1,
                            0,
                            turn_output_tokens as i32,
                            turn_ms,
                            provider_name,
                            model_name,
                        );
                        return Err(AppError::Internal(msg));
                    }
                }
                continue;
            }

            // ── Mutations always require an explicit user confirmation ─────
            if tools::is_mutation_tool(tool_name) {
                let preview =
                    tools::dry_run_mutation(pool, tool_name, tool_input, input.currency_exponent)
                        .await?;
                let tool_input_json = tool_input.to_string();
                let preview_text = preview_to_text(&preview);
                let action = ai_admin_repo::create_action(
                    pool,
                    &input.user_id,
                    &input.branch_id,
                    tool_name,
                    &tool_input_json,
                    &hash_str(&tool_input_json),
                    &preview_text,
                    &Ulid::new().to_string(),
                    params.action_expiry_minutes,
                )
                .await?;
                pending_mutations.push(BatchPendingAction {
                    action_id: action.action_id,
                    tool_name: tool_name.clone(),
                    preview,
                    expires_at: action.expires_at,
                });
                continue;
            }

            // ── Read tools ─────────────────────────────────────────────────
            let read_end = plain_read_run_end(&parsed_tools, tool_index);
            let prepared = execute_plain_read_batch(
                pool,
                parsed_tools[tool_index..read_end].to_vec(),
                &input.branch_id,
                input.currency_exponent,
                actor_role,
            )
            .await;
            prepared_reads.extend(prepared);
            match prepared_reads
                .remove(tool_id)
                .expect("current read must be present in its prepared batch")
            {
                PreparedRead::AuthorizationError(message) => {
                    let (tool_use, tool_result) =
                        anthropic_recoverable_tool_error(tool_id, tool_name, tool_input, &message);
                    assist_blocks.push(tool_use);
                    result_blocks.push(tool_result);
                }
                PreparedRead::Result { content, is_error } => {
                    let content = budget_tool_result_if_success(
                        &mut tool_result_budget,
                        tool_name,
                        content,
                        is_error.unwrap_or(false),
                    );
                    if tool_name == "request_full_tool_access" {
                        full_tool_access = true;
                        tracing::info!("ZanAI tool catalogue widened for the next step");
                    }
                    if let Some(event) =
                        ui_event_for_read_tool(tool_name, tool_input, &content, is_error)
                    {
                        form_shown |= matches!(event, StreamEvent::FormRequest { .. });
                        send_or_log!(ev, event);
                    }
                    assist_blocks.push(MsgContent::ToolUse {
                        id: tool_id.clone(),
                        name: tool_name.clone(),
                        input: tool_input.clone(),
                    });
                    result_blocks.push(MsgContent::ToolResult {
                        tool_use_id: tool_id.clone(),
                        content,
                        is_error,
                    });
                }
            }
        }

        // A form on screen ends the turn. The stop is what makes the guarantee
        // real: whatever the tool result told the model, it gets no further
        // step in which to act on values the operator has not supplied yet.
        if form_shown {
            send_or_log!(ev, StreamEvent::Done);
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            record_turn(
                pool,
                session_id,
                turn_num as i32 + 1,
                0,
                turn_output_tokens as i32,
                turn_ms,
                provider_name,
                model_name,
            );
            return Ok(accumulated_text);
        }

        // Emit all accumulated high-risk pending mutations, then stop this turn.
        if !pending_mutations.is_empty() {
            if pending_mutations.len() == 1 {
                let m = pending_mutations.remove(0);
                send_or_log!(
                    ev,
                    StreamEvent::MutationPending {
                        action_id: m.action_id,
                        tool_name: m.tool_name,
                        preview: m.preview,
                        expires_at: m.expires_at,
                        assistant_text: turn_text.clone(),
                    }
                );
            } else {
                send_or_log!(
                    ev,
                    StreamEvent::MutationBatchPending {
                        actions: pending_mutations,
                        assistant_text: turn_text.clone(),
                    }
                );
            }
            record_turn(
                pool,
                session_id,
                turn_num as i32 + 1,
                0,
                turn_output_tokens as i32,
                turn_ms,
                provider_name,
                model_name,
            );
            return Ok(accumulated_text);
        }

        // Push the combined assistant + user message pair for this turn.
        msgs.push(AnthropicMsg {
            role: "assistant".into(),
            content: assist_blocks,
        });
        msgs.push(AnthropicMsg {
            role: "user".into(),
            content: result_blocks,
        });
        record_turn(
            pool,
            session_id,
            turn_num as i32 + 1,
            0,
            turn_output_tokens as i32,
            turn_ms,
            provider_name,
            model_name,
        );
    }

    // Exhausted max_turns — emit a continuation note (not an error) so the user
    // can ask the AI to continue. The AI has made progress; it's not a failure.
    let max_turns = params.max_turns;
    tracing::warn!(
        "AI streaming hit max_turns ({max_turns}) tool-loop — emitting continuation note"
    );
    send_or_log!(
        ev,
        StreamEvent::Token {
            text: format!(
                "\n\n_I've used all {max_turns} steps available in this request. \
             If there's more to do, just say **\"continue\"** and I'll pick up where I left off._"
            ),
        }
    );
    send_or_log!(ev, StreamEvent::Done);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    Ok(accumulated_text)
}
// ── End of Anthropic streaming function ──────────────────────────────────────

// ── OpenAI streaming function ──────────────────────────────────────────────────

/// Drive a full multi-turn tool-loop via OpenAI SSE streaming, emitting
/// StreamEvent tokens to the channel. Same dispatch logic as the Anthropic path
/// but uses OpenAIMessage types and the OpenAI SSE wire format.
pub async fn run_streaming_chat_openai(
    pool: &SqlitePool,
    client: &OpenAIClient,
    system: &str,
    input: &AiChatInput,
    tool_defs: &[crate::ai::client::ToolDef],
    on_event: &Channel<StreamEvent>,
    mutation_tracker: &std::sync::atomic::AtomicBool,
    session_id: &str,
    provider_name: &str,
    model_name: &str,
    tool_subsetting_enabled: bool,
    actor_role: &str,
) -> AppResult<String> {
    let ev = on_event;
    let params = load_ai_params(pool).await;
    let mut msgs = build_openai_messages(input, params.context_window_chars);
    let mut accumulated_text = String::new();
    let mut provenance = crate::ai::tool_policy::ProvenanceState::default();
    let mut full_tool_access = false;
    // Share the same request-wide result ceiling as the Anthropic path.
    let mut tool_result_budget = ToolResultBudget::new(
        params.tool_result_max_chars,
        params.turn_tool_results_max_chars,
    );
    // Auto-continue budget for text turns cut off by the output-token limit
    // (finish_reason == "length"). Bounded so a runaway model can't loop.
    let mut length_continuations: u8 = 0;

    let record_turn = |pool: &SqlitePool,
                       sid: &str,
                       turn: i32,
                       tok_in: i32,
                       tok_out: i32,
                       lat: i32,
                       pn: &str,
                       mn: &str| {
        let pool = pool.clone();
        let sid = sid.to_string();
        let pn = pn.to_string();
        let mn = mn.to_string();
        tokio::spawn(async move {
            if let Err(e) =
                ai_admin_repo::record_usage(&pool, &sid, turn, tok_in, tok_out, lat, &pn, &mn).await
            {
                tracing::error!("record_turn failed (OpenAI): session={sid} turn={turn} err={e}");
            }
        });
    };

    for turn_num in 0..params.max_turns {
        let turn_start = Instant::now();
        // Wrap channel in Arc so closures share the same underlying IPC sender
        let ev = std::sync::Arc::new(on_event.clone());
        let ev_tok = ev.clone();
        let ev_start = ev.clone();

        let policy_definitions =
            crate::ai::tool_policy::definitions_for_request(tool_defs, &provenance)?;
        let subset = crate::ai::tool_subsetting::subset_for_message(
            &policy_definitions,
            &input.message,
            tool_subsetting_enabled,
            full_tool_access,
        )?;
        if tool_subsetting_enabled {
            tracing::info!(
                applied = subset.applied,
                omitted_mutations = subset.omitted_mutations,
                omitted_reads = subset.omitted_reads,
                kept = subset.definitions.len(),
                domains = ?subset.domains,
                widened = full_tool_access,
                "ZanAI mutation-tool subsetting decision"
            );
        }
        let turn_tool_defs = subset.definitions;
        // Box::pin: keep the per-turn SSE future off the stack — combined with
        // fat LTO this frame previously contributed to a 0xc00000fd overflow.
        let result = match tokio::time::timeout(
            std::time::Duration::from_secs(params.stream_timeout_secs.max(30) as u64),
            Box::pin(client.send_stream(
                system,
                msgs.clone(),
                &turn_tool_defs,
                mutation_tracker.load(std::sync::atomic::Ordering::Acquire),
                move |token| {
                    send_or_log!(ev_tok, StreamEvent::Token { text: token });
                },
                move |_id, name| {
                    send_or_log!(ev_start, StreamEvent::ToolStart { name });
                },
                |_delta| {}, // silently accumulate tool arg fragments
            )),
        )
        .await
        {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                send_or_log!(
                    ev,
                    StreamEvent::Error {
                        message: e.to_string()
                    }
                );
                return Err(e);
            }
            Err(_elapsed) => {
                tracing::error!(
                    "send_stream: top-level timeout after {}s",
                    params.stream_timeout_secs
                );
                let message =
                    "The AI request reached its total time limit. Retry the request.".to_string();
                send_or_log!(
                    ev,
                    StreamEvent::Error {
                        message: message.clone()
                    }
                );
                return Err(AppError::Internal(message));
            }
        };

        let turn_output_tokens: u32 = result
            .usage
            .as_ref()
            .map(|u| u.completion_tokens as u32)
            .unwrap_or(0);
        accumulated_text.push_str(&result.text);

        if result.tool_calls.is_empty() {
            // ── Auto-continue: text answer was truncated by max_tokens ──────
            if result.finish_reason == "length" && length_continuations < 2 {
                length_continuations += 1;
                tracing::info!(
                    attempt = length_continuations,
                    "send_stream: finish_reason=length with no tool calls — auto-continuing"
                );
                msgs.push(assistant_msg(&result.text));
                msgs.push(user_msg(
                    "Your previous reply was cut off by the output token limit. \
                     Continue EXACTLY where you stopped — do not repeat anything.",
                ));
                let turn_ms = turn_start.elapsed().as_millis() as i32;
                record_turn(
                    pool,
                    session_id,
                    turn_num as i32 + 1,
                    0,
                    turn_output_tokens as i32,
                    turn_ms,
                    provider_name,
                    model_name,
                );
                continue;
            }
            send_or_log!(ev, StreamEvent::Done);
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let turn_ms = turn_start.elapsed().as_millis() as i32;
            record_turn(
                pool,
                session_id,
                turn_num as i32 + 1,
                0,
                turn_output_tokens as i32,
                turn_ms,
                provider_name,
                model_name,
            );
            return Ok(accumulated_text);
        }

        let turn_text = result.text.clone();
        let reasoning = result.reasoning_content.clone();

        // Notify frontend for each tool, then process.
        for tc in &result.tool_calls {
            send_or_log!(
                ev,
                StreamEvent::ToolDone {
                    name: tc.name.clone()
                }
            );
        }

        // Accumulate tool calls for a single assistant message + individual result messages.
        let mut acc_tool_calls: Vec<crate::ai::openai_client::OpenAIToolCall> = vec![];
        let mut acc_results: Vec<OpenAIMessage> = vec![];
        let mut pending_mutations: Vec<BatchPendingAction> = Vec::new();
        for tool_call in &result.tool_calls {
            crate::ai::tool_policy::require_role_allows_tool(actor_role, &tool_call.name)?;
        }
        // Same rule as the Anthropic loop above: a turn that asks for values
        // does not get to write them. See `writes_data` / `FORM_TURN_BLOCKED`.
        let form_requested = result
            .tool_calls
            .iter()
            .any(|call| call.name == "request_input");
        let mut form_shown = false;
        let engine_calls: Vec<_> = result
            .tool_calls
            .iter()
            .filter(|call| crate::ai::engine::ops::is_registered_operation(&call.name))
            .map(|call| (call.id.clone(), call.name.clone(), call.input.clone()))
            .collect();
        let mut engine_preflight =
            preflight_engine_turn(pool, engine_calls, result.tool_calls.len()).await;

        if result
            .tool_calls
            .iter()
            .any(|call| crate::ai::tool_policy::is_external_content_tool(&call.name))
        {
            provenance.mark_external("external tool result in current request");
        }
        let mut prepared_reads = std::collections::HashMap::new();
        for (tool_index, tc) in result.tool_calls.iter().enumerate() {
            let tool_name = &tc.name;
            let tool_id = &tc.id;
            let tool_input = &tc.input;

            if form_requested && writes_data(tool_name) {
                let (tool_call, tool_result) =
                    openai_recoverable_tool_error(tool_id, tool_name, tool_input, FORM_TURN_BLOCKED);
                acc_tool_calls.push(tool_call);
                acc_results.push(tool_result);
                continue;
            }

            // ── Malformed arguments (truncated stream / broken model JSON) ──
            // Feed the parse error back as a recoverable tool result so the
            // model re-issues the call, instead of failing the whole turn with
            // "Provider tool call had invalid JSON arguments".
            if let Some(parse_err) = &tc.parse_error {
                let msg = format!(
                    "The arguments for this tool call did not arrive as valid JSON ({parse_err}). \
                     This usually means the response was cut off by the output token limit. \
                     Re-issue this tool call with complete JSON. If you are creating many items, \
                     send fewer tool calls per turn and continue in the next turn."
                );
                let (tool_call, tool_result) =
                    openai_recoverable_tool_error(tool_id, tool_name, tool_input, &msg);
                acc_tool_calls.push(tool_call);
                acc_results.push(tool_result);
                continue;
            }

            if let Some(prepared) = prepared_reads.remove(tool_id) {
                match prepared {
                    PreparedRead::AuthorizationError(message) => {
                        let (tool_call, tool_result) =
                            openai_recoverable_tool_error(tool_id, tool_name, tool_input, &message);
                        acc_tool_calls.push(tool_call);
                        acc_results.push(tool_result);
                    }
                    PreparedRead::Result { content, is_error } => {
                        let content = budget_tool_result_if_success(
                            &mut tool_result_budget,
                            tool_name,
                            content,
                            is_error.unwrap_or(false),
                        );
                        if tool_name == "request_full_tool_access" {
                            full_tool_access = true;
                            tracing::info!("ZanAI tool catalogue widened for the next step");
                        }
                        if let Some(event) =
                            ui_event_for_read_tool(tool_name, tool_input, &content, is_error)
                        {
                            form_shown |= matches!(event, StreamEvent::FormRequest { .. });
                            send_or_log!(ev, event);
                        }
                        acc_tool_calls.push(crate::ai::openai_client::OpenAIToolCall {
                            id: tool_id.clone(),
                            kind: "function".into(),
                            function: crate::ai::openai_client::OpenAIToolCallFunction {
                                name: tool_name.clone(),
                                arguments: tool_input.to_string(),
                            },
                        });
                        acc_results.push(tool_result_msg(tool_id, content));
                    }
                }
                continue;
            }

            let plan_decision = match crate::ai::tool_policy::authorize_plan(
                pool,
                tool_name,
                tool_input,
                &provenance,
            )
            .await
            {
                Ok(decision) => decision,
                Err(e) => {
                    let (tool_call, tool_result) = openai_recoverable_tool_error(
                        tool_id,
                        tool_name,
                        tool_input,
                        &e.to_string(),
                    );
                    acc_tool_calls.push(tool_call);
                    acc_results.push(tool_result);
                    continue;
                }
            };
            if automatic_execution_allowed(plan_decision, result.tool_calls.len())
                && (tool_name == "create_product"
                    || !crate::ai::engine::ops::is_registered_operation(tool_name))
            {
                let context = crate::ai::tool_policy::MutationExecutionContext {
                    actor_user_id: input.user_id.clone(),
                    branch_id: input.branch_id.clone(),
                };
                let automatic = crate::ai::tool_policy::execute_automatic_mutation(
                    pool,
                    &context,
                    tool_name,
                    tool_input,
                    input.currency_exponent,
                    &provenance,
                )
                .await?;
                mutation_tracker.store(true, std::sync::atomic::Ordering::Release);
                acc_tool_calls.push(crate::ai::openai_client::OpenAIToolCall {
                    id: tool_id.clone(),
                    kind: "function".into(),
                    function: crate::ai::openai_client::OpenAIToolCallFunction {
                        name: tool_name.clone(),
                        arguments: tool_input.to_string(),
                    },
                });
                acc_results.push(openai_automatic_mutation_result(tool_id, &automatic));
                send_or_log!(
                    ev,
                    StreamEvent::MutationExecuted {
                        action_id: automatic.action_id,
                        tool_name: tool_name.clone(),
                        undo_id: automatic.undo_id,
                        description: automatic.description,
                    }
                );
                continue;
            }

            // ── Engine ops ─────────────────────────────────────────────────
            if crate::ai::engine::ops::is_registered_operation(tool_name) {
                if let Some(EngineTurnDecision::RecoverableError(message)) =
                    engine_preflight.remove(tool_id)
                {
                    let (tool_call, tool_result) =
                        openai_recoverable_tool_error(tool_id, tool_name, tool_input, &message);
                    acc_tool_calls.push(tool_call);
                    acc_results.push(tool_result);
                    continue;
                }
                use crate::ai::engine::{runs, selector::Selector};
                let registry = crate::ai::engine::ops::operation_registry();

                if let Some(op) = registry.find(tool_name) {
                    match op.validate(pool, tool_input).await {
                        Err(errs) => {
                            let msg = format!("Validation failed: {}", errs.join("; "));
                            let (tool_call, tool_result) =
                                openai_recoverable_tool_error(tool_id, tool_name, tool_input, &msg);
                            acc_tool_calls.push(tool_call);
                            acc_results.push(tool_result);
                            continue;
                        }
                        Ok(()) => {
                            let is_single = tool_name == "create_product";
                            let (preview, run_id, count): (Preview, Option<String>, i64) =
                                if is_single {
                                    let p = op.preview(pool, tool_input).await?;
                                    (p, None, 1)
                                } else {
                                    let selector: Selector = serde_json::from_value(
                                        tool_input.get("selector").cloned().unwrap_or_default(),
                                    )
                                    .map_err(|e| {
                                        let msg = format!("Invalid selector: {e}");
                                        send_or_log!(
                                            ev,
                                            StreamEvent::Error {
                                                message: msg.clone()
                                            }
                                        );
                                        AppError::Validation(msg)
                                    })?;
                                    selector.validate_for_mutation()?;
                                    let count = selector.count(pool).await?;
                                    let maximum =
                                        crate::ai::engine::batch::max_bulk_affected(tool_name);
                                    if count == 0 || count > maximum {
                                        return Err(AppError::Validation(format!(
                                        "Bulk mutation must match 1..={maximum} records; matched {count}"
                                    )));
                                    }
                                    let p = op.preview(pool, tool_input).await?;
                                    let selector_json =
                                        serde_json::to_string(&selector).unwrap_or_default();
                                    let params_json =
                                        serde_json::to_string(tool_input).unwrap_or_default();
                                    let rid = runs::create_run(
                                        pool,
                                        tool_name,
                                        &selector_json,
                                        &params_json,
                                        count,
                                        &input.user_id,
                                        &input.branch_id,
                                    )
                                    .await?;
                                    (p, Some(rid), count)
                                };
                            if let Some(rid) = run_id {
                                send_or_log!(
                                    ev,
                                    StreamEvent::RunPreview {
                                        run_id: rid,
                                        op_id: tool_name.clone(),
                                        description: preview.description,
                                        count,
                                        samples: preview.samples,
                                        requires_confirmation: plan_decision
                                            == crate::ai::tool_policy::PlanDecision::ConfirmationRequired,
                                    }
                                );
                            } else {
                                let tool_input_json = tool_input.to_string();
                                let action = ai_admin_repo::create_action(
                                    pool,
                                    &input.user_id,
                                    &input.branch_id,
                                    tool_name,
                                    &tool_input_json,
                                    &hash_str(&tool_input_json),
                                    &preview.description,
                                    &Ulid::new().to_string(),
                                    params.action_expiry_minutes,
                                )
                                .await?;
                                send_or_log!(
                                    ev,
                                    StreamEvent::MutationPending {
                                        action_id: action.action_id,
                                        tool_name: tool_name.clone(),
                                        preview: ToolPreview {
                                            tool_name: tool_name.into(),
                                            description: preview.description.clone(),
                                            fields: vec![ToolPreviewField {
                                                label: "Action".into(),
                                                value: preview.description.clone(),
                                            }],
                                        },
                                        expires_at: action.expires_at,
                                        assistant_text: format!(
                                            "I'd like to {}.",
                                            preview.description
                                        ),
                                    }
                                );
                            }
                        }
                    }
                } else {
                    let msg = format!("Unknown engine operation: {tool_name}");
                    send_or_log!(ev, StreamEvent::Error { message: msg });
                }
                send_or_log!(ev, StreamEvent::Done);
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                let turn_ms = turn_start.elapsed().as_millis() as i32;
                record_turn(
                    pool,
                    session_id,
                    turn_num as i32 + 1,
                    0,
                    turn_output_tokens as i32,
                    turn_ms,
                    provider_name,
                    model_name,
                );
                return Ok(accumulated_text);
            }

            // ── Intent Engine dispatch ─────────────────────────────────────
            if crate::ai::intent_engine::INTENT_NAMES.contains(&tool_name.as_str())
                && !crate::ai::intent_engine::is_mutation_intent(tool_name)
            {
                if crate::ai::intent_engine::is_mutation_intent(tool_name) {
                    let preview_text = format!("{} with params: {}", tool_name, tool_input);
                    let action = ai_admin_repo::create_action(
                        pool,
                        &input.user_id,
                        &input.branch_id,
                        tool_name,
                        &tool_input.to_string(),
                        &hash_str(&tool_input.to_string()),
                        &preview_text,
                        &Ulid::new().to_string(),
                        params.action_expiry_minutes,
                    )
                    .await?;
                    send_or_log!(
                        ev,
                        StreamEvent::MutationPending {
                            action_id: action.action_id,
                            tool_name: tool_name.clone(),
                            preview: ToolPreview {
                                tool_name: tool_name.clone(),
                                description: preview_text,
                                fields: vec![],
                            },
                            expires_at: action.expires_at,
                            assistant_text: turn_text.clone(),
                        }
                    );
                    let turn_ms = turn_start.elapsed().as_millis() as i32;
                    record_turn(
                        pool,
                        session_id,
                        turn_num as i32 + 1,
                        0,
                        turn_output_tokens as i32,
                        turn_ms,
                        provider_name,
                        model_name,
                    );
                    return Ok(accumulated_text);
                }
                // Read intent — execute directly
                let result = crate::ai::intent_engine::execute_intent(
                    pool,
                    tool_name,
                    tool_input,
                    &input.branch_id,
                )
                .await;
                match result {
                    Ok(r) => {
                        let result_text =
                            serde_json::to_string(&r.data).unwrap_or_else(|_| "{}".into());
                        let result_text = budget_tool_result_if_success(
                            &mut tool_result_budget,
                            tool_name,
                            result_text,
                            false,
                        );
                        acc_tool_calls.push(crate::ai::openai_client::OpenAIToolCall {
                            id: tool_id.clone(),
                            kind: "function".into(),
                            function: crate::ai::openai_client::OpenAIToolCallFunction {
                                name: tool_name.clone(),
                                arguments: tool_input.to_string(),
                            },
                        });
                        acc_results.push(tool_result_msg(tool_id, result_text));
                        if tool_name == "open_tab" {
                            if let Some(tab) = tool_input.get("tab").and_then(|v| v.as_str()) {
                                send_or_log!(
                                    ev,
                                    StreamEvent::Navigate {
                                        tab: tab.to_string()
                                    }
                                );
                            }
                        }
                    }
                    Err(e) => {
                        let msg = format!("Intent '{}' failed: {e}", tool_name);
                        send_or_log!(
                            ev,
                            StreamEvent::Error {
                                message: msg.clone()
                            }
                        );
                        let turn_ms = turn_start.elapsed().as_millis() as i32;
                        record_turn(
                            pool,
                            session_id,
                            turn_num as i32 + 1,
                            0,
                            turn_output_tokens as i32,
                            turn_ms,
                            provider_name,
                            model_name,
                        );
                        return Err(AppError::Internal(msg));
                    }
                }
                continue;
            }

            // ── Mutations always require an explicit user confirmation ─────
            if tools::is_mutation_tool(tool_name) {
                let preview =
                    tools::dry_run_mutation(pool, tool_name, tool_input, input.currency_exponent)
                        .await?;
                let tool_input_json = tool_input.to_string();
                let preview_text = preview_to_text(&preview);
                let action = ai_admin_repo::create_action(
                    pool,
                    &input.user_id,
                    &input.branch_id,
                    tool_name,
                    &tool_input_json,
                    &hash_str(&tool_input_json),
                    &preview_text,
                    &Ulid::new().to_string(),
                    params.action_expiry_minutes,
                )
                .await?;
                pending_mutations.push(BatchPendingAction {
                    action_id: action.action_id,
                    tool_name: tool_name.clone(),
                    preview,
                    expires_at: action.expires_at,
                });
                continue;
            }

            // ── Read tools ─────────────────────────────────────────────────
            let read_calls: Vec<ParsedToolCall> = result.tool_calls[tool_index..]
                .iter()
                .take_while(|call| call.parse_error.is_none() && is_plain_read_tool(&call.name))
                .map(|call| (call.id.clone(), call.name.clone(), call.input.clone()))
                .collect();
            let prepared = execute_plain_read_batch(
                pool,
                read_calls,
                &input.branch_id,
                input.currency_exponent,
                actor_role,
            )
            .await;
            prepared_reads.extend(prepared);
            match prepared_reads
                .remove(tool_id)
                .expect("current read must be present in its prepared batch")
            {
                PreparedRead::AuthorizationError(message) => {
                    let (tool_call, tool_result) =
                        openai_recoverable_tool_error(tool_id, tool_name, tool_input, &message);
                    acc_tool_calls.push(tool_call);
                    acc_results.push(tool_result);
                }
                PreparedRead::Result { content, is_error } => {
                    let content = budget_tool_result_if_success(
                        &mut tool_result_budget,
                        tool_name,
                        content,
                        is_error.unwrap_or(false),
                    );
                    if tool_name == "request_full_tool_access" {
                        full_tool_access = true;
                        tracing::info!("ZanAI tool catalogue widened for the next step");
                    }
                    if let Some(event) =
                        ui_event_for_read_tool(tool_name, tool_input, &content, is_error)
                    {
                        form_shown |= matches!(event, StreamEvent::FormRequest { .. });
                        send_or_log!(ev, event);
                    }
                    acc_tool_calls.push(crate::ai::openai_client::OpenAIToolCall {
                        id: tool_id.clone(),
                        kind: "function".into(),
                        function: crate::ai::openai_client::OpenAIToolCallFunction {
                            name: tool_name.clone(),
                            arguments: tool_input.to_string(),
                        },
                    });
                    acc_results.push(tool_result_msg(tool_id, content));
                }
            }
        }

        // A form on screen ends the turn — see the Anthropic loop for why.
        if form_shown {
            send_or_log!(ev, StreamEvent::Done);
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            record_turn(
                pool,
                session_id,
                turn_num as i32 + 1,
                0,
                turn_output_tokens as i32,
                turn_start.elapsed().as_millis() as i32,
                provider_name,
                model_name,
            );
            return Ok(accumulated_text);
        }

        // Emit all accumulated high-risk pending mutations, then stop this turn.
        if !pending_mutations.is_empty() {
            let turn_ms = turn_start.elapsed().as_millis() as i32;
            if pending_mutations.len() == 1 {
                let m = pending_mutations.remove(0);
                send_or_log!(
                    ev,
                    StreamEvent::MutationPending {
                        action_id: m.action_id,
                        tool_name: m.tool_name,
                        preview: m.preview,
                        expires_at: m.expires_at,
                        assistant_text: turn_text.clone(),
                    }
                );
            } else {
                send_or_log!(
                    ev,
                    StreamEvent::MutationBatchPending {
                        actions: pending_mutations,
                        assistant_text: turn_text.clone(),
                    }
                );
            }
            record_turn(
                pool,
                session_id,
                turn_num as i32 + 1,
                0,
                turn_output_tokens as i32,
                turn_ms,
                provider_name,
                model_name,
            );
            return Ok(accumulated_text);
        }

        // Push the combined assistant message + individual tool-result messages.
        msgs.push(OpenAIMessage::Assistant {
            content: None,
            reasoning_content: reasoning.clone(),
            tool_calls: acc_tool_calls,
        });
        msgs.extend(acc_results);
        let turn_ms = turn_start.elapsed().as_millis() as i32;
        record_turn(
            pool,
            session_id,
            turn_num as i32 + 1,
            0,
            turn_output_tokens as i32,
            turn_ms,
            provider_name,
            model_name,
        );
    }

    // max_turns exhausted — same graceful continuation note as Anthropic path.
    let max_turns = params.max_turns;
    tracing::warn!(
        "AI streaming (OpenAI) hit max_turns ({max_turns}) tool-loop — emitting continuation note"
    );
    send_or_log!(
        ev,
        StreamEvent::Token {
            text: format!(
                "\n\n_I've used all {max_turns} steps available in this request. \
             If there's more to do, just say **\"continue\"** and I'll pick up where I left off._"
            ),
        }
    );
    send_or_log!(ev, StreamEvent::Done);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    Ok(accumulated_text)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn build_openai_messages(input: &AiChatInput, max_chars: usize) -> Vec<OpenAIMessage> {
    let history = truncate_history(&input.history, max_chars);
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
    msgs.push(match (&input.image_base64, &input.image_media_type) {
        (Some(data), Some(mt)) if !data.is_empty() => {
            user_msg_with_image(&input.message, format!("data:{mt};base64,{data}"))
        }
        _ => user_msg(&input.message),
    });
    msgs
}

/// The rule that makes an interactive form safe: a turn that asks the operator
/// for values does not get to write them.
///
/// Everything else about forms is tested elsewhere — the spec parser, the
/// widget, the round trip. This is the part with teeth, and it was the one part
/// asserted without proof. Without the guard the model can draw "what price?"
/// and set a guessed price in the same step, and nothing downstream stops it:
/// `update_product_price` is reversible and routine, so the default
/// confirmation policy executes it automatically.
#[cfg(test)]
mod form_turn_tests {
    use super::*;

    fn price_form_input() -> Value {
        serde_json::json!({
            "title": "Update price",
            "fields": [
                { "name": "barcode", "label": "Product barcode", "type": "barcode", "required": true },
                { "name": "new_price", "label": "New price (BHD)", "type": "money", "required": true }
            ]
        })
    }

    #[test]
    fn everything_that_can_change_the_shop_is_recognised_as_a_write() {
        // Plain mutations.
        assert!(writes_data("update_product_price"));
        assert!(writes_data("adjust_stock"));
        assert!(writes_data("delete_product"));
        assert!(writes_data("create_refund"));
        // Engine operations run through a different execution path and would
        // otherwise slip past a check that only knew about MUTATION_TOOLS.
        assert!(writes_data("bulk_price_adjust"));
        assert!(writes_data("create_product"));

        // Reads are untouched: a form turn still has to be able to look things
        // up, or the model cannot prefill the boxes it is drawing.
        assert!(!writes_data("search_products"));
        assert!(!writes_data("get_product"));
        assert!(!writes_data("lookup_barcode"));
        assert!(!writes_data("request_input"));
        assert!(!writes_data("open_tab"));
    }

    /// Read tools whose whole purpose is the screen. A result is consumed in
    /// four places across the two provider loops, so the mapping lives in one
    /// function; these pin what it maps.
    #[test]
    fn a_valid_form_spec_becomes_the_event_the_widget_draws() {
        let input = price_form_input();
        let event = ui_event_for_read_tool("request_input", &input, "{\"ok\":true}", None)
            .expect("no event raised");

        match event {
            StreamEvent::FormRequest { form } => {
                assert_eq!(form.title, "Update price");
                assert_eq!(form.fields.len(), 2);
                assert_eq!(form.fields[0].name, "barcode");
            }
            other => panic!("expected a form request, got {other:?}"),
        }
    }

    #[test]
    fn navigation_still_comes_off_the_tool_result_as_it_always_did() {
        let event = ui_event_for_read_tool(
            "open_tab",
            &serde_json::json!({ "tab": "products" }),
            "{\"ok\":true,\"tab\":\"products\"}",
            None,
        );
        assert!(matches!(event, Some(StreamEvent::Navigate { tab }) if tab == "products"));
    }

    /// A spec the model got wrong must leave the turn running so it can correct
    /// itself. If a failed form still ended the turn, the operator would be
    /// left looking at nothing with no way to ask again.
    #[test]
    fn a_rejected_spec_raises_no_event_so_the_turn_carries_on() {
        // Empty: no fields, no choices, no table — nothing to submit.
        let empty = serde_json::json!({ "title": "Hmm" });
        assert!(ui_event_for_read_tool("request_input", &empty, "{}", None).is_none());

        // A failed tool result never raises a UI event either, whatever the
        // input said.
        assert!(
            ui_event_for_read_tool("request_input", &price_form_input(), "boom", Some(true))
                .is_none()
        );
        assert!(ui_event_for_read_tool(
            "open_tab",
            &serde_json::json!({ "tab": "products" }),
            "Tool 'open_tab' failed",
            Some(true)
        )
        .is_none());
    }

    #[test]
    fn ordinary_read_tools_raise_nothing() {
        for name in ["search_products", "get_product", "get_today_summary"] {
            assert!(
                ui_event_for_read_tool(name, &serde_json::json!({}), "some result", None).is_none(),
                "{name} raised a UI event"
            );
        }
    }

    /// The message the model gets back has to say what to do next, or it
    /// retries the same blocked call and burns the turn budget.
    #[test]
    fn the_block_tells_the_model_to_wait_rather_than_retry_immediately() {
        assert!(FORM_TURN_BLOCKED.contains("Wait for their answer"));
        assert!(FORM_TURN_BLOCKED.contains("do not have the values"));
    }
}

#[cfg(test)]
mod recoverable_tool_error_tests {
    use super::*;

    async fn preflight_pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO categories (category_id,name,sort_order,is_active,created_at,updated_at) VALUES ('cat-valid','Chocolate',0,1,?,?)")
            .bind(&now)
            .bind(&now)
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    fn product_input(category_id: &str) -> Value {
        serde_json::json!({
            "name": "Kinder Riegel 21g",
            "category_id": category_id,
            "price_minor": 150
        })
    }

    #[test]
    fn anthropic_policy_rejection_is_a_paired_error_tool_result() {
        let input = serde_json::json!({"query":"drinks"});
        let (tool_use, tool_result) = anthropic_recoverable_tool_error(
            "tool-1",
            "list_categories",
            &input,
            "Invalid input: unknown field 'query'",
        );

        assert!(matches!(
            tool_use,
            MsgContent::ToolUse { ref id, ref name, ref input }
                if id == "tool-1" && name == "list_categories" && input == &serde_json::json!({"query":"drinks"})
        ));
        assert!(matches!(
            tool_result,
            MsgContent::ToolResult { ref tool_use_id, ref content, is_error: Some(true) }
                if tool_use_id == "tool-1"
                    && content.contains("blocked by execution policy")
                    && content.contains("Correct the tool arguments and retry once")
        ));
    }

    #[test]
    fn openai_policy_rejection_is_a_paired_tool_message() {
        let input = serde_json::json!({"query":"drinks"});
        let (tool_call, tool_result) = openai_recoverable_tool_error(
            "tool-1",
            "list_categories",
            &input,
            "Invalid input: unknown field 'query'",
        );

        assert_eq!(tool_call.id, "tool-1");
        assert_eq!(tool_call.function.name, "list_categories");
        assert_eq!(tool_call.function.arguments, input.to_string());
        assert!(matches!(
            tool_result,
            OpenAIMessage::Tool { ref tool_call_id, ref content }
                if tool_call_id == "tool-1"
                    && content.contains("blocked by execution policy")
                    && content.contains("Correct the tool arguments and retry once")
        ));
    }

    #[test]
    fn anthropic_invalid_product_category_points_model_to_grounded_retry() {
        let input = serde_json::json!({
            "name": "Kinder Riegel 21g",
            "price_minor": 150,
            "category_id": "fabricated-category"
        });
        let (_tool_use, tool_result) = anthropic_recoverable_tool_error(
            "tool-2",
            "create_product",
            &input,
            "Validation failed: category fabricated-category not found",
        );

        assert!(matches!(
            tool_result,
            MsgContent::ToolResult { ref content, is_error: Some(true), .. }
                if content.contains("list_categories")
                    && content.contains("retry create_product")
        ));
    }

    #[test]
    fn openai_invalid_product_category_points_model_to_grounded_retry() {
        let input = serde_json::json!({
            "name": "Kinder Riegel 21g",
            "price_minor": 150,
            "category_id": "stale-category"
        });
        let (_tool_call, tool_result) = openai_recoverable_tool_error(
            "tool-2",
            "create_product",
            &input,
            "Validation failed: category stale-category not found",
        );

        assert!(matches!(
            tool_result,
            OpenAIMessage::Tool { ref content, .. }
                if content.contains("list_categories")
                    && content.contains("retry create_product")
        ));
    }

    #[test]
    fn anthropic_automatic_mutation_result_captures_undo_and_continues_as_tool_result() {
        let result = crate::ai::tool_policy::AutomaticMutationResult {
            action_id: "action-1".into(),
            undo_id: Some("undo-1".into()),
            description: "Price updated".into(),
        };

        let message = anthropic_automatic_mutation_result("tool-1", &result);

        assert!(matches!(
            message,
            MsgContent::ToolResult { ref tool_use_id, ref content, is_error: None }
                if tool_use_id == "tool-1"
                    && content.contains("Price updated")
                    && content.contains("undo-1")
        ));
    }

    #[test]
    fn openai_automatic_mutation_result_captures_undo_and_continues_as_tool_result() {
        let result = crate::ai::tool_policy::AutomaticMutationResult {
            action_id: "action-1".into(),
            undo_id: Some("undo-1".into()),
            description: "Product created".into(),
        };

        let message = openai_automatic_mutation_result("tool-1", &result);

        assert!(matches!(
            message,
            OpenAIMessage::Tool { ref tool_call_id, ref content }
                if tool_call_id == "tool-1"
                    && content.contains("Product created")
                    && content.contains("undo-1")
        ));
    }

    #[test]
    fn automatic_mutations_are_allowed_for_multi_tool_turns() {
        assert!(automatic_execution_allowed(
            crate::ai::tool_policy::PlanDecision::AutomaticEligible,
            28
        ));
    }

    #[tokio::test]
    async fn anthropic_preflight_pairs_every_engine_call_for_all_parallel_orderings() {
        let pool = preflight_pool().await;
        for categories in [
            ["cat-valid", "cat-stale"],
            ["cat-valid", "cat-valid"],
            ["cat-stale", "cat-valid"],
        ] {
            let calls: Vec<_> = categories
                .iter()
                .enumerate()
                .map(|(index, category)| {
                    (
                        format!("anthropic-{index}"),
                        "create_product".to_string(),
                        product_input(category),
                    )
                })
                .collect();
            let mut plan = preflight_engine_turn(&pool, calls, 2).await;
            let mut paired_ids = Vec::new();
            for index in 0..2 {
                let id = format!("anthropic-{index}");
                let decision = plan.remove(&id).unwrap();
                let EngineTurnDecision::RecoverableError(message) = decision else {
                    panic!("parallel engine operation was allowed to execute");
                };
                let (_tool_use, result) = anthropic_recoverable_tool_error(
                    &id,
                    "create_product",
                    &serde_json::json!({}),
                    &message,
                );
                if let MsgContent::ToolResult { tool_use_id, .. } = result {
                    paired_ids.push(tool_use_id);
                }
            }
            assert_eq!(paired_ids, ["anthropic-0", "anthropic-1"]);
        }
    }

    #[tokio::test]
    async fn openai_preflight_pairs_every_engine_call_for_all_parallel_orderings() {
        let pool = preflight_pool().await;
        for categories in [
            ["cat-valid", "cat-stale"],
            ["cat-valid", "cat-valid"],
            ["cat-stale", "cat-valid"],
        ] {
            let calls: Vec<_> = categories
                .iter()
                .enumerate()
                .map(|(index, category)| {
                    (
                        format!("openai-{index}"),
                        "create_product".to_string(),
                        product_input(category),
                    )
                })
                .collect();
            let mut plan = preflight_engine_turn(&pool, calls, 2).await;
            let mut paired_ids = Vec::new();
            for index in 0..2 {
                let id = format!("openai-{index}");
                let decision = plan.remove(&id).unwrap();
                let EngineTurnDecision::RecoverableError(message) = decision else {
                    panic!("parallel engine operation was allowed to execute");
                };
                let (_tool_call, result) = openai_recoverable_tool_error(
                    &id,
                    "create_product",
                    &serde_json::json!({}),
                    &message,
                );
                if let OpenAIMessage::Tool { tool_call_id, .. } = result {
                    paired_ids.push(tool_call_id);
                }
            }
            assert_eq!(paired_ids, ["openai-0", "openai-1"]);
        }
    }

    #[tokio::test]
    async fn engine_preflight_preserves_single_valid_operation_execution() {
        let pool = preflight_pool().await;
        let plan = preflight_engine_turn(
            &pool,
            vec![(
                "single".into(),
                "create_product".into(),
                product_input("cat-valid"),
            )],
            1,
        )
        .await;
        assert_eq!(plan.get("single"), Some(&EngineTurnDecision::Execute));
    }
}

fn build_messages(input: &AiChatInput, max_chars: usize) -> Vec<AnthropicMsg> {
    // C-05: apply sliding-window guard — drop oldest pairs if history is too large
    let history = truncate_history(&input.history, max_chars);
    let mut msgs: Vec<AnthropicMsg> = history
        .iter()
        .map(|m| AnthropicMsg {
            role: m.role.clone(),
            content: vec![MsgContent::Text {
                text: m.content.clone(),
            }],
        })
        .collect();
    let mut user_content: Vec<MsgContent> = Vec::new();
    // Image block before text so the model sees the visual before the question
    if let (Some(data), Some(mt)) = (&input.image_base64, &input.image_media_type) {
        if !data.is_empty() {
            user_content.push(MsgContent::Image {
                source: ImageSource {
                    source_type: "base64".into(),
                    media_type: mt.clone(),
                    data: data.clone(),
                },
            });
        }
    }
    user_content.push(MsgContent::Text {
        text: input.message.clone(),
    });
    msgs.push(AnthropicMsg {
        role: "user".into(),
        content: user_content,
    });
    msgs
}

pub(crate) fn append_runtime_context(message: &str, runtime_context: &str) -> String {
    format!("{message}\n\n[ZANPOS_RUNTIME_CONTEXT]\n{runtime_context}\n[/ZANPOS_RUNTIME_CONTEXT]")
}

pub(crate) fn append_capability_context(message: &str, capability_context: &str) -> String {
    format!(
        "{message}\n\n[ZANAI_CAPABILITY_CONTEXT]\n{capability_context}\n[/ZANAI_CAPABILITY_CONTEXT]"
    )
}

fn history_message_cost(message: &ChatMessage) -> usize {
    message
        .content
        .chars()
        .map(|character| {
            if matches!(
                character,
                '\u{0600}'..='\u{06ff}'
                    | '\u{0750}'..='\u{077f}'
                    | '\u{08a0}'..='\u{08ff}'
                    | '\u{fb50}'..='\u{fdff}'
                    | '\u{fe70}'..='\u{feff}'
            ) {
                2
            } else {
                1
            }
        })
        .sum()
}

/// Keep the newest valid suffix within the configured Latin-character budget.
/// Arabic characters count twice because they tokenize more densely. Trimming
/// advances one message at a time and then skips to the next user turn, so a
/// cancelled or odd-length conversation can never leave Anthropic starting
/// with an orphaned assistant message.
fn truncate_history(history: &[ChatMessage], max_chars: usize) -> &[ChatMessage] {
    let mut start = history
        .iter()
        .position(|message| message.role == "user")
        .unwrap_or(history.len());
    let mut running: usize = history[start..].iter().map(history_message_cost).sum();

    while running > max_chars && start < history.len() {
        running = running.saturating_sub(history_message_cost(&history[start]));
        start += 1;
        while start < history.len() && history[start].role != "user" {
            running = running.saturating_sub(history_message_cost(&history[start]));
            start += 1;
        }
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

#[cfg(test)]
mod prompt_cache_tests {
    use super::*;
    use crate::ai::client::ToolDef;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    fn tool(name: &str) -> ToolDef {
        ToolDef {
            name: name.into(),
            description: format!("{name} description"),
            input_schema: serde_json::json!({"type":"object","properties":{}}),
        }
    }

    #[test]
    fn anthropic_marks_the_stable_system_and_last_tool_as_cacheable() {
        let system = anthropic_system_blocks("stable policy");
        let definitions = [tool("first"), tool("last")];
        let tools = anthropic_tools_with_cache_control(&definitions);
        let system_json = serde_json::to_value(system).unwrap();
        let tools_json = serde_json::to_value(tools).unwrap();

        assert_eq!(system_json[0]["text"], "stable policy");
        assert_eq!(system_json[0]["cache_control"]["type"], "ephemeral");
        assert!(tools_json[0].get("cache_control").is_none());
        assert_eq!(tools_json[1]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn runtime_metadata_is_appended_after_the_stable_conversation_prefix() {
        assert_eq!(
            append_runtime_context("How were sales?", r#"{"today":"2026-07-30"}"#),
            "How were sales?\n\n[ZANPOS_RUNTIME_CONTEXT]\n{\"today\":\"2026-07-30\"}\n[/ZANPOS_RUNTIME_CONTEXT]"
        );
    }

    #[test]
    fn capability_context_is_appended_in_its_own_data_only_envelope() {
        assert_eq!(
            append_capability_context(
                "question with runtime context",
                r#"{"tool_count":3,"actor_role":"manager"}"#,
            ),
            "question with runtime context\n\n[ZANAI_CAPABILITY_CONTEXT]\n{\"tool_count\":3,\"actor_role\":\"manager\"}\n[/ZANAI_CAPABILITY_CONTEXT]"
        );
    }

    fn chat(role: &str, content: impl Into<String>) -> ChatMessage {
        ChatMessage {
            role: role.into(),
            content: content.into(),
        }
    }

    #[test]
    fn truncation_handles_odd_history_without_dropping_the_latest_user_turn() {
        let history = vec![
            chat("user", "old user message"),
            chat("assistant", "old assistant message"),
            chat("user", "latest user message"),
        ];

        let kept = truncate_history(&history, 20);

        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].role, "user");
        assert_eq!(kept[0].content, "latest user message");
    }

    #[test]
    fn truncation_discards_leading_assistant_messages() {
        let history = vec![
            chat("assistant", "orphaned reply"),
            chat("user", "valid question"),
            chat("assistant", "valid answer"),
        ];

        let kept = truncate_history(&history, 1_000);

        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].role, "user");
        assert_eq!(kept[1].role, "assistant");
    }

    #[test]
    fn truncation_of_a_single_orphaned_assistant_is_empty() {
        let history = vec![chat("assistant", "cancelled response")];

        assert!(truncate_history(&history, 1_000).is_empty());
    }

    #[test]
    fn arabic_heavy_history_is_trimmed_conservatively() {
        let history = vec![
            chat("user", "م".repeat(300)),
            chat("assistant", "ر".repeat(300)),
            chat("user", "آخر سؤال"),
        ];

        let kept = truncate_history(&history, 700);

        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].role, "user");
        assert_eq!(kept[0].content, "آخر سؤال");
    }

    #[test]
    fn read_runs_stop_at_mutations_and_resume_after_them() {
        let calls = vec![
            ("1".into(), "report_sales".into(), Value::Null),
            ("2".into(), "get_products".into(), Value::Null),
            ("3".into(), "create_product".into(), Value::Null),
            ("4".into(), "get_customers".into(), Value::Null),
        ];

        assert_eq!(plain_read_run_end(&calls, 0), 2);
        assert_eq!(plain_read_run_end(&calls, 2), 2);
        assert_eq!(plain_read_run_end(&calls, 3), 4);
    }

    #[test]
    fn successful_provider_results_share_the_runtime_budget() {
        let mut budget = crate::ai::result_budget::ToolResultBudget::new(4, 6);

        let anthropic =
            budget_tool_result_if_success(&mut budget, "anthropic_read", "abcdef".into(), false);
        let openai =
            budget_tool_result_if_success(&mut budget, "openai_read", "wxyz".into(), false);

        assert!(anthropic.contains("per_result_limit"));
        assert!(openai.contains("turn_limit"));
    }

    #[test]
    fn provider_error_results_are_not_truncated() {
        let mut budget = crate::ai::result_budget::ToolResultBudget::new(4, 4);
        let error = "a detailed authorization error".to_string();

        assert_eq!(
            budget_tool_result_if_success(&mut budget, "blocked", error.clone(), true),
            error
        );
    }

    #[tokio::test]
    async fn bounded_collection_preserves_order_and_caps_concurrency() {
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let results = collect_bounded_ordered(0usize..8, 3, {
            let active = active.clone();
            let peak = peak.clone();
            move |value| {
                let active = active.clone();
                let peak = peak.clone();
                async move {
                    let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_millis((8 - value) as u64)).await;
                    active.fetch_sub(1, Ordering::SeqCst);
                    value
                }
            }
        })
        .await;

        assert_eq!(results, (0usize..8).collect::<Vec<_>>());
        assert!(peak.load(Ordering::SeqCst) <= 3);
        assert!(peak.load(Ordering::SeqCst) > 1);
    }
}

fn hash_str(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}
