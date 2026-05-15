use tauri::State;
use serde::Serialize;
use crate::db::repositories::{ai_admin_repo, sync_repo};
use crate::errors::AppError;
use crate::sync::central_schema::CENTRAL_SCHEMA_SQL;
use crate::sync::supabase_client::{SupabaseClient, extract_project_ref};
use crate::AppState;

const DEVICE_ID: &str = "01JDEVICE0000000000000001";

// ── sync_status ───────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn sync_status(state: State<'_, AppState>) -> Result<sync_repo::SyncStatus, AppError> {
    let worker_state = state.sync_worker.state.lock().await;
    let online = worker_state.online;
    let last_error = worker_state.last_error.clone();
    drop(worker_state);

    let mut status = sync_repo::get_sync_status(&state.db, DEVICE_ID).await?;
    status.online = online;
    // If worker has a more specific error, surface it
    if last_error.is_some() {
        status.last_error = last_error;
    }
    Ok(status)
}

// ── sync_trigger_now ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn sync_trigger_now(state: State<'_, AppState>) -> Result<String, AppError> {
    state.sync_worker.run_once().await;
    let worker_state = state.sync_worker.state.lock().await;
    if worker_state.online {
        Ok("Sync completed successfully".to_string())
    } else if let Some(ref e) = worker_state.last_error {
        Ok(format!("Sync attempted — last error: {e}"))
    } else {
        Ok("Sync not configured — enter Supabase credentials in settings".to_string())
    }
}

// ── admin_setup_supabase ──────────────────────────────────────────────────────

/// Called from the setup wizard. Validates credentials, runs the central schema
/// migration via the Supabase Management API, then stores url + service key.
/// The PAT is never stored — used only for this one migration call.
#[tauri::command]
pub async fn admin_setup_supabase(
    state:       State<'_, AppState>,
    url:         String,
    service_key: String,
    pat:         String,
) -> Result<(), AppError> {
    let url = url.trim().trim_end_matches('/').to_string();
    if url.is_empty() {
        return Err(AppError::Validation("Supabase URL is required".into()));
    }
    if service_key.is_empty() {
        return Err(AppError::Validation("Service role key is required".into()));
    }
    if pat.is_empty() {
        return Err(AppError::Validation("Personal access token is required".into()));
    }

    let client = SupabaseClient::new(url.clone(), service_key.clone());

    // Step 1: Validate service role key
    client.validate().await
        .map_err(|_| AppError::Validation("Could not connect to Supabase — check the URL and service role key".into()))?;

    // Step 2: Extract project ref and run migration
    let project_ref = extract_project_ref(&url)
        .ok_or_else(|| AppError::Validation("Invalid Supabase URL format (expected https://xyz.supabase.co)".into()))?;

    client.migrate(&pat, &project_ref, CENTRAL_SCHEMA_SQL).await
        .map_err(|e| AppError::Validation(format!("Schema migration failed: {e}")))?;

    // Step 3: Persist config (PAT is intentionally NOT stored)
    ai_admin_repo::set_config(&state.db, "supabase_url", &url).await?;
    ai_admin_repo::set_config(&state.db, "supabase_service_key", &service_key).await?;

    tracing::info!("Supabase configured: {url}");
    Ok(())
}

// ── admin_get_supabase_status ─────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SupabaseStatus {
    pub configured: bool,
}

#[tauri::command]
pub async fn admin_get_supabase_status(
    state: State<'_, AppState>,
) -> Result<SupabaseStatus, AppError> {
    let url = ai_admin_repo::get_config(&state.db, "supabase_url").await?
        .unwrap_or_default();
    let key = ai_admin_repo::get_config(&state.db, "supabase_service_key").await?
        .unwrap_or_default();
    Ok(SupabaseStatus { configured: !url.is_empty() && !key.is_empty() })
}
