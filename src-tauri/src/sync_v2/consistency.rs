use crate::errors::AppResult;
use crate::sync_v2::apply::{
    pk_for_table, should_skip_column, value_from_row_column, ALLOWED_CONFIG_KEYS,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Column, Row, SqlitePool};

pub const CONSISTENCY_TABLES: &[&str] = &[
    "branches",
    "roles",
    "users",
    "devices",
    "categories",
    "tax_rules",
    "products",
    "product_barcodes",
    "product_prices",
    "stock_levels",
    "stock_movements",
    "customers",
    "suppliers",
    "purchase_orders",
    "purchase_order_lines",
    "po_receipts",
    "riders",
    "app_config",
];

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
    for table in CONSISTENCY_TABLES {
        tables.push(table_snapshot(pool, table).await?);
    }
    Ok(ConsistencySnapshot {
        generated_at: chrono::Utc::now().to_rfc3339(),
        schema_version: schema_version(pool).await,
        tables,
    })
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
        let mut map = serde_json::Map::new();
        for col in row.columns() {
            let name = col.name();
            if should_skip_column(table, name) {
                continue;
            }
            map.insert(name.to_string(), value_from_row_column(row, name));
        }
        let line = serde_json::to_string(&map).unwrap_or_default();
        hasher.update(line.as_bytes());
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
}
