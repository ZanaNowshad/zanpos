//! Reprint queue for receipts the printer refused.
//!
//! The rule this exists to protect: a printer failure must never fail a sale.
//! The sale is already committed by the time printing runs, so a failure is
//! recorded and surfaced rather than propagated — at the till immediately, and
//! again at EOD so an unprinted receipt cannot quietly cross a shift boundary.
//!
//! Queued entries hold the rendered lines, not a sale id, so a reprint
//! reproduces what should have printed at the time even if prices, receipt
//! headers or tax settings have since changed.

use crate::commands::rbac;
use crate::errors::{AppError, AppResult};
use crate::AppState;
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use tauri::State;

#[derive(Debug, Serialize)]
pub struct ReprintEntry {
    pub id: String,
    pub receipt_number: Option<String>,
    pub store_name: String,
    pub lines: Vec<String>,
    pub failed_at: String,
    pub error: Option<String>,
    pub business_date: String,
}

/// Records a failed print. Never returns an error to the caller's sale path —
/// the caller ignores the result deliberately; a queue write that fails must
/// not turn a completed sale into a failed one.
pub async fn enqueue(
    pool: &SqlitePool,
    store_name: &str,
    lines: &[String],
    error: &str,
) -> AppResult<()> {
    // The receipt number is embedded in the rendered lines rather than passed
    // separately; pulling it out here keeps the EOD list readable without
    // changing the print call signature everywhere.
    let receipt_number = lines.iter().find_map(|line| {
        line.strip_prefix("Receipt:")
            .map(str::trim)
            .and_then(|value| value.strip_prefix('#').or(Some(value)))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    });
    let serialized_lines = serde_json::to_string(lines).unwrap_or_else(|_| "[]".into());

    sqlx::query(
        "INSERT INTO reprint_queue (id, receipt_number, store_name, lines, failed_at, error, business_date)
         SELECT ?, ?, ?, ?, ?, ?, ?
         WHERE NOT EXISTS (
             SELECT 1 FROM reprint_queue
             WHERE printed_at IS NULL AND store_name = ? AND lines = ?
         )",
    )
    .bind(ulid::Ulid::new().to_string())
    .bind(receipt_number)
    .bind(store_name)
    .bind(&serialized_lines)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(error)
    .bind(chrono::Local::now().format("%Y-%m-%d").to_string())
    .bind(store_name)
    .bind(&serialized_lines)
    .execute(pool)
    .await?;
    Ok(())
}

async fn pending(pool: &SqlitePool) -> AppResult<Vec<ReprintEntry>> {
    let rows = sqlx::query(
        "SELECT id, receipt_number, store_name, lines, failed_at, error, business_date
         FROM reprint_queue WHERE printed_at IS NULL ORDER BY failed_at",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| ReprintEntry {
            id: row.get("id"),
            receipt_number: row.get("receipt_number"),
            store_name: row.get("store_name"),
            lines: serde_json::from_str(&row.get::<String, _>("lines")).unwrap_or_default(),
            failed_at: row.get("failed_at"),
            error: row.get("error"),
            business_date: row.get("business_date"),
        })
        .collect())
}

/// Everything still unprinted. Drives both the till banner and the EOD list.
#[tauri::command]
pub async fn reprint_queue_pending(
    session_token: String,
    state: State<'_, AppState>,
) -> Result<Vec<ReprintEntry>, AppError> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE)
        .await?;
    pending(&state.db).await
}

/// Marks one entry printed. Called after a successful retry, so a receipt that
/// finally prints stops nagging.
#[tauri::command]
pub async fn reprint_queue_mark_printed(
    session_token: String,
    id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE)
        .await?;
    sqlx::query("UPDATE reprint_queue SET printed_at = ? WHERE id = ? AND printed_at IS NULL")
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(&id)
        .execute(&state.db)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE reprint_queue (
                id TEXT PRIMARY KEY NOT NULL, sale_id TEXT, receipt_number TEXT,
                store_name TEXT NOT NULL, lines TEXT NOT NULL, failed_at TEXT NOT NULL,
                error TEXT, business_date TEXT NOT NULL, printed_at TEXT)",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn a_failed_print_becomes_a_pending_entry_with_its_lines_intact() {
        let pool = pool().await;
        let lines = vec!["ZANPOS".to_string(), "Receipt: #A-01-000042".to_string()];
        enqueue(&pool, "Amwaj", &lines, "port busy").await.unwrap();

        let queued = pending(&pool).await.unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(
            queued[0].lines, lines,
            "the rendered receipt must survive verbatim"
        );
        assert_eq!(queued[0].receipt_number.as_deref(), Some("A-01-000042"));
        assert_eq!(queued[0].error.as_deref(), Some("port busy"));
    }

    #[tokio::test]
    async fn retry_failure_does_not_duplicate_an_existing_pending_receipt() {
        let pool = pool().await;
        let lines = vec!["Receipt: #A-01-000042".to_string()];
        enqueue(&pool, "Amwaj", &lines, "offline").await.unwrap();
        enqueue(&pool, "Amwaj", &lines, "still offline")
            .await
            .unwrap();

        assert_eq!(pending(&pool).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn marking_printed_clears_it_from_pending_without_deleting_history() {
        let pool = pool().await;
        enqueue(&pool, "Amwaj", &["x".into()], "offline")
            .await
            .unwrap();
        let id = pending(&pool).await.unwrap()[0].id.clone();

        sqlx::query("UPDATE reprint_queue SET printed_at = ? WHERE id = ?")
            .bind("2026-07-26T00:00:00Z")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();

        assert!(pending(&pool).await.unwrap().is_empty());
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM reprint_queue")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(total, 1, "history is kept so EOD can still account for it");
    }

    #[tokio::test]
    async fn entries_come_back_oldest_first() {
        let pool = pool().await;
        enqueue(&pool, "Amwaj", &["first".into()], "e")
            .await
            .unwrap();
        enqueue(&pool, "Amwaj", &["second".into()], "e")
            .await
            .unwrap();
        let queued = pending(&pool).await.unwrap();
        assert_eq!(queued[0].lines[0], "first");
    }
}
