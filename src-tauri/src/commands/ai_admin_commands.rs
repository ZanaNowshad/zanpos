use crate::ai::{client::ToolDef, provider::{ChatResult, Provider, ToolCallResult}, tools};
use crate::commands::rbac;
use crate::db::repositories::{ai_admin_repo, ai_chat_history_repo};
use crate::domain::ai_admin::*;
use crate::errors::{AppError, AppResult};
use crate::secure_store;
use crate::AppState;
use tauri::ipc::Channel;
use tauri::State;
use ulid::Ulid;

// ── Provider config management ────────────────────────────────────────────────

#[tauri::command]
pub async fn admin_get_provider_config(state: State<'_, AppState>) -> AppResult<ProviderConfig> {
    let provider = ai_admin_repo::get_config(&state.db, "ai_provider")
        .await?
        .unwrap_or_default();

    // Keys: prefer OS credential store, fall back to legacy plaintext SQLite value.
    // HIGH #8: log a warning when the OS store call itself errors (returns None)
    // so "401 Unauthorized" API errors have an obvious upstream cause.
    let anthropic_key = {
        let from_os = match secure_store::get_secret("anthropic_api_key") {
            Some(k) => k,
            None => {
                tracing::warn!("OS credential store returned None for anthropic_api_key — falling back to DB");
                String::new()
            }
        };
        if !from_os.is_empty() {
            from_os
        } else {
            ai_admin_repo::get_config(&state.db, "anthropic_api_key")
                .await?
                .unwrap_or_default()
        }
    };

    let openai_key = {
        let from_os = match secure_store::get_secret("openai_api_key") {
            Some(k) => k,
            None => {
                tracing::warn!("OS credential store returned None for openai_api_key — falling back to DB");
                String::new()
            }
        };
        if !from_os.is_empty() {
            from_os
        } else {
            ai_admin_repo::get_config(&state.db, "openai_api_key")
                .await?
                .unwrap_or_default()
        }
    };

    let openai_base_url = ai_admin_repo::get_config(&state.db, "openai_base_url")
        .await?
        .unwrap_or_default();
    let openai_model = ai_admin_repo::get_config(&state.db, "openai_model")
        .await?
        .unwrap_or_default();

    // Gemini key: OS store first, DB fallback (same pattern as the others).
    let gemini_key = {
        let from_os = secure_store::get_secret("gemini_api_key").unwrap_or_default();
        if !from_os.is_empty() {
            from_os
        } else {
            ai_admin_repo::get_config(&state.db, "gemini_api_key")
                .await?
                .unwrap_or_default()
        }
    };
    let gemini_model = ai_admin_repo::get_config(&state.db, "gemini_model")
        .await?
        .unwrap_or_default();

    Ok(ProviderConfig {
        provider,
        anthropic_key_set: !anthropic_key.is_empty(),
        openai_base_url,
        openai_key_set: !openai_key.is_empty(),
        openai_model,
        gemini_key_set: !gemini_key.is_empty(),
        gemini_model,
    })
}

/// Set Anthropic as the provider.
/// The API key is stored in the OS credential manager; only the provider name is in SQLite.
#[tauri::command]
pub async fn admin_set_anthropic(
    state: State<'_, AppState>,
    actor_user_id: String,
    api_key: String,
) -> AppResult<()> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    // Store key securely in the OS credential manager (Windows Credential Manager).
    // If the OS store is unavailable we REFUSE to store the key rather than silently
    // downgrading to cleartext SQLite — see F-SEC-001.
    if !secure_store::set_secret("anthropic_api_key", &api_key) {
        tracing::error!(
            "OS credential store unavailable — API key NOT saved. \
             Check Windows Credential Manager access and retry."
        );
        return Err(AppError::Internal(
            "Windows Credential Manager is unavailable. \
             The API key cannot be stored securely. \
             Please check your Windows user profile and try again.".into(),
        ));
    }
    // Remove any stale plaintext copy that may have been written before this fix.
    ai_admin_repo::set_config(&state.db, "anthropic_api_key", "").await?;
    ai_admin_repo::set_config(&state.db, "ai_provider", "anthropic").await
}

