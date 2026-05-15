use tauri::State;
use ulid::Ulid;
use crate::AppState;
use crate::domain::ai_admin::*;
use crate::db::repositories::ai_admin_repo;
use crate::ai::{provider::Provider, tools};
use crate::errors::{AppError, AppResult};

// ── Provider config management ────────────────────────────────────────────────

#[tauri::command]
pub async fn admin_get_provider_config(state: State<'_, AppState>) -> AppResult<ProviderConfig> {
    let provider = ai_admin_repo::get_config(&state.db, "ai_provider").await?
        .unwrap_or_default();
    let anthropic_key = ai_admin_repo::get_config(&state.db, "anthropic_api_key").await?
        .unwrap_or_default();
    let openai_base_url = ai_admin_repo::get_config(&state.db, "openai_base_url").await?
        .unwrap_or_default();
    let openai_key = ai_admin_repo::get_config(&state.db, "openai_api_key").await?
        .unwrap_or_default();
    let openai_model = ai_admin_repo::get_config(&state.db, "openai_model").await?
        .unwrap_or_default();

    Ok(ProviderConfig {
        provider,
        anthropic_key_set: !anthropic_key.is_empty(),
        openai_base_url,
        openai_key_set: !openai_key.is_empty(),
        openai_model,
    })
}

/// Set Anthropic as the provider.
#[tauri::command]
pub async fn admin_set_anthropic(state: State<'_, AppState>, api_key: String) -> AppResult<()> {
    ai_admin_repo::set_config(&state.db, "anthropic_api_key", &api_key).await?;
    ai_admin_repo::set_config(&state.db, "ai_provider", "anthropic").await
}

/// Validate an OpenAI-compatible endpoint: calls /models and returns the list.
#[tauri::command]
pub async fn admin_validate_openai(
    _state: State<'_, AppState>,
    base_url: String,
    api_key: String,
) -> AppResult<ValidateProviderResult> {
    use crate::ai::openai_client::OpenAIClient;

    let client = OpenAIClient::new(&base_url, &api_key, "test");
    match client.list_models().await {
        Ok(ids) => {
            let models = ids.into_iter().map(|id| ModelInfo { id }).collect();
            Ok(ValidateProviderResult { success: true, models, error: None })
        }
        Err(e) => Ok(ValidateProviderResult {
            success: false,
            models: vec![],
            error: Some(e.to_string()),
        }),
    }
}

/// Save OpenAI-compatible provider config and set it as active.
#[tauri::command]
pub async fn admin_set_openai(
    state: State<'_, AppState>,
    base_url: String,
    api_key: String,
    model: String,
) -> AppResult<()> {
    ai_admin_repo::set_config(&state.db, "openai_base_url", &base_url).await?;
    ai_admin_repo::set_config(&state.db, "openai_api_key", &api_key).await?;
    ai_admin_repo::set_config(&state.db, "openai_model", &model).await?;
    ai_admin_repo::set_config(&state.db, "ai_provider", "openai").await
}

// Legacy command — kept for backwards compatibility, forwards to admin_set_anthropic.
#[tauri::command]
pub async fn admin_get_api_key_set(state: State<'_, AppState>) -> AppResult<bool> {
    let cfg = admin_get_provider_config(state).await?;
    Ok(!cfg.provider.is_empty())
}

#[tauri::command]
pub async fn admin_set_api_key(state: State<'_, AppState>, key: String) -> AppResult<()> {
    admin_set_anthropic(state, key).await
}

// ── Core chat command ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn ai_chat(
    state: State<'_, AppState>,
    input: AiChatInput,
) -> AppResult<AiChatResponse> {
    ai_admin_repo::expire_old_actions(&state.db).await.ok();

    let Some(provider) = Provider::from_db(&state.db).await? else {
        return Ok(AiChatResponse::NoApiKey);
    };

    let tool_defs = tools::all_tool_definitions();
    let system = build_system_prompt();

    let result = provider.send_chat(&system, &input.history, &input.message, &tool_defs).await?;

    let Some(tool_call) = result.tool_call else {
        return Ok(AiChatResponse::Message { content: result.text });
    };

    let assistant_text = result.text;

    if tools::is_mutation_tool(&tool_call.name) {
        let preview = tools::dry_run_mutation(
            &state.db,
            &tool_call.name,
            &tool_call.input,
            input.currency_exponent,
        ).await?;

        let tool_input_json = tool_call.input.to_string();
        let preview_text = format!("{}: {}",
            preview.description,
            preview.fields.iter()
                .map(|f| format!("{} = {}", f.label, f.value))
                .collect::<Vec<_>>()
                .join(", "));

        let action = ai_admin_repo::create_action(
            &state.db,
            &input.user_id,
            &tool_call.name,
            &tool_input_json,
            &hash_str(&tool_input_json),
            &preview_text,
            &Ulid::new().to_string(),
        ).await?;

        return Ok(AiChatResponse::PendingAction {
            action_id: action.action_id,
            tool_name: tool_call.name,
            preview,
            expires_at: action.expires_at,
            assistant_text,
        });
    }

    // Read-only tool: execute and continue
    let tool_result = tools::execute_read_tool(
        &state.db,
        &tool_call.name,
        &tool_call.input,
        &input.branch_id,
        input.currency_exponent,
    ).await?;

    let followup = provider.continue_with_tool_result(
        &system,
        &input.history,
        &input.message,
        &crate::ai::provider::ToolCallResult {
            id: tool_call.id,
            name: tool_call.name,
            input: tool_call.input,
        },
        tool_result,
        &tool_defs,
    ).await?;

    Ok(AiChatResponse::Message { content: followup })
}

