//! Trust spine: background telemetry uploader.
//!
//! Flushes the local `diagnostics` and `analytics_events` tables to a worker
//! that speaks the storefront HMAC protocol (`storefront::publisher::sign_request`)
//! so field issues are visible without asking a cashier to read a log file.
//! Runs as a background loop only; never touches the sale path.
//!
//! Two destinations are supported. When `telemetry_endpoint_url` is configured,
//! batches go to that central endpoint signed with the per-store
//! `telemetry_device_key` — this is what gives one operator visibility across a
//! fleet of shops. With no central endpoint configured the uploader falls back
//! to the shop's own storefront worker, which keeps existing installs working
//! but means a crash is only ever visible to the shop it happened in.
use serde::Serialize;
use serde_json::Value;
use sqlx::{Column, Row, SqlitePool};

const BATCH_LIMIT: i64 = 50;
const BASE_INTERVAL_SECS: u64 = 300;
const MAX_INTERVAL_SECS: u64 = 1800;
const LOCAL_CAP: i64 = 500;
const SECRET_KEY: &str = "storefront_publish_secret";
const CENTRAL_URL_KEY: &str = "telemetry_endpoint_url";
const CENTRAL_SECRET_KEY: &str = "telemetry_device_key";
const TELEMETRY_PATH: &str = "/api/telemetry";

/// Starts the background upload loop. Fire-and-forget: failures back off,
/// they never propagate anywhere a cashier would see them.
pub fn spawn(pool: SqlitePool) {
    tauri::async_runtime::spawn(async move {
        let mut interval = std::time::Duration::from_secs(BASE_INTERVAL_SECS);
        loop {
            tokio::time::sleep(interval).await;
            let ok = run_cycle(&pool).await;
            interval = if ok {
                std::time::Duration::from_secs(BASE_INTERVAL_SECS)
            } else {
                std::cmp::min(
                    interval * 2,
                    std::time::Duration::from_secs(MAX_INTERVAL_SECS),
                )
            };
        }
    });
}

async fn run_cycle(pool: &SqlitePool) -> bool {
    let d = upload_table(pool, "diagnostics").await;
    let a = upload_table(pool, "analytics_events").await;
    let _ = prune_table(pool, "diagnostics").await;
    let _ = prune_table(pool, "analytics_events").await;
    d && a
}

