//! The two halves of the wire protocol: what the hub serves, and what it accepts.
//!
//! Split out of `rest.rs` for size, but `pull_table` and `push_table` belong in
//! one file for the same reason `parity` and `parity_rows` do — they are one
//! conversation seen from two ends, and every rule about which columns cross the
//! wire has to hold identically in both. A change applied to only one of them is
//! how a column starts syncing in one direction.

use super::rest::{check_auth, row_to_json};
use super::HubState;
use crate::sync_v2::apply::{self, pk_for_table, ALLOWED_CONFIG_KEYS, SYNC_TABLES};
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::Value;
use std::collections::HashMap;
use std::net::SocketAddr;

pub(super) async fn pull_table(
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
            let body: Vec<Value> = rows.iter().map(|row| row_to_json(&table, row)).collect();
            Json(body).into_response()
        }
        Err(e) => {
            tracing::warn!("Hub pull {table} failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response()
        }
    }
}

pub(super) async fn push_table(
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
        let mut inbound = row.clone();
        if table == "devices" {
            if let Some(object) = inbound.as_object_mut() {
                for hub_owned in [
                    "last_heartbeat_at",
                    "last_seen_at",
                    "observed_ip",
                    "app_version",
                    "heartbeat_seq",
                    "heartbeat_hub_id",
                ] {
                    object.remove(hub_owned);
                }
            }
        }
        if let Err(e) = apply::apply_row(&state.pool, &table, &inbound).await {
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
