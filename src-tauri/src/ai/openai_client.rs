use crate::ai::client::ToolDef;
use crate::errors::{AppError, AppResult};
use futures::StreamExt;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const MAX_PROVIDER_ATTEMPTS: u8 = 3;
const PROVIDER_IDLE_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, Copy)]
pub(crate) struct ProviderTimeouts {
    pub total: Duration,
    pub idle: Duration,
}

pub(crate) fn provider_timeouts(total: Duration) -> ProviderTimeouts {
    ProviderTimeouts {
        total,
        idle: PROVIDER_IDLE_TIMEOUT,
    }
}

pub(crate) fn should_retry_provider_request(
    status: Option<u16>,
    connection_error: bool,
    mutation_executed: bool,
    attempt: u8,
) -> bool {
    if mutation_executed || attempt >= MAX_PROVIDER_ATTEMPTS {
        return false;
    }
    connection_error || status.is_some_and(|code| matches!(code, 429 | 500 | 502 | 503 | 504))
}

pub(crate) fn provider_retry_delay(
    attempt: u8,
    retry_after: Option<Duration>,
    jitter_ms: u64,
) -> Duration {
    if let Some(retry_after) = retry_after {
        return retry_after;
    }
    let exponent = u32::from(attempt.saturating_sub(1).min(6));
    let base_ms = 500_u64.saturating_mul(1_u64 << exponent);
    Duration::from_millis(base_ms.saturating_add(jitter_ms.min(base_ms / 2)))
}

fn random_retry_delay(attempt: u8, retry_after: Option<Duration>) -> Duration {
    let exponent = u32::from(attempt.saturating_sub(1).min(6));
    let base_ms = 500_u64.saturating_mul(1_u64 << exponent);
    let jitter_ms = rand::thread_rng().gen_range(0..=base_ms / 2);
    provider_retry_delay(attempt, retry_after, jitter_ms)
}

fn retry_after_from_headers(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let raw = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    if let Ok(seconds) = raw.trim().parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let retry_at = chrono::DateTime::parse_from_rfc2822(raw).ok()?;
    let remaining = retry_at.with_timezone(&chrono::Utc) - chrono::Utc::now();
    remaining.to_std().ok()
}

pub(crate) fn retry_delay_for_response(
    attempt: u8,
    headers: &reqwest::header::HeaderMap,
) -> Duration {
    random_retry_delay(attempt, retry_after_from_headers(headers))
}

pub(crate) fn retry_delay_for_connection(attempt: u8) -> Duration {
    random_retry_delay(attempt, None)
}

// ── Request types ──────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct OpenAIChatRequest<'a> {
    model: &'a str,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OpenAITool<'a>>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

#[derive(Serialize)]
struct StreamOptions {
    include_usage: bool,
}