async fn app_config(pool: &SqlitePool, key: &str) -> Option<String> {
    sqlx::query_scalar("SELECT value FROM app_config WHERE key=?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .filter(|v: &String| !v.is_empty())
}

/// Same "per-shop store identifier" the catalog publish flow uses
/// (`CatalogBranch.id` in storefront/catalog.rs) — the active branch's id.
async fn active_branch_id(pool: &SqlitePool) -> Option<String> {
    sqlx::query_scalar(
        "SELECT branch_id FROM branches WHERE is_active=1 AND deleted_at IS NULL ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

fn row_to_json(row: &sqlx::sqlite::SqliteRow) -> Value {
    let mut map = serde_json::Map::new();
    for col in row.columns() {
        let name = col.name();
        map.insert(
            name.to_string(),
            crate::sync_v2::apply::value_from_row_column(row, name),
        );
    }
    Value::Object(map)
}

/// Where one batch is going, and the secret it is signed with.
struct Destination {
    base_url: String,
    secret: String,
    store: String,
}

/// Central endpoint wins when it is fully configured. A central URL with no
/// device key is a half-finished setup, not an instruction to drop telemetry,
/// so it falls back to the shop's own worker rather than failing closed.
fn choose_destination(
    central: Option<(String, String)>,
    shop: Option<(String, String)>,
    store: String,
) -> Option<Destination> {
    let (base_url, secret) = central.or(shop)?;
    Some(Destination {
        base_url,
        secret,
        store,
    })
}

async fn resolve_destination(pool: &SqlitePool) -> Option<Destination> {
    let store = active_branch_id(pool).await?;
    let central = match (
        app_config(pool, CENTRAL_URL_KEY).await,
        crate::secure_store::get_secret(CENTRAL_SECRET_KEY),
    ) {
        (Some(url), Some(key)) => Some((url, key)),
        (Some(_), None) => {
            tracing::warn!(
                "telemetry_uploader: {CENTRAL_URL_KEY} is set but {CENTRAL_SECRET_KEY} is missing — \
                 falling back to this shop's storefront worker"
            );
            None
        }
        (None, _) => None,
    };
    let shop = match (
        app_config(pool, "storefront_publish_url").await,
        crate::secure_store::get_secret(SECRET_KEY),
    ) {
        (Some(url), Some(secret)) => Some((url, secret)),
        _ => None,
    };
    choose_destination(central, shop, store)
}

#[derive(Serialize)]
struct TelemetryBatch<'a> {
    store: &'a str,
    table: &'a str,
    device_id: &'a str,
    date: &'a str,
    records: &'a [Value],
}

/// Signs and POSTs one batch. Every failure mode (missing config, network
/// error, non-2xx) returns `false` so the caller backs off; nothing here
/// panics or surfaces to the UI.
async fn post_batch(destination: &Destination, table: &str, records: &[Value]) -> bool {
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let device_id = crate::diagnostics::device_id();
    let body = TelemetryBatch {
        store: &destination.store,
        table,
        device_id: &device_id,
        date: &date,
        records,
    };
    let Ok(body_bytes) = serde_json::to_vec(&body) else {
        return false;
    };
    let timestamp = chrono::Utc::now().timestamp().to_string();
    let signature = crate::storefront::publisher::sign_request(
        destination.secret.as_bytes(),
        "POST",
        TELEMETRY_PATH,
        &timestamp,
        &body_bytes,
    );
    let idempotency_key = ulid::Ulid::new().to_string();
    let Ok(client) = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(15))
        .build()
    else {
        return false;
    };
    let url = format!(
        "{}{}",
        destination.base_url.trim_end_matches('/'),
        TELEMETRY_PATH
    );
    match client
        .post(url)
        .header("Content-Type", "application/json")
        .header("x-zanpos-timestamp", timestamp)
        .header("x-idempotency-key", idempotency_key)
        .header("x-zanpos-signature", signature)
        .body(body_bytes)
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

async fn upload_table(pool: &SqlitePool, table: &str) -> bool {
    let rows = match sqlx::query(&format!(
        "SELECT * FROM {table} WHERE uploaded_at IS NULL ORDER BY ts LIMIT {BATCH_LIMIT}"
    ))
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("telemetry_uploader: select {table} failed: {e}");
            return false;
        }
    };
    if rows.is_empty() {
        return true;
    }

    let Some(destination) = resolve_destination(pool).await else {
        return false;
    };

    let ids: Vec<String> = rows.iter().map(|r| r.get::<String, _>("id")).collect();
    let records: Vec<Value> = rows.iter().map(row_to_json).collect();

    if !post_batch(&destination, table, &records).await {
        return false;
    }

    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("UPDATE {table} SET uploaded_at=? WHERE id IN ({placeholders})");
    let mut query = sqlx::query(&sql).bind(chrono::Utc::now().to_rfc3339());
    for id in &ids {
        query = query.bind(id);
    }
    if let Err(e) = query.execute(pool).await {
        tracing::warn!("telemetry_uploader: marking {table} uploaded failed: {e}");
        return false;
    }
    // Emitted for `diagnostics` only, never for `analytics_events`. Recording
    // an analytics row every time analytics uploads would feed itself: each
    // batch would create the next batch's reason to run, and the table would
    // never drain. Scoped this way it is bounded — one row per crash batch.
    if table == "diagnostics" {
        crate::diagnostics::record_event(
            pool,
            "crash_uploaded",
            Some(serde_json::json!({ "count": ids.len() })),
        )
        .await;
    }
    true
}

/// Keeps each table at or under `LOCAL_CAP`, preferring to delete rows that
/// already made it to the server before touching anything still pending.
async fn prune_table(pool: &SqlitePool, table: &str) -> Result<(), sqlx::Error> {
    prune_table_to_cap(pool, table, LOCAL_CAP).await
}

async fn prune_table_to_cap(pool: &SqlitePool, table: &str, cap: i64) -> Result<(), sqlx::Error> {
    let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await?;
    let mut over = count - cap;
    if over <= 0 {
        return Ok(());
    }

    let uploaded_count: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM {table} WHERE uploaded_at IS NOT NULL"
    ))
    .fetch_one(pool)
    .await?;
    let delete_uploaded = over.min(uploaded_count);
    if delete_uploaded > 0 {
        sqlx::query(&format!(
            "DELETE FROM {table} WHERE id IN (SELECT id FROM {table} WHERE uploaded_at IS NOT NULL ORDER BY ts ASC LIMIT ?)"
        ))
        .bind(delete_uploaded)
        .execute(pool)
        .await?;
        over -= delete_uploaded;
    }
    if over > 0 {
        sqlx::query(&format!(
            "DELETE FROM {table} WHERE id IN (SELECT id FROM {table} ORDER BY ts ASC LIMIT ?)"
        ))
        .bind(over)
        .execute(pool)
        .await?;
    }
    Ok(())
}