/// Validate an Anthropic API key by calling the models endpoint.
#[tauri::command]
pub async fn admin_validate_anthropic(
    _state: State<'_, AppState>,
    api_key: String,
) -> AppResult<ValidateProviderResult> {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_default();
    let resp = client
        .get("https://api.anthropic.com/v1/models")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => Ok(ValidateProviderResult {
            success: true,
            models: vec![],
            error: None,
        }),
        Ok(r) => Ok(ValidateProviderResult {
            success: false,
            models: vec![],
            error: Some(format!("HTTP {}", r.status())),
        }),
        Err(e) => Ok(ValidateProviderResult {
            success: false,
            models: vec![],
            error: Some(e.to_string()),
        }),
    }
}

/// Validate an OpenAI-compatible endpoint: calls /models and returns the list.
#[tauri::command]
pub async fn admin_validate_openai(
    _state: State<'_, AppState>,
    base_url: String,
    api_key: String,
) -> AppResult<ValidateProviderResult> {
    use crate::ai::openai_client::OpenAIClient;

    let client = OpenAIClient::new(&base_url, &api_key, "gpt-4o-mini");
    match client.list_models().await {
        Ok(ids) => {
            let models = ids.into_iter().map(|id| ModelInfo { id }).collect();
            Ok(ValidateProviderResult {
                success: true,
                models,
                error: None,
            })
        }
        Err(e) => Ok(ValidateProviderResult {
            success: false,
            models: vec![],
            error: Some(e.to_string()),
        }),
    }
}

/// Save OpenAI-compatible provider config and set it as active.
/// The API key is stored in the OS credential manager; non-secret fields stay in SQLite.
#[tauri::command]
pub async fn admin_set_openai(
    state: State<'_, AppState>,
    actor_user_id: String,
    base_url: String,
    api_key: String,
    model: String,
) -> AppResult<()> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    ai_admin_repo::set_config(&state.db, "openai_base_url", &base_url).await?;
    // Store key securely in the OS credential manager — refuse if unavailable (F-SEC-001).
    if !secure_store::set_secret("openai_api_key", &api_key) {
        tracing::error!(
            "OS credential store unavailable — OpenAI API key NOT saved."
        );
        return Err(AppError::Internal(
            "Windows Credential Manager is unavailable. \
             The API key cannot be stored securely. \
             Please check your Windows user profile and try again.".into(),
        ));
    }
    // Remove any stale plaintext copy.
    ai_admin_repo::set_config(&state.db, "openai_api_key", "").await?;
    ai_admin_repo::set_config(&state.db, "openai_model", &model).await?;
    ai_admin_repo::set_config(&state.db, "ai_provider", "openai").await
}

/// Validate a Google Gemini API key by listing models on its OpenAI-compatible endpoint.
#[tauri::command]
pub async fn admin_validate_gemini(
    _state: State<'_, AppState>,
    api_key: String,
) -> AppResult<ValidateProviderResult> {
    use crate::ai::openai_client::OpenAIClient;
    use crate::ai::provider::{GEMINI_BASE_URL, GEMINI_DEFAULT_MODEL};

    let client = OpenAIClient::new(GEMINI_BASE_URL, &api_key, GEMINI_DEFAULT_MODEL);
    match client.list_models().await {
        Ok(ids) => {
            // Gemini lists many models; keep only the chat-capable "gemini-*" ones.
            let models = ids
                .into_iter()
                .filter(|id| id.contains("gemini"))
                .map(|id| ModelInfo { id })
                .collect();
            Ok(ValidateProviderResult { success: true, models, error: None })
        }
        Err(e) => Ok(ValidateProviderResult {
            success: false,
            models: vec![],
            error: Some(e.to_string()),
        }),
    }
}

