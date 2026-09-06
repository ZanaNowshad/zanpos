//! Joining a hub, from the terminal's side.
//!
//! Split from `hub_commands` for size. The seam is the direction of the
//! relationship: this file is what a terminal does to attach itself to someone
//! else's hub — probe it, check the schema is compatible, store the URL and
//! token. `hub_commands` next door is what a machine does to *be* a hub.

use crate::commands::hub_commands::{hub_status, read_cfg, HubStatus};
use crate::commands::rbac;
use crate::errors::AppError;
use crate::sync_v2::client::HttpSyncClient;
use crate::AppState;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Serialize)]
pub struct HubTestResult {
    pub ok: bool,
    pub store_name: Option<String>,
    pub error: Option<String>,
}

fn warn_on_clock_skew(hub_time: &str) {
    if let Ok(hub_t) = chrono::DateTime::parse_from_rfc3339(hub_time) {
        let skew = (chrono::Utc::now() - hub_t.with_timezone(&chrono::Utc))
            .num_seconds()
            .abs();
        if skew > 120 {
            tracing::warn!(
                "Clock skew vs hub is {skew}s (>120s). Sync conflict resolution \
                 depends on clocks — enable Windows time sync on both machines."
            );
        }
    }
}

fn normalize_hub_url(raw: &str) -> String {
    let s = raw.trim().trim_end_matches('/');
    let with_scheme = if s.starts_with("http://") || s.starts_with("https://") {
        s.to_string()
    } else {
        format!("http://{s}")
    };
    let after = with_scheme.split_once("://").map(|x| x.1).unwrap_or("");
    if after.contains(':') {
        with_scheme
    } else {
        format!("{with_scheme}:8923")
    }
}

pub(crate) fn join_schema_compatible(local_version: i64, hub_version: i64) -> bool {
    local_version > 0 && local_version == hub_version
}

async fn verify_join_schema(
    client: &HttpSyncClient,
    pool: &sqlx::SqlitePool,
) -> Result<(), AppError> {
    let local_version = crate::sync_v2::consistency::schema_version(pool).await;
    let hub = client.hub_consistency().await.map_err(|error| {
        AppError::Validation(format!(
            "Could not verify the hub database version. Update ZANPOS on the hub and try again: {error}"
        ))
    })?;
    if !join_schema_compatible(local_version, hub.schema_version) {
        return Err(AppError::Validation(format!(
            "ZANPOS versions do not match. This terminal uses database version {local_version}, but the hub uses version {}. Update ZANPOS on both devices before joining.",
            hub.schema_version
        )));
    }
    Ok(())
}

#[tauri::command]
pub async fn hub_test_connection(
    url: String,
    token: String,
    state: State<'_, AppState>,
) -> Result<HubTestResult, AppError> {
    let url = normalize_hub_url(&url);
    let client = HttpSyncClient::new(&url, &token, None);
    match client.hub_info().await {
        Ok(info) => match verify_join_schema(&client, &state.db).await {
            Ok(()) => Ok(HubTestResult {
                ok: true,
                store_name: Some(info.store_name),
                error: None,
            }),
            Err(error) => Ok(HubTestResult {
                ok: false,
                store_name: Some(info.store_name),
                error: Some(error.user_message().to_string()),
            }),
        },
        Err(AppError::Validation(m)) => Ok(HubTestResult {
            ok: false,
            store_name: None,
            error: Some(m),
        }),
        Err(e) => Ok(HubTestResult {
            ok: false,
            store_name: None,
            error: Some(format!(
                "Cannot reach hub: {e}. Check the IP, that ZANPOS is running on the hub, \
             and that Windows Firewall allows ZANPOS on private networks."
            )),
        }),
    }
}

#[derive(Deserialize)]
pub struct HubJoinInput {
    pub hub_url: String,
    pub token: String,
    pub device_name: String,
    pub device_code: String,
}

