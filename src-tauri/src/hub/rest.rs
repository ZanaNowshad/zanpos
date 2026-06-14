use super::HubState;
use crate::sync_v2::apply::{
    self, pk_for_table, should_skip_column, value_from_row_column, ALLOWED_CONFIG_KEYS, SYNC_TABLES,
};
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Value};
use sqlx::{Column, Row};
use std::collections::HashMap;
use std::net::SocketAddr;

pub fn router(state: HubState) -> Router {
    Router::new()
        .route("/rest/v1/", get(probe_ok))
        .route("/rest/v1/{table}", get(pull_table).post(push_table))
        .route("/zanpos/info", get(info))
        .layer(axum::extract::DefaultBodyLimit::max(32 * 1024 * 1024))
        .with_state(state)
}

fn check_auth(state: &HubState, headers: &HeaderMap, addr: &SocketAddr) -> Result<(), Response> {
    let presented = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    if super::token_digest(presented) != state.token_digest {
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
        sql.push_str(" AND datetime(updated_at) > datetime(?)");
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