/// Save Google Gemini provider config and set it as active.
/// Base URL is fixed (Gemini's OpenAI-compatible endpoint); the user supplies key + model.
#[tauri::command]
pub async fn admin_set_gemini(
    state: State<'_, AppState>,
    actor_user_id: String,
    api_key: String,
    model: String,
) -> AppResult<()> {
    use crate::ai::provider::GEMINI_DEFAULT_MODEL;
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    // Store key securely in the OS credential manager — refuse if unavailable (F-SEC-001).
    if !secure_store::set_secret("gemini_api_key", &api_key) {
        tracing::error!("OS credential store unavailable — Gemini API key NOT saved.");
        return Err(AppError::Internal(
            "Windows Credential Manager is unavailable. \
             The API key cannot be stored securely. \
             Please check your Windows user profile and try again.".into(),
        ));
    }
    // Remove any stale plaintext copy.
    ai_admin_repo::set_config(&state.db, "gemini_api_key", "").await?;
    let model = if model.trim().is_empty() {
        GEMINI_DEFAULT_MODEL.to_string()
    } else {
        model
    };
    ai_admin_repo::set_config(&state.db, "gemini_model", &model).await?;
    ai_admin_repo::set_config(&state.db, "ai_provider", "gemini").await
}

// Legacy command — kept for backwards compatibility, forwards to admin_set_anthropic.
#[tauri::command]
pub async fn admin_get_api_key_set(state: State<'_, AppState>) -> AppResult<bool> {
    let cfg = admin_get_provider_config(state).await?;
    Ok(!cfg.provider.is_empty())
}

#[tauri::command]
pub async fn admin_set_api_key(
    state: State<'_, AppState>,
    actor_user_id: String,
    key: String,
) -> AppResult<()> {
    admin_set_anthropic(state, actor_user_id, key).await
}

// ── Core chat command ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn ai_chat(state: State<'_, AppState>, input: AiChatInput) -> AppResult<AiChatResponse> {
    ai_admin_repo::expire_old_actions(&state.db).await.ok();

    let Some(provider) = Provider::from_db(&state.db).await? else {
        return Ok(AiChatResponse::NoApiKey);
    };

    let tool_defs = tools::all_tool_definitions();
    let system = build_system_prompt();

    let initial = provider
        .send_chat(&system, &input.history, &input.message, &tool_defs)
        .await?;

    match run_tool_loop(&state.db, &provider, &system, &input, &tool_defs, initial, |_| {}, |_| {})
        .await?
    {
        ToolLoopOutcome::Done { text } => Ok(AiChatResponse::Message { content: text }),
        ToolLoopOutcome::PendingAction {
            action_id,
            tool_name,
            preview,
            expires_at,
            assistant_text,
        } => Ok(AiChatResponse::PendingAction {
            action_id,
            tool_name,
            preview,
            expires_at,
            assistant_text,
        }),
    }
}

// ── Execute confirmed action ───────────────────────────────────────────────────