/// User-message content: either a plain string, or an array of parts (text +
/// images) for vision requests. Serializes to exactly what the OpenAI/Gemini
/// chat API expects in each case.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum UserContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text { text: String },
    ImageUrl { image_url: ImageUrlPart },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ImageUrlPart {
    /// A data URL: "data:image/jpeg;base64,<...>".
    pub url: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum OpenAIMessage {
    System {
        content: String,
    },
    User {
        content: UserContent,
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
    max_tokens: u32,
    temperature: Option<f32>,
    http: reqwest::Client,
}

#[cfg(test)]
mod boundary_tests {
    use super::{
        extract_error_message, provider_retry_delay, provider_timeouts,
        should_retry_provider_request, validate_provider_base_url,
    };
    use std::time::Duration;

    #[test]
    fn provider_urls_require_https_except_loopback() {
        assert!(validate_provider_base_url("https://api.openai.com/v1").is_ok());
        assert!(validate_provider_base_url("http://localhost:11434/v1").is_ok());
        assert!(validate_provider_base_url("http://127.0.0.1:11434/v1").is_ok());
        assert!(validate_provider_base_url("http://example.com/v1").is_err());
        assert!(validate_provider_base_url("https://user:pass@example.com/v1").is_err());
        assert!(validate_provider_base_url("file:///tmp/model").is_err());
    }

    #[test]
    fn provider_error_truncation_is_unicode_safe() {
        let message = extract_error_message(&"🙂".repeat(400));
        assert!(message.ends_with("… (truncated)"));
        assert_eq!(message.matches('🙂').count(), 300);
    }

    #[test]
    fn provider_idle_budget_is_shorter_than_the_total_turn_budget() {
        let timeouts = provider_timeouts(Duration::from_secs(1_800));
        assert_eq!(timeouts.total, Duration::from_secs(1_800));
        assert!(timeouts.idle >= Duration::from_secs(60));
        assert!(timeouts.idle <= Duration::from_secs(90));
        assert!(timeouts.idle < timeouts.total);
    }

    #[test]
    fn transient_status_and_connection_failures_retry_at_most_three_attempts() {
        for status in [429, 500, 502, 503, 504] {
            assert!(should_retry_provider_request(Some(status), false, false, 1));
        }
        assert!(should_retry_provider_request(None, true, false, 2));
        assert!(!should_retry_provider_request(None, true, false, 3));
    }

    #[test]
    fn permanent_statuses_and_post_mutation_failures_never_retry() {
        for status in [400, 401, 403, 404, 409, 422] {
            assert!(!should_retry_provider_request(
                Some(status),
                false,
                false,
                1
            ));
        }
        assert!(!should_retry_provider_request(Some(503), false, true, 1));
        assert!(!should_retry_provider_request(None, true, true, 1));
    }

    #[test]
    fn exponential_retry_jitter_stays_inside_the_bounded_window() {
        assert_eq!(provider_retry_delay(1, None, 0), Duration::from_millis(500));
        assert_eq!(
            provider_retry_delay(1, None, 250),
            Duration::from_millis(750)
        );
        assert_eq!(
            provider_retry_delay(2, None, 500),
            Duration::from_millis(1_500)
        );
        assert_eq!(
            provider_retry_delay(2, Some(Duration::from_secs(7)), 500),
            Duration::from_secs(7)
        );
    }
}

pub fn validate_provider_base_url(raw: &str) -> AppResult<()> {
    if raw.chars().count() > 2_048 {
        return Err(AppError::Validation(
            "Provider base URL exceeds 2048 characters".into(),
        ));
    }
    let url = reqwest::Url::parse(raw)
        .map_err(|e| AppError::Validation(format!("Invalid provider base URL: {e}")))?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AppError::Validation(
            "Provider base URL cannot contain credentials, query, or fragment".into(),
        ));
    }
    let loopback = url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err(AppError::Validation(
            "Provider base URL must use HTTPS; HTTP is allowed only for loopback providers".into(),
        ));
    }
    Ok(())
}

async fn read_bounded_body(response: reqwest::Response, limit: usize) -> AppResult<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(AppError::Internal(
            "Provider response exceeded the safety limit".into(),
        ));
    }
    let mut stream = response.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk
            .map_err(|error| AppError::Internal(format!("Provider response failed: {error}")))?;
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(AppError::Internal(
                "Provider response exceeded the safety limit".into(),
            ));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

