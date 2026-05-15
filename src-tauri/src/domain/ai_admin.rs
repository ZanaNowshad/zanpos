use serde::{Deserialize, Serialize};

// ── Persisted action lifecycle ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAction {
    pub action_id: String,
    pub session_user_id: String,
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

// ── Command I/O types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiChatInput {
    pub history: Vec<ChatMessage>,
    pub message: String,
    pub user_id: String,
    pub branch_id: String,
    pub currency_exponent: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AiChatResponse {
    Message {
        content: String,
    },
    PendingAction {
        action_id: String,
        tool_name: String,
        preview: ToolPreview,
        expires_at: String,
        /// Partial assistant turn text shown before the confirmation card
        assistant_text: String,
    },
    NoApiKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteActionInput {
    pub action_id: String,
    pub user_id: String,
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

// ── Provider config ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// "anthropic" | "openai" | ""
    pub provider: String,
    pub anthropic_key_set: bool,
    pub openai_base_url: String,
    pub openai_key_set: bool,
    pub openai_model: String,
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