#[tauri::command]
pub async fn hub_join(
    input: HubJoinInput,
    state: State<'_, AppState>,
) -> Result<crate::commands::setup_commands::AppConfig, AppError> {
    let already: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key='setup_complete'")
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .flatten();
    if already.as_deref() == Some("1") {
        return Err(AppError::Permission(
            "This terminal is already set up. Use Settings → Hub to change hubs.".into(),
        ));
    }
    let device_name = input.device_name.trim().to_string();
    let device_code = input.device_code.trim().to_uppercase();
    if device_name.is_empty() {
        return Err(AppError::Validation("Device name is required".into()));
    }
    if device_code.len() < 2 {
        return Err(AppError::Validation(
            "Device code must be at least 2 characters".into(),
        ));
    }
    let url = normalize_hub_url(&input.hub_url);
    let client = HttpSyncClient::new(&url, &input.token, None);
    let info = client
        .hub_info()
        .await
        .map_err(|e| AppError::Validation(format!("Could not connect to the hub: {e}")))?;
    warn_on_clock_skew(&info.hub_time);
    verify_join_schema(&client, &state.db).await?;
    let branch_val = client
        .pull_branch()
        .await
        .map_err(|e| AppError::Validation(format!("Failed to read store data from hub: {e}")))?;

    if !crate::secure_store::set_secret("hub_store_token", &input.token) {
        return Err(AppError::Internal(
            "Windows Credential Manager unavailable — cannot store the store token securely."
                .into(),
        ));
    }

    let now = chrono::Utc::now().to_rfc3339();
    let central_name = branch_val["name"].as_str().unwrap_or("").to_string();
    if central_name.is_empty() {
        return Err(AppError::Validation(
            "Store data from hub is incomplete".into(),
        ));
    }

    crate::sync_v2::apply::apply_row(&state.db, "branches", &branch_val).await?;
    let branch_id = info.branch_id.clone();
    sqlx::query("UPDATE branches SET is_active=0 WHERE branch_id <> ?")
        .bind(&branch_id)
        .execute(&state.db)
        .await?;

    let device_id = ulid::Ulid::new().to_string();
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM devices WHERE is_active = 0")
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT OR REPLACE INTO devices
           (device_id, branch_id, device_code, name, status, is_active, created_at, updated_at)
         VALUES (?,?,?,?,'online',1,?,?)",
    )
    .bind(&device_id)
    .bind(&branch_id)
    .bind(&device_code)
    .bind(&device_name)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    for (k, v) in [
        ("device_id", device_id.as_str()),
        ("hub_url", url.as_str()),
        ("setup_complete", "1"),
        ("join_snapshot_initialized", "0"),
    ] {
        sqlx::query("INSERT INTO app_config(key,value,updated_at) VALUES (?,?,?)
                     ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at")
            .bind(k).bind(v).bind(&now).execute(&mut *tx).await?;
    }
    tx.commit().await?;

    client
        .upsert_rows(
            "devices",
            &[serde_json::json!({
                "device_id": device_id,
                "branch_id": branch_id,
                "device_code": device_code,
                "name": device_name,
                "status": "online",
                "is_active": 1,
                "next_receipt_seq": 1,
                "created_at": now,
                "updated_at": now
            })],
        )
        .await
        .map_err(|e| AppError::Internal(format!("Terminal registration failed: {e}")))?;

    crate::commands::setup_commands::app_config_load(state).await
}

#[tauri::command]
pub async fn hub_connect_existing(
    session_token: String,
    hub_url: String,
    token: String,
    state: State<'_, AppState>,
) -> Result<HubStatus, AppError> {
    rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;
    if read_cfg(&state.db, "hub_mode").await.as_deref() == Some("1") {
        return Err(AppError::Validation("This device IS the hub.".into()));
    }
    let url = normalize_hub_url(&hub_url);
    let client = HttpSyncClient::new(&url, &token, None);
    let info = client
        .hub_info()
        .await
        .map_err(|e| AppError::Validation(format!("Could not connect to the hub: {e}")))?;
    warn_on_clock_skew(&info.hub_time);
    verify_join_schema(&client, &state.db).await?;

    if !crate::secure_store::set_secret("hub_store_token", &token) {
        return Err(AppError::Internal(
            "Windows Credential Manager unavailable.".into(),
        ));
    }

    let device_id: String =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key='device_id'")
            .fetch_optional(&state.db)
            .await?
            .unwrap_or_default();

    let now = chrono::Utc::now().to_rfc3339();
    let mut tx = state.db.begin().await?;
    sqlx::query("INSERT INTO app_config(key,value,updated_at) VALUES ('hub_url',?,?)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at")
        .bind(&url).bind(&now).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM app_config WHERE key IN ('supabase_url','supabase_service_key','schema_migrated','cloud_grace_deadline')")
        .execute(&mut *tx).await?;
    sqlx::query("DELETE FROM app_config WHERE key LIKE 'sync_v2_watermark_%'")
        .execute(&mut *tx)
        .await?;
    if !device_id.is_empty() {
        // Every table whose rows carry this terminal's origin is offered to the
        // hub again. Derived from the schema-pinned has_origin_device_id list
        // rather than kept by hand — the copy here once omitted loyalty_events.
        for t in crate::sync_v2::apply::SYNC_TABLES
            .iter()
            .filter(|t| crate::sync_v2::apply::has_origin_device_id(t))
        {
            let sql = format!(
                "UPDATE {t} SET sync_status='pending', sync_attempts=0 WHERE origin_device_id = ?"
            );
            sqlx::query(&sql).bind(&device_id).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    crate::secure_store::delete_secret("supabase_service_key");

    let worker = state.sync_worker.clone();
    tauri::async_runtime::spawn(async move {
        worker.run_once_wait().await;
    });
    hub_status(session_token, state).await
}

#[tauri::command]
pub async fn hub_set_url(
    session_token: String,
    hub_url: String,
    state: State<'_, AppState>,
) -> Result<HubStatus, AppError> {
    rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;
    let token = crate::secure_store::get_secret("hub_store_token")
        .ok_or_else(|| AppError::Validation("No store token on this terminal.".into()))?;
    let url = normalize_hub_url(&hub_url);
    let client = HttpSyncClient::new(&url, &token, None);
    client
        .hub_info()
        .await
        .map_err(|e| AppError::Validation(format!("Could not connect: {e}")))?;
    verify_join_schema(&client, &state.db).await?;
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO app_config(key,value,updated_at) VALUES ('hub_url',?,?)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at")
        .bind(&url).bind(&now).execute(&state.db).await?;
    hub_status(session_token, state).await
}
