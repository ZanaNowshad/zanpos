use crate::errors::AppResult;
use crate::sync_v2::apply::{
    pk_for_table, skip_in_fingerprint, value_from_row_column, ALLOWED_CONFIG_KEYS,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Column, Row, SqlitePool};

/// Tables compared when checking whether a terminal matches the hub.
///
/// Derived from [`crate::sync_v2::registry`], which is what guarantees this
/// covers every synced table except the one carrying a written exemption.
pub static CONSISTENCY_TABLES: std::sync::LazyLock<Vec<&'static str>> =
    std::sync::LazyLock::new(crate::sync_v2::registry::parity_checked);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConsistencyTableSnapshot {
    pub table: String,
    pub count: i64,
    pub checksum: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConsistencySnapshot {
    pub generated_at: String,
    pub schema_version: i64,
    pub tables: Vec<ConsistencyTableSnapshot>,
}

pub async fn schema_version(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations")
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or(0)
}

pub async fn snapshot(pool: &SqlitePool) -> AppResult<ConsistencySnapshot> {
    let mut tables = Vec::with_capacity(CONSISTENCY_TABLES.len());
    for table in CONSISTENCY_TABLES.iter() {
        tables.push(table_snapshot(pool, table).await?);
    }
    Ok(ConsistencySnapshot {
        generated_at: chrono::Utc::now().to_rfc3339(),
        schema_version: schema_version(pool).await,
        tables,
    })
}

/// One row, reduced to the exact bytes the table checksum is built from.
///
/// Extracted so `parity` can narrow a mismatch to individual rows using the
/// *same* definition of equality. If the two ever diverged the drill-down would
/// point at rows the table checksum considers identical — which looks like the
/// parity tool lying, and is worse than not having one.
pub fn row_fingerprint(table: &str, row: &sqlx::sqlite::SqliteRow) -> String {
    let mut map = serde_json::Map::new();
    for col in row.columns() {
        let name = col.name();
        if skip_in_fingerprint(table, name) {
            continue;
        }
        map.insert(name.to_string(), value_from_row_column(row, name));
    }
    serde_json::to_string(&map).unwrap_or_default()
}

pub async fn table_snapshot(pool: &SqlitePool, table: &str) -> AppResult<ConsistencyTableSnapshot> {
    let pk = pk_for_table(table);
    let mut sql = format!("SELECT * FROM {table}");
    if table == "app_config" {
        let list = ALLOWED_CONFIG_KEYS
            .iter()
            .map(|k| format!("'{k}'"))
            .collect::<Vec<_>>()
            .join(",");
        sql.push_str(&format!(" WHERE key IN ({list})"));
    }
    sql.push_str(&format!(" ORDER BY {pk} ASC"));

    let rows = sqlx::query(&sql).fetch_all(pool).await?;
    let mut hasher = Sha256::new();
    for row in &rows {
        hasher.update(row_fingerprint(table, row).as_bytes());
        hasher.update(b"\n");
    }
    Ok(ConsistencyTableSnapshot {
        table: table.to_string(),
        count: rows.len() as i64,
        checksum: hex::encode(hasher.finalize()),
    })
}

pub fn consistency_score(total: usize, mismatched: usize) -> u8 {
    if total == 0 {
        return 100;
    }
    let ok = total.saturating_sub(mismatched);
    ((ok * 100) / total) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consistency_score_counts_table_mismatches() {
        assert_eq!(consistency_score(0, 0), 100);
        assert_eq!(consistency_score(6, 0), 100);
        assert_eq!(consistency_score(6, 1), 83);
        assert_eq!(consistency_score(6, 6), 0);
    }

    async fn migrated_pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    /// The fingerprint decides what two terminals must agree about, so widening
    /// it changes every row's checksum at once and makes the whole fleet report
    /// divergence until the last terminal upgrades.
    ///
    /// That is the specific hazard in letting `version` cross the wire, since
    /// the wire filter and this one used to be a single function. This test pins
    /// the boundary: the fingerprint must keep excluding per-device bookkeeping
    /// no matter what the wire decides to carry.
    #[tokio::test]
    async fn per_device_bookkeeping_stays_out_of_the_fingerprint() {
        for table in ["products", "customers", "sales", "shifts", "devices"] {
            for column in ["sync_status", "sync_attempts", "version"] {
                assert!(
                    skip_in_fingerprint(table, column),
                    "{table}.{column} entered the fingerprint — every terminal \
                     would now report divergence"
                );
            }
        }
    }

    /// And the columns that carry business meaning must stay *in* it, or two
    /// terminals can disagree about a real value and still score 100%.
    #[tokio::test]
    async fn business_columns_are_still_fingerprinted() {
        for (table, column) in [
            ("products", "name"),
            ("products", "is_active"),
            ("customers", "deleted_at"),
            ("shifts", "deleted_at"),
            ("sales", "net_total_minor"),
            ("product_prices", "price_minor"),
        ] {
            assert!(
                !skip_in_fingerprint(table, column),
                "{table}.{column} left the fingerprint"
            );
        }
    }

    /// The fingerprint is computed from real rows, so prove it on one: the JSON
    /// that gets hashed must contain the business columns and none of the
    /// bookkeeping.
    #[tokio::test]
    async fn a_real_row_fingerprint_covers_business_columns_only() {
        let pool = migrated_pool().await;
        sqlx::query(
            "INSERT INTO categories (category_id, name, created_at, updated_at)
             VALUES ('cat_1','Grocery','2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let row = sqlx::query("SELECT * FROM categories WHERE category_id='cat_1'")
            .fetch_one(&pool)
            .await
            .unwrap();

        // Rebuild the same map row_fingerprint hashes, so the assertion is about
        // the covered columns rather than an opaque digest.
        let mut covered = Vec::new();
        for col in row.columns() {
            if !skip_in_fingerprint("categories", col.name()) {
                covered.push(col.name().to_string());
            }
        }
        assert!(covered.contains(&"name".to_string()));
        assert!(covered.contains(&"updated_at".to_string()));
        assert!(!covered.contains(&"sync_status".to_string()));
        assert!(!covered.contains(&"version".to_string()));

        // The fingerprint is the canonical JSON that the table checksum hashes,
        // so it can be asserted on directly rather than through a digest.
        let printed = row_fingerprint("categories", &row);
        assert!(printed.contains("\"name\":\"Grocery\""), "{printed}");
        assert!(
            !printed.contains("\"version\""),
            "version entered the fingerprint: {printed}"
        );
        assert!(!printed.contains("\"sync_status\""), "{printed}");

        // Stable across calls, or two terminals would disagree with themselves.
        assert_eq!(printed, row_fingerprint("categories", &row));
    }
}