// ── Execute confirmed action ───────────────────────────────────────────────────

#[tauri::command]
pub async fn ai_execute_action(
    state: State<'_, AppState>,
    input: ExecuteActionInput,
) -> AppResult<ExecuteActionResult> {
    let action = ai_admin_repo::get_action(&state.db, &input.action_id).await?
        .ok_or_else(|| AppError::NotFound("Action not found".into()))?;

    if action.status != "prepared" {
        return Err(AppError::Conflict(format!("Action status is '{}', expected 'prepared'", action.status)));
    }
    if action.session_user_id != input.user_id {
        return Err(AppError::Permission("Action belongs to different user".into()));
    }
    if action.expires_at < chrono::Utc::now().to_rfc3339() {
        return Err(AppError::Conflict("Action has expired".into()));
    }

    let tool_input: serde_json::Value = serde_json::from_str(&action.tool_input_json)
        .map_err(|e| AppError::Validation(format!("Invalid tool input: {}", e)))?;

    let mutation_result = tools::execute_mutation(
        &state.db,
        &action.tool_name,
        &tool_input,
        input.currency_exponent,
    ).await?;

    let result_json = serde_json::json!({ "description": &mutation_result.description }).to_string();
    ai_admin_repo::mark_executed(&state.db, &input.action_id, &result_json).await?;

    let undo = ai_admin_repo::create_undo_record(
        &state.db,
        &input.action_id,
        &mutation_result.entity_type,
        &mutation_result.entity_id,
        &mutation_result.undo_snapshot_json,
        &mutation_result.rollback_tool,
        &mutation_result.rollback_input_json,
    ).await?;

    let followup = if let Ok(Some(provider)) = Provider::from_db(&state.db).await {
        provider.get_followup(
            &build_system_prompt(),
            &input.history,
            &input.assistant_text,
            &action.tool_name,
            &mutation_result.description,
        ).await.unwrap_or_else(|_| format!("Done — {}", mutation_result.description))
    } else {
        format!("Done — {}", mutation_result.description)
    };

    Ok(ExecuteActionResult {
        action_id: input.action_id,
        undo_id: Some(undo.undo_id),
        followup,
    })
}

// ── Cancel pending action ──────────────────────────────────────────────────────

#[tauri::command]
pub async fn ai_cancel_action(state: State<'_, AppState>, action_id: String) -> AppResult<()> {
    ai_admin_repo::mark_cancelled(&state.db, &action_id).await
}

// ── Undo executed action ───────────────────────────────────────────────────────

#[tauri::command]
pub async fn ai_undo_action(
    state: State<'_, AppState>,
    undo_id: String,
    user_id: String,
    currency_exponent: u32,
) -> AppResult<UndoActionResult> {
    let record = ai_admin_repo::get_undo_record(&state.db, &undo_id).await?
        .ok_or_else(|| AppError::NotFound("Undo record not found".into()))?;

    if record.status != "available" {
        return Err(AppError::Conflict(format!("Undo record status is '{}'", record.status)));
    }

    let description = tools::execute_undo(
        &state.db,
        &record.rollback_tool,
        &record.rollback_input_json,
        currency_exponent,
    ).await?;

    ai_admin_repo::mark_undone(&state.db, &undo_id, &user_id).await?;

    Ok(UndoActionResult {
        undo_id,
        followup: format!("Undone — {}", description),
    })
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn build_system_prompt() -> String {
    "You are the AI admin assistant for ZANPOS, a point-of-sale system. \
     You help the owner/manager query sales data and manage the product catalog. \
     \n\nBHD (Bahraini Dinar) is stored as minor units (1 BHD = 1000 minor). \
     When quoting prices always show in BHD with 3 decimal places. \
     \n\nFor read operations, call the appropriate tool and summarize the results clearly. \
     For mutations (price changes, enabling/disabling products, renaming), call the tool and \
     explain what you're about to do — the system will ask the admin to confirm before executing. \
     \n\nBe concise and professional. If unsure about a product, search first."
        .into()
}

fn hash_str(s: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    format!("{:x}", h.finish())
}
