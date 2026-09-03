use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── Persisted action lifecycle ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAction {
    pub action_id: String,
    pub session_user_id: String,
    pub branch_id: String,
    pub tool_name: String,
    pub tool_input_json: String,
    pub tool_input_hash: String,
    pub preview_text: String,
    pub status: String,
    pub confirmation_token: String,
    pub prepared_at: String,
    pub confirmed_at: Option<String>,
    pub executed_at: Option<String>,
    pub expires_at: String,
    pub result_json: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoRecord {
    pub undo_id: String,
    pub action_id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub snapshot_json: String,
    pub rollback_tool: String,
    pub rollback_input_json: String,
    pub status: String,
    pub created_at: String,
    pub undone_at: Option<String>,
    pub undone_by_user_id: Option<String>,
}

// ── Chat message (simplified, frontend-visible) ───────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

// ── Tool definitions (mirrored in ai/tools.rs) ────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPreview {
    pub tool_name: String,
    pub description: String,
    pub fields: Vec<ToolPreviewField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPreviewField {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchPendingAction {
    pub action_id: String,
    pub tool_name: String,
    pub preview: ToolPreview,
    pub expires_at: String,
}

// ── Command I/O types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiChatInput {
    pub request_id: String,
    pub history: Vec<ChatMessage>,
    pub message: String,
    #[serde(default)]
    pub user_id: String,
    pub branch_id: String,
    pub currency_exponent: u32,
    /// Which thread this belongs to. Absent on an older client, in which case
    /// the backend opens one — a message is never dropped for want of a thread.
    #[serde(default)]
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub ui_context: Option<String>,
    #[serde(default)]
    pub image_base64: Option<String>,
    #[serde(default)]
    pub image_media_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteActionInput {
    pub action_id: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub actor_user_id: String,
    pub history: Vec<ChatMessage>,
    pub assistant_text: String,
    pub currency_exponent: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteActionResult {
    pub action_id: String,
    pub undo_id: Option<String>,
    pub followup: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoActionResult {
    pub undo_id: String,
    pub followup: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteBatchInput {
    pub action_ids: Vec<String>,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub actor_user_id: String,
    pub history: Vec<ChatMessage>,
    pub assistant_text: String,
    pub currency_exponent: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteBatchResult {
    pub followup: String,
    pub undo_ids: Vec<String>,
}

// ── Persisted chat message ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiChatMessage {
    pub id: i64,
    pub message_id: String,
    pub session_id: String,
    pub branch_id: String,
    pub user_id: String,
    pub role: String, // "user" | "assistant" | "system_event"
    pub content: String,
    pub message_type: String, // "text" | "action_card" | "error"
    pub created_at: String,
}

/// A chat thread, as the operator thinks of one.
///
/// Distinct from `AiSession`, which is one request's worth of usage accounting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConversation {
    pub conversation_id: String,
    pub branch_id: String,
    pub user_id: String,
    pub title: String,
    pub message_count: i64,
    pub last_message_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A thread and its messages, as the chat panel needs them together: the id so
/// the next message joins the right thread, and the messages to render.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConversationView {
    pub conversation_id: String,
    pub title: String,
    pub messages: Vec<AiChatMessage>,
}

// ── Streaming events (sent over Tauri Channel) ─────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    Started,
    Token {
        text: String,
    },
    ToolStart {
        name: String,
    },
    ToolDone {
        name: String,
    },
    MutationPending {
        action_id: String,
        tool_name: String,
        preview: ToolPreview,
        expires_at: String,
        assistant_text: String,
    },
    MutationBatchPending {
        actions: Vec<BatchPendingAction>,
        assistant_text: String,
    },
    MutationExecuted {
        action_id: String,
        tool_name: String,
        undo_id: Option<String>,
        description: String,
    },
    Navigate {
        tab: String,
    },
    /// An interactive form to draw in the chat. The turn always ends here: see
    /// `crate::ai::forms` for why a form can never execute anything itself.
    FormRequest {
        form: crate::ai::forms::AiForm,
    },
    RunPreview {
        run_id: String,
        op_id: String,
        description: String,
        count: i64,
        samples: Vec<Value>,
        requires_confirmation: bool,
    },
    RunProgress {
        run_id: String,
        done: i64,
        total: i64,
    },
    RunDone {
        run_id: String,
    },
    RunFailed {
        run_id: String,
        error: String,
    },
    MessagePersisted {
        session_id: String,
        message_id: String,
    },
    Cancelled,
    Done,
    Error {
        message: String,
    },
}

// ── Provider config ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// "anthropic" | "openai" | "gemini" | ""
    pub provider: String,
    pub anthropic_key_set: bool,
    pub anthropic_model: String,
    pub openai_base_url: String,
    pub openai_key_set: bool,
    pub openai_model: String,
    // Google Gemini (OpenAI-compatible endpoint)
    pub gemini_key_set: bool,
    pub gemini_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateProviderResult {
    pub success: bool,
    pub models: Vec<ModelInfo>,
    pub error: Option<String>,
}

/// Feature toggle flags exposed to the settings UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureToggles {
    pub web_search: bool,
    pub web_fetch: bool,
    pub compare_prices: bool,
    pub market_price: bool,
    pub smart_analytics: bool,
    pub proactive: bool,
    pub inventory_ops: bool,
    pub customer_insights: bool,
    pub insights_engine: bool,
}

// ── Proactive intelligence alert ──────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProactiveAlert {
    pub alert_id: String,
    pub branch_id: String,
    pub alert_type: String,
    pub severity: String,
    pub title: String,
    pub description: String,
    pub detail_json: Option<String>,
    pub detected_at: String,
    pub dismissed_at: Option<String>,
    pub dismissed_by_user_id: Option<String>,
    pub created_at: String,
}

// ── Session & usage tracking ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
// Retained: schema documentation: the table is queried inline, not through this type.
#[allow(dead_code)]
pub struct AiSession {
    pub session_id: String,
    pub branch_id: String,
    pub user_id: String,
    pub provider: String,
    pub model: String,
    pub status: String,
    pub total_turns: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub cost_estimate_usd: f64,
    pub total_latency_ms: i64,
    pub started_at: String,
    pub ended_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
// Retained: schema documentation: the table is queried inline, not through this type.
#[allow(dead_code)]
pub struct AiUsageRecord {
    pub id: i64,
    pub session_id: String,
    pub turn: i32,
    pub tokens_in: i32,
    pub tokens_out: i32,
    pub latency_ms: i32,
    pub provider: String,
    pub model: String,
    pub logged_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
// Retained: schema documentation: the table is queried inline, not through this type.
#[allow(dead_code)]
pub struct AiFeedback {
    pub feedback_id: String,
    pub session_id: String,
    pub user_id: String,
    pub message_id: String,
    pub rating: String,
    pub comment: Option<String>,
    pub created_at: String,
}

/// All AI runtime parameters exposed to the settings UI.
/// Mirrors `AiParams` but uses types that round-trip cleanly through JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfigPayload {
    pub anthropic_max_tokens: u32,
    pub openai_max_tokens: u32,
    pub temperature: f32,
    pub max_turns: usize,
    pub context_window_chars: usize,
    pub connect_timeout_secs: u64,
    pub stream_timeout_secs: u64,
    pub action_expiry_minutes: i64,
    pub bulk_batch_size: usize,
    pub tool_result_max_chars: usize,
    pub turn_tool_results_max_chars: usize,
    pub confirm_non_destructive_actions: bool,
    pub sensitive_protection_level: String,
}

/// Display-safe projection of `AiAction` for the Review queue.
///
/// Deliberately omits `confirmation_token` (a secret) and `tool_input_hash`
/// (an integrity value the client has no use for and must not be able to
/// influence). `preview_text` is the human-readable description already
/// prepared when the action was created, so the queue never renders raw JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiActionSummary {
    pub action_id: String,
    pub session_user_id: String,
    pub branch_id: String,
    pub tool_name: String,
    pub preview_text: String,
    pub status: String,
    pub prepared_at: String,
    pub confirmed_at: Option<String>,
    pub executed_at: Option<String>,
    pub expires_at: String,
    pub result_json: Option<String>,
    pub error_message: Option<String>,
}

/// Whether an executed action can still be reversed.
///
/// Deliberately omits `snapshot_json` and `rollback_input_json`: those are the
/// rollback payload, and the client has no legitimate use for them — undo is
/// executed by id, exactly like confirmation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoAvailability {
    pub undo_id: String,
    pub action_id: String,
    pub entity_type: String,
    pub entity_id: String,
    /// "available" | "undone"
    pub status: String,
    pub available: bool,
    pub created_at: String,
    pub undone_at: Option<String>,
}
