pub mod helpers;
#[cfg(test)]
mod million_row_tests;
pub mod repositories;

use crate::errors::AppResult;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    SqlitePool,
};
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

const LEGACY_CATALOG_REPAIR_INDEX: &str = "idx_products_active_barcode_repair";

async fn prepare_legacy_migration_indexes(pool: &SqlitePool) -> bool {
    let repair_already_applied = sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(
            SELECT 1 FROM _sqlx_migrations WHERE version = 30 AND success = 1
        )",
    )
    .fetch_one(pool)
    .await
    .map(|value| value != 0)
    .unwrap_or(false);

    if repair_already_applied {
        return false;
    }

    let sql = format!(
        "CREATE INDEX IF NOT EXISTS {LEGACY_CATALOG_REPAIR_INDEX}
         ON products(barcode, product_id)
         WHERE deleted_at IS NULL AND is_active = 1 AND barcode IS NOT NULL"
    );
    match sqlx::query(&sql).execute(pool).await {
        Ok(_) => {
            tracing::info!("Prepared index for legacy catalog identity migration");
            true
        }
        Err(error) if error.to_string().contains("no such table: products") => false,
        Err(error) => {
            tracing::warn!("Could not prepare legacy catalog migration index: {error}");
            false
        }
    }
}

pub async fn init_db(db_path: &str) -> AppResult<SqlitePool> {
    // Ensure parent directory exists
    if let Some(parent) = Path::new(db_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let url = format!("sqlite:{}?mode=rwc", db_path);

    // ── Per-connection settings ────────────────────────────────────────────────
    // PRAGMA settings are *per-connection* in SQLite. Using SqliteConnectOptions
    // ensures every connection spawned by the pool has them applied, not just the
    // first one that happens to be acquired at startup.
    //
    // Exceptions:
    //   journal_mode = WAL  — stored in the DB file header, persists across conns.
    //   foreign_keys        — MUST be per-connection; SQLite default is OFF.
    let connect_opts = SqliteConnectOptions::from_str(&url)?
        // WAL gives concurrent reads + write without blocking
        .journal_mode(SqliteJournalMode::Wal)
        // Enforce FK constraints on every connection — default is OFF in SQLite
        .foreign_keys(true)
        // NORMAL sync is safe in WAL mode and avoids a full-flush on every write
        .synchronous(SqliteSynchronous::Normal)
        // Wait up to 15 s for the write lock before returning SQLITE_BUSY. A large
        // catalog import and the sync worker both write; 3 s wasn't enough under that
        // contention (log showed 3.4 s waits → "database is locked"). 15 s lets the
        // writers queue and proceed instead of erroring.
        .busy_timeout(Duration::from_secs(15))
        // Increase default cache from 2 MB to 8 MB for faster repeated reads
        .pragma("cache_size", "-8000")
        // L2: WAL auto-checkpoint threshold — checkpoint when WAL exceeds ~1000 pages
        // (~4 MB at default page size).  Without this the WAL file grows unbounded until
        // the process exits and SQLite runs the passive checkpoint on shutdown.
        .pragma("wal_autocheckpoint", "1000");

    let pool = SqlitePoolOptions::new()
        // SQLite WAL allows multiple readers but only one writer at a time.
        // 6 connections: enough for concurrent Tauri commands + sync worker without
        // overwhelming the write-lock queue. WAL allows all 6 to read in parallel.
        .max_connections(6)
        // Must exceed busy_timeout (15 s) so that a slow sync INSERT (observed
        // at up to 10 s on initial Supabase pull) doesn't starve other operations
        // while holding a connection. 30 s gives safe headroom.
        .acquire_timeout(Duration::from_secs(30))
        .connect_with(connect_opts)
        .await?;

    // Migration 0030 compares every active product with earlier owners of the
    // same barcode. Large legacy catalogs need this temporary covering index;
    // otherwise SQLite may choose idx_products_active and perform an O(n²) scan
    // on the UI thread during startup.
    let prepared_catalog_repair = prepare_legacy_migration_indexes(&pool).await;

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;

    if prepared_catalog_repair {
        let sql = format!("DROP INDEX IF EXISTS {LEGACY_CATALOG_REPAIR_INDEX}");
        if let Err(error) = sqlx::query(&sql).execute(&pool).await {
            tracing::warn!("Could not remove temporary catalog migration index: {error}");
        }
    }

    // Reset rows that exhausted sync_attempts under the old bug where updated_at was
    // omitted from Supabase payloads, causing PostgreSQL 23502 on every upsert.
    // COALESCE(NULLIF(updated_at,''), ...) only overwrites NULL/empty values so valid
    // timestamps are preserved.
    for table in &[
        "branches",
        "categories",
        "products",
        "product_barcodes",
        "stock_levels",
        "users",
        "tax_rules",
        "devices",
        "customers",
        "shifts",
        "sales",
        "sale_items",
        "payments",
        "refunds",
        "refund_items",
        "stock_movements",
        "audit_logs",
        "delivery_orders",
        "product_prices",
        "cash_events",
        "po_receipts",
        "riders",
    ] {
        let sql = format!(
            "UPDATE {table} SET sync_attempts = 0, \
             updated_at = COALESCE(NULLIF(updated_at, ''), created_at, datetime('now')) \
             WHERE sync_status = 'pending'"
        );
        if let Err(e) = sqlx::query(&sql).execute(&pool).await {
            tracing::warn!("DB init: could not reset {table} sync state: {e}");
        }
    }

    tracing::info!("Database initialized at {}", db_path);
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Row;

    #[tokio::test]
    async fn legacy_catalog_repair_uses_barcode_index() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE products (
                product_id TEXT PRIMARY KEY,
                barcode TEXT,
                is_active INTEGER NOT NULL,
                deleted_at TEXT,
                updated_at TEXT,
                sync_status TEXT,
                sync_attempts INTEGER NOT NULL DEFAULT 0
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("CREATE INDEX idx_products_active ON products(is_active, deleted_at)")
            .execute(&pool)
            .await
            .unwrap();

        prepare_legacy_migration_indexes(&pool).await;

        let plan = sqlx::query(
            "EXPLAIN QUERY PLAN
             SELECT 1 FROM products canonical
             WHERE canonical.deleted_at IS NULL
               AND canonical.is_active = 1
               AND canonical.barcode = ?
               AND canonical.product_id < ?",
        )
        .bind("123")
        .bind("P2")
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.get::<String, _>(3))
        .collect::<Vec<_>>();

        assert!(plan
            .iter()
            .any(|row| row.contains("idx_products_active_barcode_repair")));
    }
}