impl OpenAIClient {
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
        max_tokens: u32,
        temperature: f32,
    ) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        // Total request time remains long for genuine tool chains. Individual
        // SSE reads use the much shorter idle budget in `send_stream`.
        let timeouts = provider_timeouts(Duration::from_secs(1_800));
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(timeouts.total)
            .build()
            .unwrap_or_default();
        Self {
            base_url,
            api_key: api_key.into(),
            model: model.into(),
            max_tokens,
            temperature: Some(temperature),
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
            let body = read_bounded_body(resp, 1_000_000).await?;
            let message = extract_error_message(&String::from_utf8_lossy(&body));
            return Err(AppError::Internal(format!(
                "Models error {}: {}",
                status, message
            )));
        }

        let body = read_bounded_body(resp, 5_000_000).await?;
        let parsed: OpenAIModelsResponse = serde_json::from_slice(&body)
            .map_err(|e| AppError::Internal(format!("Failed to parse models: {}", e)))?;

        let mut ids: Vec<String> = parsed
            .data
            .into_iter()
            .map(|m| m.id)
            .filter(|id| {
                !id.is_empty() && id.chars().count() <= 200 && !id.chars().any(char::is_control)
            })
            .take(1_000)
            .collect();
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
            temperature: self.temperature,
            tools: oai_tools,
            max_tokens: self.max_tokens,
            stream: None,
            stream_options: None,
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
            let body = read_bounded_body(resp, 1_000_000).await?;
            let msg = extract_error_message(&String::from_utf8_lossy(&body));
            return Err(AppError::Internal(format!(
                "Chat error {}: {}",
                status, msg
            )));
        }

        let body = read_bounded_body(resp, 5_000_000).await?;
        let parsed: OpenAIChatResponse = serde_json::from_slice(&body)
            .map_err(|e| AppError::Internal(format!("Failed to parse chat response: {}", e)))?;

        let choice = parsed
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| AppError::Internal("Empty choices from API".into()))?;

        let finish_reason = choice.finish_reason.unwrap_or_default();
        let msg = choice.message;

        if finish_reason == "tool_calls" || !msg.tool_calls.is_empty() {
            let mut tool_calls = Vec::new();
            for tc in msg.tool_calls {
                match serde_json::from_str::<serde_json::Value>(&tc.function.arguments) {
                    Ok(input) => tool_calls.push(OpenAIToolCallResult {
                        id: tc.id,
                        name: tc.function.name,
                        input,
                        parse_error: None,
                    }),
                    Err(e) => {
                        tracing::warn!(
                            tool = %tc.function.name,
                            err = %e,
                            "send: tool call had invalid JSON arguments — converting to recoverable error"
                        );
                        tool_calls.push(OpenAIToolCallResult {
                            id: tc.id,
                            name: tc.function.name,
                            input: serde_json::Value::Null,
                            parse_error: Some(format!("arguments were not valid JSON ({e})")),
                        });
                    }
                }
            }
            return Ok(OpenAIChatResult {
                text: msg.content.unwrap_or_default(),
                tool_calls,
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
                tool_calls: vec![tc],
                reasoning_content: msg.reasoning_content,
                finish_reason,
            });
        }

        Ok(OpenAIChatResult {
            text: raw_text,
            tool_calls: vec![],
            reasoning_content: msg.reasoning_content,
            finish_reason,
        })
    }

    /// Send a streaming chat request. Parses SSE chunks and calls callbacks for
    /// each token / tool-call fragment. Returns the assembled result.
    pub async fn send_stream(
        &self,
        system: &str,
        messages: Vec<OpenAIMessage>,
        tools: &[ToolDef],
        mutation_executed: bool,
        on_token: impl Fn(String),
        on_tool_start: impl Fn(String, String),
        on_tool_delta: impl Fn(String),
    ) -> AppResult<OpenAIStreamResult> {
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

        let msg_count = all_messages.len();

        let req = OpenAIChatRequest {
            model: &self.model,
            messages: all_messages,
            temperature: self.temperature,
            tools: oai_tools,
            max_tokens: self.max_tokens,
            stream: Some(true),
            stream_options: Some(StreamOptions {
                include_usage: true,
            }),
        };

        tracing::info!(url=%self.chat_url(), model=%self.model, msgs=msg_count, "send_stream: POST");
        let mut attempt = 1_u8;
        let resp = loop {
            match self
                .http
                .post(self.chat_url())
                .bearer_auth(&self.api_key)
                .json(&req)
                .send()
                .await
            {
                Ok(response)
                    if should_retry_provider_request(
                        Some(response.status().as_u16()),
                        false,
                        mutation_executed,
                        attempt,
                    ) =>
                {
                    let delay =
                        random_retry_delay(attempt, retry_after_from_headers(response.headers()));
                    tracing::warn!(
                        attempt,
                        status = %response.status(),
                        delay_ms = delay.as_millis(),
                        "send_stream: transient provider response; retrying"
                    );
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                }
                Ok(response) => break response,
                Err(error)
                    if should_retry_provider_request(
                        None,
                        error.is_connect(),
                        mutation_executed,
                        attempt,
                    ) =>
                {
                    let delay = random_retry_delay(attempt, None);
                    tracing::warn!(
                        attempt,
                        delay_ms = delay.as_millis(),
                        %error,
                        "send_stream: provider connection failed; retrying"
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

        tracing::info!(status=%resp.status(), "send_stream: response received");
        if !resp.status().is_success() {
            let status = resp.status();
            let body = read_bounded_body(resp, 1_000_000).await?;
            let msg = extract_error_message(&String::from_utf8_lossy(&body));
            tracing::error!(%status, %msg, "send_stream: API error");
            return Err(AppError::Internal(format!(
                "Stream error {}: {}",
                status, msg
            )));
        }

        let mut byte_stream = resp.bytes_stream();
        let mut line_buf: Vec<u8> = Vec::new();
        let mut turn_text = String::new();
        let mut tool_calls: std::collections::BTreeMap<usize, StreamingToolCall> =
            std::collections::BTreeMap::new();
        let mut finish_reason = String::new();
        let mut usage: Option<StreamUsage> = None;
        let mut reasoning_content = String::new();

        tracing::info!("send_stream: entering SSE read loop");
        let mut chunk_count: u64 = 0;
        let mut total_bytes: u64 = 0;
        let idle_timeout = provider_timeouts(Duration::from_secs(1_800)).idle;
        loop {
            let chunk = match tokio::time::timeout(idle_timeout, byte_stream.next()).await {
                Ok(Some(Ok(c))) => c,
                Ok(Some(Err(e))) => {
                    tracing::error!("send_stream: SSE read error: {e}");
                    return Err(AppError::Internal(format!("Stream read error: {e}")));
                }
                Ok(None) => {
                    tracing::info!(chunk_count, total_bytes, "send_stream: stream ended (None)");
                    break;
                }
                Err(_elapsed) => {
                    tracing::error!(
                        chunk_count,
                        total_bytes,
                        idle_secs = idle_timeout.as_secs(),
                        "send_stream: SSE idle timeout"
                    );
                    return Err(AppError::Internal(format!(
                        "The AI provider stopped responding for {} seconds. Retry the request.",
                        idle_timeout.as_secs()
                    )));
                }
            };
            chunk_count += 1;
            total_bytes += chunk.len() as u64;
            if total_bytes > 32 * 1024 * 1024 {
                return Err(AppError::Internal(
                    "Provider stream exceeded the 32 MB safety limit".into(),
                ));
            }
            if chunk_count <= 5 || chunk_count % 50 == 0 {
                tracing::info!(
                    chunk_count,
                    total_bytes,
                    chunk_len = chunk.len(),
                    "send_stream: chunk received"
                );
            }
            line_buf.extend_from_slice(&chunk);

            loop {
                let nl = line_buf.iter().position(|&b| b == b'\n');
                match nl {
                    None => {
                        if line_buf.len() > 65536 {
                            return Err(AppError::Internal(
                                "Provider sent an oversized SSE line".into(),
                            ));
                        }
                        break;
                    }
                    Some(pos) => {
                        let raw = String::from_utf8_lossy(&line_buf[..pos])
                            .trim_end_matches('\r')
                            .to_string();
                        line_buf.drain(..pos + 1);
                        if raw.is_empty() {
                            continue;
                        }
                        let data = match raw.strip_prefix("data: ") {
                            None => {
                                // Log first few non-data lines for diagnostics
                                if !raw.is_empty() && chunk_count <= 3 {
                                    tracing::warn!(%raw, "send_stream: non-data SSE line");
                                }
                                continue;
                            }
                            Some(d) => d,
                        };
                        if data == "[DONE]" {
                            tracing::info!("send_stream: received [DONE] sentinel");
                            continue;
                        }
                        let chunk: StreamChunk = match serde_json::from_str(data) {
                            Ok(c) => c,
                            Err(_) => continue,
                        };
                        for choice in chunk.choices {
                            if let Some(content) = choice.delta.content {
                                if !content.is_empty() {
                                    if turn_text.len().saturating_add(content.len()) > 1_000_000 {
                                        return Err(AppError::Internal(
                                            "Provider response exceeded the text safety limit"
                                                .into(),
                                        ));
                                    }
                                    turn_text.push_str(&content);
                                    on_token(content);
                                }
                            }
                            if let Some(reason) = choice.delta.reasoning_content {
                                if reasoning_content.len().saturating_add(reason.len()) > 1_000_000
                                {
                                    return Err(AppError::Internal(
                                        "Provider reasoning exceeded the safety limit".into(),
                                    ));
                                }
                                reasoning_content.push_str(&reason);
                            }
                            for tc in choice.delta.tool_calls {
                                let entry = tool_calls.entry(tc.index).or_insert_with(|| {
                                    StreamingToolCall {
                                        id: String::new(),
                                        name: String::new(),
                                        arguments: String::new(),
                                    }
                                });
                                let is_new = entry.id.is_empty();
                                if let Some(id) = tc.id {
                                    if id.len() > 512 {
                                        return Err(AppError::Internal(
                                            "Provider tool-call ID exceeded the safety limit"
                                                .into(),
                                        ));
                                    }
                                    entry.id = id;
                                }
                                if let Some(func) = tc.function {
                                    if let Some(name) = func.name {
                                        if name.len() > 256 {
                                            return Err(AppError::Internal(
                                                "Provider tool name exceeded the safety limit"
                                                    .into(),
                                            ));
                                        }
                                        entry.name = name;
                                        if is_new {
                                            on_tool_start(entry.id.clone(), entry.name.clone());
                                        }
                                    }
                                    if let Some(args) = func.arguments {
                                        if entry.arguments.len().saturating_add(args.len())
                                            > 1_000_000
                                        {
                                            return Err(AppError::Internal(
                                                "Provider tool arguments exceeded the safety limit"
                                                    .into(),
                                            ));
                                        }
                                        entry.arguments.push_str(&args);
                                        on_tool_delta(args);
                                    }
                                }
                            }
                            if let Some(fr) = choice.finish_reason {
                                if !fr.is_empty() {
                                    finish_reason = fr;
                                }
                            }
                        }
                        if let Some(u) = chunk.usage {
                            usage = Some(StreamUsage {
                                prompt_tokens: u.prompt_tokens,
                                completion_tokens: u.completion_tokens,
                                total_tokens: u.total_tokens,
                            });
                        }
                    }
                }
            }
        }

        let assembled_tool_calls: Vec<_> = tool_calls.into_values().collect();
        // "length" is included so that calls completed BEFORE the token cutoff
        // still execute, and the truncated final call surfaces as a recoverable
        // parse_error tool-result instead of being dropped silently.
        if !assembled_tool_calls.is_empty()
            && (finish_reason == "tool_calls"
                || finish_reason == "stop"
                || finish_reason == "length")
        {
            let mut parsed_tool_calls = Vec::with_capacity(assembled_tool_calls.len());
            for tc in &assembled_tool_calls {
                match serde_json::from_str::<serde_json::Value>(&tc.arguments) {
                    Ok(input) => parsed_tool_calls.push(OpenAIToolCallResult {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        input,
                        parse_error: None,
                    }),
                    Err(e) => {
                        // Do NOT fail the whole turn: report the malformed call
                        // back to the model so it can re-issue it. This happens
                        // when the provider truncates arguments (token limit)
                        // or a model emits broken JSON mid-batch.
                        tracing::warn!(
                            tool = %tc.name,
                            err = %e,
                            args_len = tc.arguments.len(),
                            %finish_reason,
                            "send_stream: tool call had invalid JSON arguments — converting to recoverable error"
                        );
                        parsed_tool_calls.push(OpenAIToolCallResult {
                            id: tc.id.clone(),
                            name: tc.name.clone(),
                            input: serde_json::Value::Null,
                            parse_error: Some(format!(
                                "arguments were not valid JSON ({e}); received {} chars, finish_reason={finish_reason}",
                                tc.arguments.len()
                            )),
                        });
                    }
                }
            }
            tracing::info!(text_len=turn_text.len(), tool_count=parsed_tool_calls.len(), finish_reason=%finish_reason, "send_stream: complete (with tool calls)");
            return Ok(OpenAIStreamResult {
                text: turn_text,
                tool_calls: parsed_tool_calls,
                reasoning_content: if reasoning_content.is_empty() {
                    None
                } else {
                    Some(reasoning_content)
                },
                finish_reason,
                usage,
            });
        }

        tracing::info!(text_len=turn_text.len(), finish_reason=%finish_reason, "send_stream: complete (no tool calls)");
        Ok(OpenAIStreamResult {
            text: turn_text,
            tool_calls: vec![],
            reasoning_content: if reasoning_content.is_empty() {
                None
            } else {
                Some(reasoning_content)
            },
            finish_reason,
            usage,
        })
    }
}

// ── Stream types ───────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
    #[serde(default)]
    usage: Option<StreamUsageRaw>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<StreamToolCall>,
    #[serde(default)]
    reasoning_content: Option<String>,
}

#[derive(Deserialize)]
struct StreamToolCall {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<StreamFunction>,
}

#[derive(Deserialize)]
struct StreamFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Deserialize)]
struct StreamUsageRaw {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
}

