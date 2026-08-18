use super::HubState;
use crate::sync_v2::apply::{
    self, pk_for_table, should_skip_column, value_from_row_column, ALLOWED_CONFIG_KEYS, SYNC_TABLES,
};
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use futures::stream;
use serde_json::{json, Value};
use sqlx::{Column, Row};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::time::Duration;

pub fn router(state: HubState) -> Router {
    Router::new()
        .route("/rest/v1/", get(probe_ok))
        .route("/rest/v1/{table}", get(pull_table).post(push_table))
        .route("/zanpos/info", get(info))
        .route("/zanpos/health", get(health))
        .route("/zanpos/consistency", get(consistency))
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

fn check_auth(state: &HubState, headers: &HeaderMap, addr: &SocketAddr) -> Result<(), Response> {
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
    if let Some(dev) = headers.get("x-zanpos-device").and_then(|v| v.to_str().ok()) {
        if !dev.is_empty() {
            let now = chrono::Utc::now().to_rfc3339();
            if let Ok(mut m) = state.seen.lock() {
                m.insert(dev.to_string(), (addr.ip().to_string(), now));
            }
        }
    }
    Ok(())
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
        crate::commands::sync_commands::SYNC_TABLES,
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

async fn pull_table(
    State(state): State<HubState>,
    Path(table): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Err(r) = check_auth(&state, &headers, &addr) {
        return r;
    }
    if !SYNC_TABLES.contains(&table.as_str()) {
        return (StatusCode::NOT_FOUND, "unknown table").into_response();
    }
    let pk = pk_for_table(&table);
    let limit: i64 = q
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(500)
        .clamp(1, 1000);
    let offset: i64 = q
        .get("offset")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
        .max(0);
    let since = q
        .get("updated_at")
        .and_then(|v| v.strip_prefix("gt."))
        .map(str::to_string);
    let neq_dev = q
        .get("origin_device_id")
        .and_then(|v| v.strip_prefix("neq."))
        .map(str::to_string);
    let active_only = q
        .get("is_active")
        .map(|v| v == "eq.true" || v == "eq.1")
        .unwrap_or(false);

    let mut sql = format!("SELECT * FROM {table} WHERE 1=1");
    if since.is_some() {
        // Millisecond precision on both sides. datetime() truncates to whole
        // seconds, so every row written in the same second as the watermark
        // compared equal and was skipped by `>` — permanently, since the
        // watermark only moves forward. strftime also normalises the two
        // timestamp formats in use ("2026-01-01 10:00:00" from the importer and
        // RFC3339 from everything else), which a raw text compare does not.
        sql.push_str(
            " AND strftime('%Y-%m-%dT%H:%M:%f', updated_at) \
              > strftime('%Y-%m-%dT%H:%M:%f', ?)",
        );
    }
    if neq_dev.is_some() {
        sql.push_str(" AND origin_device_id <> ?");
    }
    if active_only {
        sql.push_str(" AND is_active = 1");
    }
    if table == "app_config" {
        let list = ALLOWED_CONFIG_KEYS
            .iter()
            .map(|k| format!("'{k}'"))
            .collect::<Vec<_>>()
            .join(",");
        sql.push_str(&format!(" AND key IN ({list})"));
    }
    sql.push_str(&format!(
        " ORDER BY datetime(updated_at) ASC, {pk} ASC LIMIT ? OFFSET ?"
    ));

    let mut query = sqlx::query(&sql);
    if let Some(s) = &since {
        query = query.bind(s);
    }
    if let Some(d) = &neq_dev {
        query = query.bind(d);
    }
    query = query.bind(limit).bind(offset);

    match query.fetch_all(&state.pool).await {
        Ok(rows) => {
            let body: Vec<Value> = rows
                .iter()
                .map(|row| {
                    let mut map = serde_json::Map::new();
                    for col in row.columns() {
                        let name = col.name();
                        if should_skip_column(&table, name) {
                            continue;
                        }
                        map.insert(name.to_string(), value_from_row_column(row, name));
                    }
                    Value::Object(map)
                })
                .collect();
            Json(body).into_response()
        }
        Err(e) => {
            tracing::warn!("Hub pull {table} failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response()
        }
    }
}

async fn push_table(
    State(state): State<HubState>,
    Path(table): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(rows): Json<Vec<Value>>,
) -> Response {
    if let Err(r) = check_auth(&state, &headers, &addr) {
        return r;
    }
    if !SYNC_TABLES.contains(&table.as_str()) {
        return (StatusCode::NOT_FOUND, "unknown table").into_response();
    }
    let origin_device_id = headers
        .get("x-zanpos-device")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    for row in &rows {
        if let Err(e) = apply::apply_row(&state.pool, &table, row).await {
            tracing::warn!("Hub apply {table} failed: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("apply failed: {e}"),
            )
                .into_response();
        }
    }
    if !rows.is_empty() {
        state.events.publish(&table, &origin_device_id);
    }
    StatusCode::CREATED.into_response()
}

#[cfg(test)]
mod tests {
    #[test]
    fn param_prefix_parsing() {
        assert_eq!(
            "gt.2026-01-01T00:00:00Z".strip_prefix("gt."),
            Some("2026-01-01T00:00:00Z")
        );
        assert_eq!("neq.DEV1".strip_prefix("neq."), Some("DEV1"));
    }
}
