//! Per-device pairing commands for the LAN hub.
//!
//! Split out of hub_commands.rs, which the ship gate flagged at 544 lines.
//! The storage model and rationale live in hub::pairing; this file is only the
//! command surface plus the live-map refresh.

use crate::commands::rbac;
use crate::errors::AppError;
use crate::AppState;
use tauri::State;
// ─── Per-device pairing (spec item 99) ───────────────────────────────────────
//
// The hub binds 0.0.0.0:8923, so anything on the shop WiFi can reach it. A
// single shared token could not tell one till from another and could not be
// withdrawn from a lost device without re-keying every terminal. These three
// commands manage per-device tokens; see hub::pairing for the storage model.

/// Refreshes the running server's in-memory pairing snapshot.
///
/// `check_auth` is synchronous and runs on every request, so it reads a
/// snapshot rather than the database. Without this refresh a revoked till
/// would keep authenticating until the hub restarted — which is precisely
/// when you least want to restart it.
async fn reload_paired(state: &AppState) {
    let runtime = state.hub.lock().await;
    if let Some(handle) = runtime.handle.as_ref() {
        let fresh = crate::hub::pairing::load_live(&state.db).await;
        if let Ok(mut live) = handle.paired.lock() {
            *live = fresh;
        }
    }
}

/// Pairs a device and returns its token ONCE. Only the digest is stored, so
/// this value cannot be recovered later — show it, then let it go.
#[tauri::command]
pub async fn hub_pair_device(
    session_token: String,
    device_id: String,
    device_name: String,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::OWNER_ONLY).await?;
    let token = crate::hub::pairing::pair_device(&state.db, &device_id, &device_name).await?;
    reload_paired(&state).await;
    Ok(token)
}

#[tauri::command]
pub async fn hub_revoke_device(
    session_token: String,
    device_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::OWNER_ONLY).await?;
    crate::hub::pairing::revoke_device(&state.db, &device_id).await?;
    reload_paired(&state).await;
    Ok(())
}

#[tauri::command]
pub async fn hub_list_devices(
    session_token: String,
    state: State<'_, AppState>,
) -> Result<Vec<crate::hub::pairing::PairedDeviceRow>, AppError> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::OWNER_ONLY).await?;
    crate::hub::pairing::list_devices(&state.db).await
}