struct StreamingToolCall {
    id: String,
    name: String,
    arguments: String,
}

pub struct OpenAIStreamResult {
    pub text: String,
    pub tool_calls: Vec<OpenAIToolCallResult>,
    pub reasoning_content: Option<String>,
    #[allow(dead_code)]
    pub finish_reason: String,
    #[allow(dead_code)]
    pub usage: Option<StreamUsage>,
}

#[allow(dead_code)]
pub struct StreamUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

pub struct OpenAIChatResult {
    pub text: String,
    pub tool_calls: Vec<OpenAIToolCallResult>,
    pub reasoning_content: Option<String>,
    /// Deserialized from API response; retained for future logging/retry logic.
    #[allow(dead_code)]
    pub finish_reason: String,
}

pub struct OpenAIToolCallResult {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
    /// Set when the provider streamed arguments that were not valid JSON
    /// (typically truncated by the output-token limit). The agent loop turns
    /// this into a recoverable tool-result error so the model can retry,
    /// instead of the whole turn failing with an Internal error.
    pub parse_error: Option<String>,
}

// ── Message builders ───────────────────────────────────────────────────────────

pub fn user_msg(text: impl Into<String>) -> OpenAIMessage {
    OpenAIMessage::User {
        content: UserContent::Text(text.into()),
    }
}