#[tauri::command]
pub async fn ai_execute_action(
    state: State<'_, AppState>,
    input: ExecuteActionInput,
) -> AppResult<ExecuteActionResult> {
    let action = ai_admin_repo::get_action(&state.db, &input.action_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Action not found".into()))?;

    if action.status != "prepared" {
        return Err(AppError::Conflict(format!(
            "Action status is '{}', expected 'prepared'",
            action.status
        )));
    }
    if action.session_user_id != input.user_id {
        return Err(AppError::Permission(
            "Action belongs to different user".into(),
        ));
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
    )
    .await?;

    let result_json =
        serde_json::json!({ "description": &mutation_result.description }).to_string();
    ai_admin_repo::mark_executed(&state.db, &input.action_id, &result_json).await?;

    let undo = ai_admin_repo::create_undo_record(
        &state.db,
        &input.action_id,
        &mutation_result.entity_type,
        &mutation_result.entity_id,
        &mutation_result.undo_snapshot_json,
        &mutation_result.rollback_tool,
        &mutation_result.rollback_input_json,
    )
    .await?;

    let followup = if let Ok(Some(provider)) = Provider::from_db(&state.db).await {
        provider
            .get_followup(
                &build_system_prompt(),
                &input.history,
                &input.assistant_text,
                &action.tool_name,
                &mutation_result.description,
            )
            .await
            .unwrap_or_else(|_| format!("Done — {}", mutation_result.description))
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
    // Undoing an AI-driven mutation is a destructive operation — manager or owner only.
    rbac::manager_or_owner(&state.db, &user_id).await?;

    let record = ai_admin_repo::get_undo_record(&state.db, &undo_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Undo record not found".into()))?;

    if record.status != "available" {
        return Err(AppError::Conflict(format!(
            "Undo record status is '{}'",
            record.status
        )));
    }

    let description = tools::execute_undo(
        &state.db,
        &record.rollback_tool,
        &record.rollback_input_json,
        currency_exponent,
    )
    .await?;

    ai_admin_repo::mark_undone(&state.db, &undo_id, &user_id).await?;

    Ok(UndoActionResult {
        undo_id,
        followup: format!("Undone — {}", description),
    })
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn build_system_prompt() -> String {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    let header = format!(
"# ZANPOS AI Assistant — Precision Rules (READ FIRST)

You are an EXACT business intelligence assistant for a POS system in Bahrain.
You operate with temperature=0.0: every response must be deterministic and fact-based.

## CRITICAL RULES (violating any = failure)

1.  **NEVER fabricate data.** If a tool returns no results, say so explicitly. Do not invent numbers, names, or IDs.
2.  **ALWAYS call tools** for factual queries. Your training data is stale — the database is ground truth.
3.  **ALWAYS prefix data sources:**
    - Data from database tools → prepend responses with `[DB]`
    - Data from web_search / fetch_url → prepend with `[WEB]`
    - Include a **Data as of: {now}** footer on every response.
4.  **If you are unsure** about ANY numerical value, respond: `I do not have enough data to answer this precisely.` Never guess.
5.  **ALWAYS show prices as BHD X.XXX** (3 decimal places, zero-padded). Never abbreviate.
6.  **For mutations:** first explain what will happen, call the tool, then STOP. Do not add follow-up text after a mutation tool call — the admin must confirm first.
7.  **After mutation confirmation:** acknowledge execution, then report the new state by querying the relevant read tool.

## Currency Math (Exact)

- 1 BHD = 1000 minor units (fils).
- User says \"BHD 1.5\" or \"1.500\" → convert to 1500 minor units.
- User says \"5.250\" → convert to 5250 minor units.
- Always verify: multiply decimal price by 1000, round to nearest integer.
- Format output: BHD {{major}}.{{3-digit-fils}} e.g. BHD 1.500, BHD 0.750, BHD 12.050.

## Date Math (Exact)
- today         = {today}
- yesterday     = {today} minus 1 day
- this week     = last 7 days (inclusive of today)
- last week     = 8–14 days ago
- this month    = last 30 days
- last month    = 31–60 days ago
- last N days   = exactly N days back from today
- Date range params: always YYYY-MM-DD, both inclusive

## Response Format (Exact)

1.  Start with the key finding/metric (lead with the number).
2.  Use markdown tables for any multi-column data.
3.  Use markdown lists for sequential items.
4.  End with a `---` separator, then offer 1–2 relevant follow-up questions.
5.  Footer: *Data as of {now} AST (UTC+3)*

## Common Workflows

### Barcode → Auto-Create Product (scan to catalog)
1. User gives you a barcode number (e.g. \"6294012345678\")
2. Call `smart_barcode_lookup(barcode)` — this returns product name, brand, size, suggested category
3. Present the findings: \"I found **Product X** by Brand Y, 400ml. Suggested category: Dairy.\"
4. Ask: \"What price should I set for this product in BHD?\"
5. When user replies with the price (e.g. \"BHD 1.500\"), validate the amount
6. Run `list_categories` to find the matching category_id
7. Call `create_product(name, price_minor, category_id, barcode=...)` (CONFIRMATION REQUIRED)
8. After confirmation: \"Product created! It's now available in the POS.\"

### Check Competitive Prices (research before setting your own)
1. User asks: \"What's the market price for Nido 900g?\"
2. Call `compare_store_prices(product_name)` — searches Lulu, Carrefour, Alosra, Talabat
3. Call `bahrain_market_price_check(product_name)` — searches delivery platforms
4. Present a comparison: store names, prices found, sources
5. Recommend: \"Based on the market, I suggest setting your price between BHD X and BHD Y.\"
6. If user accepts, call `create_product` or `update_product_price`

### Full Research → Create (for uncertain products)
1. User: \"I need to add this new item to my store, barcode 1234567890123, I want to know everything about it\"
2. Call `smart_barcode_lookup` → get product details
3. Call `compare_store_prices` → get market pricing
4. Call `bahrain_market_price_check` → check delivery platforms
5. Present all findings in a structured table
6. Ask user for: name (confirm/edit), price, category choice
7. Create the product once all fields are confirmed

## Tool Categories (Summary)

Read tools: get_today_summary, get_daily_report, get_date_range_report, get_top_products, get_hourly_sales, get_sales_by_category, get_cashier_performance, get_tax_report, list_products, search_products, get_product, list_categories, get_stock_levels, get_low_stock, get_stock_movements, get_cash_summary, get_recent_refunds, get_audit_log, list_safe_drops, list_no_sale_events, get_audit_chain_status, get_sync_status, list_customers, get_customer, list_deliveries, get_shift_history, list_users, list_roles, list_tax_rules, get_store_settings, get_business_rules, list_devices, get_session_timeout, web_search, search_market_prices, compare_store_prices, bahrain_market_price_check, fetch_url, lookup_barcode, smart_barcode_lookup, get_exchange_rates, get_prayer_times, get_bahrain_holidays

Mutation tools [REQUIRE CONFIRMATION]: update_product_price, set_product_active, update_product_name, create_product, adjust_stock, stock_take, bulk_stock_take, update_reorder_point, create_customer, update_customer, advance_delivery_status, create_category, update_category, create_user, update_user, create_tax_rule, update_tax_rule, update_product_full, update_store_settings, update_business_rules, confirm_delivery_payment, cancel_delivery, backup_database

## Context
- Today: {today} | Time: {now}
- Currency: BHD (3 decimal places)
- Timezone: AST (UTC+3, Bahrain — no DST)"
    );
    header
}

// ── Shared tool loop ───────────────────────────────────────────────────────────

enum ToolLoopOutcome {
    Done { text: String },
    PendingAction {
        action_id: String,
        tool_name: String,
        preview: ToolPreview,
        expires_at: String,
        assistant_text: String,
    },
}

/// Multi-turn tool loop (max 8 read-tool calls per user message) shared by the
/// blocking and streaming paths. Callers provide no-op or event-emitting
/// callbacks for `on_tool_start`/`on_tool_done`.
async fn run_tool_loop<F, G>(
    db: &sqlx::SqlitePool,
    provider: &Provider,
    system: &str,
    input: &AiChatInput,
    tool_defs: &[ToolDef],
    mut current: ChatResult,
    on_tool_start: F,
    on_tool_done: G,
) -> AppResult<ToolLoopOutcome>
where
    F: Fn(&str) + Send,
    G: Fn(&str) + Send,
{
    const MAX_TURNS: usize = 8;
    for _turn in 0..MAX_TURNS {
        let Some(tool_call) = current.tool_call else {
            return Ok(ToolLoopOutcome::Done { text: current.text });
        };
        let assistant_text = current.text.clone();
        if tools::is_mutation_tool(&tool_call.name) {
            let preview = tools::dry_run_mutation(
                db,
                &tool_call.name,
                &tool_call.input,
                input.currency_exponent,
            )
            .await?;
            let tool_input_json = tool_call.input.to_string();
            let preview_text = format!(
                "{}: {}",
                preview.description,
                preview
                    .fields
                    .iter()
                    .map(|f| format!("{} = {}", f.label, f.value))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let action = ai_admin_repo::create_action(
                db,
                &input.user_id,
                &tool_call.name,
                &tool_input_json,
                &hash_str(&tool_input_json),
                &preview_text,
                &Ulid::new().to_string(),
            )
            .await?;
            return Ok(ToolLoopOutcome::PendingAction {
                action_id: action.action_id,
                tool_name: tool_call.name,
                preview,
                expires_at: action.expires_at,
                assistant_text,
            });
        }
        on_tool_start(&tool_call.name);
        let tool_result = match tools::execute_read_tool(
            db,
            &tool_call.name,
            &tool_call.input,
            &input.branch_id,
            input.currency_exponent,
        )
        .await
        {
            Ok(result) => result,
            Err(e) => {
                let msg = format!("Tool '{}' failed: {e}", tool_call.name);
                tracing::error!("{msg}");
                msg
            }
        };
        on_tool_done(&tool_call.name);
        let prev_reasoning = current.reasoning_content.clone();
        current = provider
            .continue_with_tool_result(
                system,
                &input.history,
                &input.message,
                &ToolCallResult {
                    id: tool_call.id,
                    name: tool_call.name,
                    input: tool_call.input,
                },
                tool_result,
                tool_defs,
                prev_reasoning,
            )
            .await?;
    }
    Ok(ToolLoopOutcome::Done { text: current.text })
}

// ── Streaming chat command ─────────────────────────────────────────────────────

#[tauri::command]
pub async fn ai_chat_stream(
    state: State<'_, AppState>,
    input: AiChatInput,
    on_event: Channel<StreamEvent>,
) -> Result<(), String> {
    ai_admin_repo::expire_old_actions(&state.db).await.ok();

    let Some(provider) = Provider::from_db(&state.db).await.map_err(|e| e.to_string())? else {
        let _ = on_event.send(StreamEvent::Error {
            message: "No AI provider configured. Set an API key in Admin Settings.".into(),
        });
        return Ok(());
    };

    let tool_defs = tools::all_tool_definitions();
    let system = build_system_prompt();

    // Only Anthropic supports streaming natively; OpenAI falls back to non-streaming
    if provider.is_anthropic() {
        let api_key = provider.api_key().to_string();
        crate::ai::streaming::run_streaming_chat(
            &state.db,
            &api_key,
            &system,
            &input,
            &tool_defs,
            &on_event,
        )
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
    } else {
        // OpenAI fallback: run blocking, emit as single token + done
        let initial = provider
            .send_chat(&system, &input.history, &input.message, &tool_defs)
            .await
            .map_err(|e| e.to_string())?;
        let ev_start = on_event.clone();
        let ev_done = on_event.clone();
        let outcome = run_tool_loop(
            &state.db,
            &provider,
            &system,
            &input,
            &tool_defs,
            initial,
            move |name| { let _ = ev_start.send(StreamEvent::ToolStart { name: name.to_string() }); },
            move |name| { let _ = ev_done.send(StreamEvent::ToolDone { name: name.to_string() }); },
        )
        .await
        .map_err(|e| e.to_string())?;
        match outcome {
            ToolLoopOutcome::Done { text } => {
                let _ = on_event.send(StreamEvent::Token { text });
                let _ = on_event.send(StreamEvent::Done);
            }
            ToolLoopOutcome::PendingAction {
                action_id,
                tool_name,
                preview,
                expires_at,
                assistant_text,
            } => {
                let _ = on_event.send(StreamEvent::MutationPending {
                    action_id,
                    tool_name,
                    preview,
                    expires_at,
                    assistant_text,
                });
            }
        }
        Ok(())
    }
}

// ── Chat history persistence commands ─────────────────────────────────────────

#[tauri::command]
pub async fn ai_save_message(
    state: State<'_, AppState>,
    session_id: String,
    branch_id: String,
    user_id: String,
    role: String,
    content: String,
    message_type: String,
) -> Result<i64, String> {
    ai_chat_history_repo::save_message(
        &state.db,
        &session_id,
        &branch_id,
        &user_id,
        &role,
        &content,
        &message_type,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_load_history(
    state: State<'_, AppState>,
    branch_id: String,
    user_id: String,
) -> Result<Vec<AiChatMessage>, String> {
    ai_chat_history_repo::load_history(&state.db, &branch_id, &user_id, 30)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_clear_history(
    state: State<'_, AppState>,
    branch_id: String,
    user_id: String,
) -> Result<(), String> {
    ai_chat_history_repo::clear_history(&state.db, &branch_id, &user_id)
        .await
        .map_err(|e| e.to_string())
}

// ── Helpers ────────────────────────────────────────────────────────────────────

/// SHA-256 hex digest of `s`. Used as a tamper-evident hash for AI action inputs.
/// DefaultHasher was previously used here but is neither cryptographically secure
/// nor guaranteed to be stable across Rust versions (CWE-327).
fn hash_str(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}