#[derive(serde::Serialize)]
pub struct FlushResult {
    pub ok: bool,
    pub message: String,
}

#[tauri::command]
pub async fn flush_diagnostics_now(
    state: tauri::State<'_, crate::AppState>,
) -> Result<FlushResult, String> {
    let d = upload_table(&state.db, "diagnostics").await;
    let a = upload_table(&state.db, "analytics_events").await;
    Ok(FlushResult {
        ok: d && a,
        message: if d && a {
            "Diagnostics sent".into()
        } else {
            "Some data queued locally, will retry".into()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("memory db");
        sqlx::query(
            "CREATE TABLE diagnostics (
                id TEXT PRIMARY KEY NOT NULL, ts TEXT NOT NULL, device_id TEXT NOT NULL,
                severity TEXT NOT NULL, kind TEXT NOT NULL, message TEXT NOT NULL,
                stack TEXT, app_version TEXT NOT NULL, extra_json TEXT, uploaded_at TEXT
            )",
        )
        .execute(&pool)
        .await
        .expect("diagnostics table");
        pool
    }

    async fn seed_rows(pool: &SqlitePool, n: usize, uploaded: bool) {
        // Tests seed both uploaded and pending batches in the same table, so
        // the id must stay unique across calls — prefix by upload state.
        let prefix = if uploaded { "uploaded" } else { "pending" };
        for i in 0..n {
            sqlx::query(
                "INSERT INTO diagnostics (id, ts, device_id, severity, kind, message, app_version, uploaded_at)
                 VALUES (?,?,?,?,?,?,?,?)",
            )
            .bind(format!("{prefix}-{i}"))
            .bind(format!("2026-01-01T00:00:{:02}Z", i % 60))
            .bind("dev")
            .bind("error")
            .bind("test")
            .bind("msg")
            .bind("0.0.0")
            .bind(if uploaded { Some("2026-01-01T00:00:00Z") } else { None })
            .execute(pool)
            .await
            .expect("seed row");
        }
    }

    fn pair(url: &str, secret: &str) -> Option<(String, String)> {
        Some((url.into(), secret.into()))
    }

    #[test]
    fn central_endpoint_wins_over_the_shops_own_worker() {
        let chosen = choose_destination(
            pair("https://fleet.example", "fleet-key"),
            pair("https://shop.example", "shop-key"),
            "branch-1".into(),
        )
        .expect("a destination");
        assert_eq!(chosen.base_url, "https://fleet.example");
        assert_eq!(chosen.secret, "fleet-key");
    }

    #[test]
    fn falls_back_to_the_shop_worker_when_no_central_endpoint_is_set() {
        let chosen = choose_destination(None, pair("https://shop.example", "shop-key"), "b".into())
            .expect("a destination");
        assert_eq!(chosen.base_url, "https://shop.example");
    }

    #[test]
    fn no_destination_when_nothing_is_configured() {
        assert!(choose_destination(None, None, "b".into()).is_none());
    }

    #[tokio::test]
    async fn upload_table_is_success_on_an_empty_batch() {
        let pool = test_pool().await;
        assert!(upload_table(&pool, "diagnostics").await);
    }

    #[tokio::test]
    async fn upload_table_fails_closed_without_storefront_config() {
        // No storefront_publish_url / secret configured — must report
        // failure (so the caller backs off) rather than panicking or
        // silently dropping the rows.
        let pool = test_pool().await;
        seed_rows(&pool, 1, false).await;
        assert!(!upload_table(&pool, "diagnostics").await);
        let still_pending: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM diagnostics WHERE uploaded_at IS NULL")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(still_pending, 1);
    }

    #[tokio::test]
    async fn prune_table_prefers_deleting_already_uploaded_rows_first() {
        let pool = test_pool().await;
        seed_rows(&pool, 3, true).await; // uploaded
        seed_rows(&pool, 3, false).await; // pending

        prune_table_to_cap(&pool, "diagnostics", 4).await.unwrap();

        let pending: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM diagnostics WHERE uploaded_at IS NULL")
                .fetch_one(&pool)
                .await
                .unwrap();
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM diagnostics")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(total, 4);
        assert_eq!(pending, 3, "pending rows must survive before uploaded ones");
    }

    #[tokio::test]
    async fn prune_table_deletes_pending_once_uploaded_rows_are_exhausted() {
        let pool = test_pool().await;
        seed_rows(&pool, 1, true).await;
        seed_rows(&pool, 5, false).await;

        prune_table_to_cap(&pool, "diagnostics", 3).await.unwrap();

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM diagnostics")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(total, 3);
    }
}