/// A user message carrying text plus an image (OpenAI/Gemini vision format).
/// `image_data_url` must be a data URL, e.g. "data:image/jpeg;base64,<...>".
pub fn user_msg_with_image(
    text: impl Into<String>,
    image_data_url: impl Into<String>,
) -> OpenAIMessage {
    OpenAIMessage::User {
        content: UserContent::Parts(vec![
            ContentPart::Text { text: text.into() },
            ContentPart::ImageUrl {
                image_url: ImageUrlPart {
                    url: image_data_url.into(),
                },
            },
        ]),
    }
}

pub fn assistant_msg(text: impl Into<String>) -> OpenAIMessage {
    OpenAIMessage::Assistant {
        content: Some(text.into()),
        reasoning_content: None,
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
        parse_error: None,
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

/// Extract a user-facing error message from an OpenAI/OpenRouter error body.
/// The raw JSON can be enormous (OpenRouter includes every provider failure).
fn extract_error_message(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(msg) = v
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
        {
            return msg.to_string();
        }
    }
    // Fallback: truncate the raw body so we never dump a 5 KB JSON blob to the UI.
    if body.chars().count() > 300 {
        format!(
            "{}… (truncated)",
            body.chars().take(300).collect::<String>()
        )
    } else {
        body.to_string()
    }
}
