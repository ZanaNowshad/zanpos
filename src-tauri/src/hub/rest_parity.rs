//! The parity endpoints: comparing tables, and serving the rows behind a
//! divergence so only those get repaired.
//!
//! Split out of `rest.rs` for size, but they belong together for a reason —
//! `parity` names the rows that differ and `parity_rows` hands them over, and
//! the ceiling on how many is the same constant in both. The alternative these
//! replace was re-pulling a 28,000-row catalogue to fix one row.

use super::rest::{check_auth, row_to_json};
use super::HubState;
use crate::sync_v2::apply::{pk_for_table, ALLOWED_CONFIG_KEYS, SYNC_TABLES};
use axum::extract::{ConnectInfo, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;

/// Bucketed digests, and per-row digests inside one bucket.
///
/// The second half of the pair `/zanpos/consistency` starts: that endpoint says
/// a table differs, this one says which rows. Both shapes live behind one route
/// because they are one conversation — buckets first, then the rows in the
/// buckets that did not match.
pub(super) async fn parity(
    State(state): State<HubState>,
    Query(q): Query<HashMap<String, String>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Err(r) = check_auth(&state, &headers, &addr) {
        return r;
    }
    let table = q.get("table").cloned().unwrap_or_default();
    if !crate::sync_v2::consistency::CONSISTENCY_TABLES.contains(&table.as_str()) {
        return (StatusCode::NOT_FOUND, "unknown table").into_response();
    }
    let buckets = q
        .get("buckets")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(crate::sync_v2::parity::DEFAULT_BUCKETS)
        .clamp(1, 1024);

    // With a bucket: the rows in it. Without: the bucket list.
    let result = match q.get("bucket").and_then(|v| v.parse::<u32>().ok()) {
        Some(bucket) => crate::sync_v2::parity::row_digests(&state.pool, &table, bucket, buckets)
            .await
            .map(|rows| serde_json::json!({ "table": table, "bucket": bucket, "buckets": buckets, "rows": rows })),
        None => crate::sync_v2::parity::bucket_digests(&state.pool, &table, buckets)
            .await
            .map(|digests| serde_json::json!({ "table": table, "buckets": buckets, "digests": digests })),
    };

    match result {
        Ok(body) => Json(body).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("parity failed: {e}"),
        )
            .into_response(),
    }
}

/// The rows behind a set of primary keys, for repairing only what diverged.
///
/// A POST with a JSON body rather than a `pk=in.(a,b,c)` query filter, because
/// primary keys are arbitrary strings — an `app_config` key containing a comma
/// would split into two keys that match nothing, and no amount of encoding
/// survives axum percent-decoding the whole parameter before we see it. JSON has
/// no delimiter to collide with.
///
/// Repairing 200 named rows is the entire point: the alternative on offer was
/// re-pulling a 28,000-row catalogue to fix one.
pub(super) async fn parity_rows(
    State(state): State<HubState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if let Err(r) = check_auth(&state, &headers, &addr) {
        return r;
    }
    let table = body
        .get("table")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if !SYNC_TABLES.contains(&table.as_str()) {
        return (StatusCode::NOT_FOUND, "unknown table").into_response();
    }
    let pks: Vec<String> = body
        .get("pks")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if pks.is_empty() {
        return Json(json!({ "table": table, "rows": [] })).into_response();
    }
    // Same ceiling parity uses for reporting. Past it, a full resync is the
    // honest remedy and this endpoint should not be used to smuggle one.
    if pks.len() > crate::sync_v2::parity::MAX_REPORTED_ROWS {
        return (
            StatusCode::BAD_REQUEST,
            format!(
                "at most {} rows per repair request",
                crate::sync_v2::parity::MAX_REPORTED_ROWS
            ),
        )
            .into_response();
    }

    let pk = pk_for_table(&table);
    // Bound parameters, never interpolation: these keys arrive over the network.
    let placeholders = std::iter::repeat_n("?", pks.len())
        .collect::<Vec<_>>()
        .join(",");
    let mut sql = format!("SELECT * FROM {table} WHERE {pk} IN ({placeholders})");
    if table == "app_config" {
        // The allowlist has to hold here too, or naming a key directly would be
        // a way around the filter that `pull_table` applies.
        let list = ALLOWED_CONFIG_KEYS
            .iter()
            .map(|k| format!("'{k}'"))
            .collect::<Vec<_>>()
            .join(",");
        sql.push_str(&format!(" AND key IN ({list})"));
    }

    let mut query = sqlx::query(&sql);
    for key in &pks {
        query = query.bind(key);
    }

    match query.fetch_all(&state.pool).await {
        Ok(rows) => {
            let body: Vec<Value> = rows.iter().map(|row| row_to_json(&table, row)).collect();
            Json(json!({ "table": table, "rows": body })).into_response()
        }
        Err(e) => {
            tracing::warn!("Hub parity rows {table} failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response()
        }
    }
}
