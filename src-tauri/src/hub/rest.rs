use super::HubState;
use crate::sync_v2::apply::{skip_on_wire, value_from_row_column};
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use futures::stream;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Column, Row};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::time::Duration;

pub fn router(state: HubState) -> Router {
    Router::new()
        .route("/rest/v1/", get(probe_ok))
        .route(
            "/rest/v1/{table}",
            get(super::rest_tables::pull_table).post(super::rest_tables::push_table),
        )
        .route("/zanpos/info", get(info))
        .route("/zanpos/heartbeat", axum::routing::post(heartbeat))
        .route("/zanpos/terminals", get(terminals))
        .route("/zanpos/health", get(health))
        .route("/zanpos/consistency", get(consistency))
        .route("/zanpos/parity", get(super::rest_parity::parity))
        .route(
            "/zanpos/parity/rows",
            axum::routing::post(super::rest_parity::parity_rows),
        )
        .route("/zanpos/events", get(events))
        .layer(axum::extract::DefaultBodyLimit::max(32 * 1024 * 1024))
        .with_state(state)
}

async fn events(
    State(state): State<HubState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Err(r) = check_auth(&state, &headers, &addr) {
        return r;
    }
    let receiver = state.events.subscribe();
    let event_stream = stream::unfold(receiver, |mut receiver| async move {
        loop {
            match receiver.recv().await {
                Ok(change) => {
                    let event = Event::default()
                        .event("table_changed")
                        .json_data(change)
                        .unwrap_or_else(|_| Event::default().event("table_changed"));
                    return Some((Ok::<Event, Infallible>(event), receiver));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    Sse::new(event_stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keepalive"),
        )
        .into_response()
}

/// Constant-time equality for the SHA-256 store-token digests. A plain `!=`
/// short-circuits on the first differing byte, leaking digest bytes via response
/// timing to anyone on the LAN; this compares lengths separately and then
/// XOR-accumulates over every byte so the work is independent of where (or
/// whether) the inputs diverge. `subtle` is not a dependency, so this stays local.
fn digests_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub(super) fn check_auth(
    state: &HubState,
    headers: &HeaderMap,
    _addr: &SocketAddr,
) -> Result<(), Response> {
    let presented = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    let presented_digest = super::token_digest(presented);
    let claimed_device = headers
        .get("x-zanpos-device")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // Per-device pairing first. The device id is only trusted once its token
    // matches THAT device's digest, which is what stops one terminal claiming
    // another's identity — previously the header was recorded, never verified.
    // Read from an in-memory snapshot because this function is synchronous and
    // runs on every request; a DB round-trip per call is not acceptable here.
    let paired_ok = !claimed_device.is_empty()
        && state
            .paired
            .lock()
            .map(|devices| {
                devices
                    .get(claimed_device)
                    .is_some_and(|digest| digests_eq(&presented_digest, digest))
            })
            .unwrap_or(false);

    // Legacy shared store token. Kept so a shop upgrading mid-shift does not
    // lose its second till; logged so it can be retired once every device is
    // paired.
    let shared_ok = digests_eq(&presented_digest, &state.token_digest);
    if shared_ok && !paired_ok {
        tracing::info!(
            "hub: device '{claimed_device}' authenticated with the legacy shared token — pair it to retire that path"
        );
    }

    if !paired_ok && !shared_ok {
        // Fire-and-forget: check_auth is sync and called on every route, so
        // the diagnostics insert is spawned rather than awaited — a DB
        // failure (or the flood-case rate limiter) must never delay or
        // block the 401 response itself.
        let pool = state.pool.clone();
        tauri::async_runtime::spawn(async move {
            let _ = crate::diagnostics::record(
                &pool,
                "warn",
                "hub_unauthorized",
                "invalid store token",
                None,
                None,
            )
            .await;
        });
        return Err((StatusCode::UNAUTHORIZED, "invalid store token").into_response());
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct HeartbeatRequest {
    sequence: u64,
    app_version: String,
    /// The terminal's own wall clock at send time. Absent from terminals that
    /// predate the field; the hub leaves `heartbeat_sent_at` NULL for those.
    #[serde(default)]
    sent_at: Option<String>,
}

/// Persist one authenticated terminal heartbeat using hub time and the address
/// observed by the hub. A sequence must move strictly forward, so a delayed or
/// replayed request cannot make stale metadata look fresh.
///
/// Answers are diagnoses, not just "yes/no": 404 means the hub holds no row for
/// this device at all (re-register), 410 means the row is there but turned off,
/// 403 means the pairing was revoked, and 409 — with the hub's current sequence
/// in the body — means another installation is beating under the same identity.
/// A silent 204 for any of those used to leave a terminal believing it was
/// online while the hub's `last_seen_at` stayed frozen.
async fn heartbeat(
    State(state): State<HubState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<HeartbeatRequest>,
) -> Response {
    if let Err(response) = check_auth(&state, &headers, &addr) {
        return response;
    }
    let Some(device_id) = headers
        .get("x-zanpos-device")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return (StatusCode::BAD_REQUEST, "device identity is required").into_response();
    };
    let Ok(sequence) = i64::try_from(body.sequence) else {
        return (
            StatusCode::BAD_REQUEST,
            "heartbeat sequence is out of range",
        )
            .into_response();
    };
    if sequence <= 0 {
        return (
            StatusCode::BAD_REQUEST,
            "heartbeat sequence must be positive",
        )
            .into_response();
    }
    let app_version = body.app_version.trim();
    if app_version.is_empty() || app_version.len() > 64 {
        return (StatusCode::BAD_REQUEST, "invalid app version").into_response();
    }
    let sent_at = body
        .sent_at
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            (
                s.to_string(),
                chrono::DateTime::parse_from_rfc3339(s).is_ok(),
            )
        });
    if sent_at.as_ref().is_some_and(|(_, parsed)| !parsed) {
        return (StatusCode::BAD_REQUEST, "invalid sent_at").into_response();
    }
    let sent_at = sent_at.map(|(s, _)| s);

    let now = chrono::Utc::now().to_rfc3339();
    let observed_ip = addr.ip().to_string();
    let hub_id: String = sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'device_id'")
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();

    // Read the row's state before deciding anything, so each failure mode can
    // say *why* instead of collapsing into one 404.
    struct DeviceBeat {
        heartbeat_seq: i64,
        active: bool,
        pairing_revoked: bool,
    }
    let beat = sqlx::query_as::<_, (i64, i64, i64)>(
        "SELECT COALESCE(d.heartbeat_seq, 0),
                CASE WHEN d.is_active = 1 AND d.deleted_at IS NULL THEN 1 ELSE 0 END,
                EXISTS(SELECT 1 FROM hub_paired_devices p
                        WHERE p.device_id = d.device_id AND p.revoked_at IS NOT NULL)
           FROM devices d WHERE d.device_id = ?",
    )
    .bind(device_id)
    .fetch_optional(&state.pool)
    .await
    .map(|row| {
        row.map(|(heartbeat_seq, active, pairing_revoked)| DeviceBeat {
            heartbeat_seq,
            active: active == 1,
            pairing_revoked: pairing_revoked == 1,
        })
    });

    let beat = match beat {
        Ok(Some(beat)) => beat,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "device is not registered").into_response();
        }
        Err(error) => {
            tracing::warn!("Hub heartbeat lookup failed for {device_id}: {error}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if beat.pairing_revoked {
        // A revoked device can still pass check_auth through the legacy shared
        // store token, which has no per-device revocation. Close that gap here:
        // revocation means this identity is not welcome, whatever token it
        // presents.
        return (StatusCode::FORBIDDEN, "device pairing was revoked").into_response();
    }
    if !beat.active {
        return (StatusCode::GONE, "device is deactivated on the hub").into_response();
    }
    if sequence <= beat.heartbeat_seq {
        // A later sequence is already on record for this identity. Tell the
        // caller the value so it can catch up — saying nothing made the
        // terminal report success while the hub's last_seen_at stayed frozen.
        return (
            StatusCode::CONFLICT,
            Json(json!({ "current_sequence": beat.heartbeat_seq })),
        )
            .into_response();
    }

    let mut tx = match state.pool.begin().await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::warn!("Hub heartbeat transaction failed: {error}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    // The sequence guard stays on the UPDATE itself: two beats for the same
    // device racing through this handler must not both record.
    let updated = match sqlx::query(
        "UPDATE devices
            SET last_heartbeat_at = ?, last_seen_at = ?, observed_ip = ?, app_version = ?,
                heartbeat_seq = ?, heartbeat_hub_id = ?, heartbeat_sent_at = ?
          WHERE device_id = ? AND is_active = 1 AND deleted_at IS NULL
            AND heartbeat_seq < ?",
    )
    .bind(&now)
    .bind(&now)
    .bind(&observed_ip)
    .bind(app_version)
    .bind(sequence)
    .bind(&hub_id)
    .bind(sent_at)
    .bind(device_id)
    .bind(sequence)
    .execute(&mut *tx)
    .await
    {
        Ok(result) => result.rows_affected() == 1,
        Err(error) => {
            tracing::warn!("Hub heartbeat persistence failed for {device_id}: {error}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if !updated {
        return (
            StatusCode::CONFLICT,
            Json(json!({ "current_sequence": beat.heartbeat_seq })),
        )
            .into_response();
    }

    if let Err(error) = sqlx::query(
        "UPDATE hub_paired_devices SET last_seen_at = ?
          WHERE device_id = ? AND revoked_at IS NULL",
    )
    .bind(&now)
    .bind(device_id)
    .execute(&mut *tx)
    .await
    {
        tracing::warn!("Hub pairing heartbeat persistence failed for {device_id}: {error}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    if let Err(error) = tx.commit().await {
        tracing::warn!("Hub heartbeat commit failed for {device_id}: {error}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    if let Ok(mut seen) = state.seen.lock() {
        seen.insert(device_id.to_string(), (observed_ip, now));
    }
    StatusCode::NO_CONTENT.into_response()
}

async fn terminals(
    State(state): State<HubState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = check_auth(&state, &headers, &addr) {
        return response;
    }
    match crate::commands::device_state::roster(&state.pool).await {
        Ok(rows) => Json(rows).into_response(),
        Err(error) => {
            tracing::warn!("Hub terminal roster failed: {error}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn probe_ok(
    State(state): State<HubState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Err(r) = check_auth(&state, &headers, &addr) {
        return r;
    }
    StatusCode::OK.into_response()
}

async fn info(
    State(state): State<HubState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Err(r) = check_auth(&state, &headers, &addr) {
        return r;
    }
    let row = sqlx::query(
        "SELECT branch_id, name FROM branches WHERE is_active=1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();
    let (branch_id, store_name) = row
        .map(|r| (r.get::<String, _>("branch_id"), r.get::<String, _>("name")))
        .unwrap_or_default();
    Json(json!({
        "store_name": store_name,
        "branch_id": branch_id,
        "hub_version": env!("CARGO_PKG_VERSION"),
        "hub_time": chrono::Utc::now().to_rfc3339(),
    }))
    .into_response()
}

async fn health(
    State(state): State<HubState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Err(r) = check_auth(&state, &headers, &addr) {
        return r;
    }

    let mut report = match crate::commands::system_health_commands::run_local_health_check(
        &state.pool,
        &crate::commands::sync_commands::SYNC_TABLES,
    )
    .await
    {
        Ok(report) => report,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("health check failed: {e}"),
            )
                .into_response();
        }
    };

    let seen = state.seen.lock().map(|m| m.clone()).unwrap_or_default();
    crate::commands::system_health_commands::merge_seen_devices(&mut report, &seen);
    Json(report).into_response()
}

async fn consistency(
    State(state): State<HubState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Err(r) = check_auth(&state, &headers, &addr) {
        return r;
    }
    match crate::sync_v2::consistency::snapshot(&state.pool).await {
        Ok(snapshot) => Json(snapshot).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("consistency failed: {e}"),
        )
            .into_response(),
    }
}

/// A row as the sync protocol sees it: every column except the per-device
/// bookkeeping that must not travel.
pub(super) fn row_to_json(table: &str, row: &sqlx::sqlite::SqliteRow) -> Value {
    let mut map = serde_json::Map::new();
    for col in row.columns() {
        let name = col.name();
        if skip_on_wire(table, name) {
            continue;
        }
        map.insert(name.to_string(), value_from_row_column(row, name));
    }
    Value::Object(map)
}

#[cfg(test)]
#[path = "rest_tests.rs"]
mod tests;
