/// Phase 10a commands: session timeout config, DB backup, tax report, audit log viewer.
use tauri::State;
use tauri::Manager;
use sqlx::Row;
use serde::Serialize;
use crate::errors::{AppError, AppResult};
use crate::AppState;

// ─── Session timeout ──────────────────────────────────────────────────────────

/// Returns the configured idle timeout in minutes (defaults to 5).
#[tauri::command]
pub async fn app_config_get_timeout(state: State<'_, AppState>) -> Result<i64, AppError> {
    let val: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_config WHERE key = 'idle_timeout_minutes'"
    )
    .fetch_optional(&state.db)
    .await?
    .flatten();

    Ok(val.and_then(|v| v.parse::<i64>().ok()).unwrap_or(5))
}

/// Sets the idle timeout in minutes (1–60).
#[tauri::command]
pub async fn app_config_set_timeout(
    minutes: i64,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    if !(1..=60).contains(&minutes) {
        return Err(AppError::Validation("Timeout must be between 1 and 60 minutes".into()));
    }
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO app_config(key, value, updated_at) VALUES('idle_timeout_minutes', ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at"
    )
    .bind(minutes.to_string())
    .bind(&now)
    .execute(&state.db)
    .await?;
    Ok(())
}

// ─── DB Backup ────────────────────────────────────────────────────────────────

/// Copy the SQLite database file.
/// If dest_path is empty, saves to the user's Documents folder with a timestamp.
/// Returns the final destination path.
#[tauri::command]
pub async fn db_backup(
    dest_path: String,
    app: tauri::AppHandle,
) -> Result<String, AppError> {
    let app_data = app.path().app_data_dir()
        .map_err(|e| AppError::Internal(format!("Could not resolve app data dir: {e}")))?;
    let src = app_data.join("zanpos.db");

    let dest = if dest_path.trim().is_empty() {
        // Auto-generate path in Documents
        let docs = app.path().document_dir()
            .map_err(|e| AppError::Internal(format!("Could not resolve documents dir: {e}")))?;
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        docs.join(format!("zanpos-backup-{ts}.db"))
    } else {
        std::path::PathBuf::from(&dest_path)
    };

    std::fs::copy(&src, &dest)
        .map_err(|e| AppError::Internal(format!("Backup failed: {e}")))?;

    Ok(dest.to_string_lossy().to_string())
}

// ─── Tax report ───────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct TaxDayRow {
    pub day:              String,
    pub transaction_count: i64,
    pub tax_minor:        i64,
    pub cumulative_minor: i64,
}

/// Return daily tax totals for a date range.
#[tauri::command]
pub async fn report_tax_by_day(
    branch_id: String,
    from_date:  String,
    to_date:    String,
    state: State<'_, AppState>,
) -> Result<Vec<TaxDayRow>, AppError> {
    let rows = sqlx::query(
        "SELECT business_date AS day,
                COUNT(*) AS transaction_count,
                COALESCE(SUM(tax_total_minor), 0) AS tax_minor
         FROM sales
         WHERE branch_id = ? AND business_date BETWEEN ? AND ?
           AND status != 'voided'
         GROUP BY business_date
         ORDER BY business_date ASC"
    )
    .bind(&branch_id)
    .bind(&from_date)
    .bind(&to_date)
    .fetch_all(&state.db)
    .await?;

    let mut cumulative: i64 = 0;
    let result = rows.iter().map(|r| {
        let tax: i64 = r.get("tax_minor");
        cumulative += tax;
        TaxDayRow {
            day:               r.get("day"),
            transaction_count: r.get("transaction_count"),
            tax_minor:         tax,
            cumulative_minor:  cumulative,
        }
    }).collect();

    Ok(result)
}

// ─── Audit log viewer ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AuditLogRow {
    pub audit_log_id: String,
    pub event_type:   String,
    pub entity_type:  String,
    pub entity_id:    Option<String>,
    pub actor_user_id: Option<String>,
    pub created_at:   String,
}

/// List audit log entries with date-range filter and offset-based pagination.
/// Returns up to 50 rows per page (page is 0-based).
#[tauri::command]
pub async fn audit_log_list(
    from: String,
    to:   String,
    page: i64,
    state: State<'_, AppState>,
) -> Result<Vec<AuditLogRow>, AppError> {
    let limit: i64 = 50;
    let offset = page * limit;

    // Append time bounds so date strings work as ISO8601 prefixes
    let from_ts = format!("{} 00:00:00", from);
    let to_ts   = format!("{} 23:59:59", to);

    let rows = sqlx::query(
        "SELECT audit_log_id, event_type, entity_type, entity_id, actor_user_id, created_at
         FROM audit_logs
         WHERE created_at >= ? AND created_at <= ?
         ORDER BY created_at DESC
         LIMIT ? OFFSET ?"
    )
    .bind(&from_ts)
    .bind(&to_ts)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    Ok(rows.iter().map(|r| AuditLogRow {
        audit_log_id:  r.get("audit_log_id"),
        event_type:    r.get("event_type"),
        entity_type:   r.get("entity_type"),
        entity_id:     r.get("entity_id"),
        actor_user_id: r.get("actor_user_id"),
        created_at:    r.get("created_at"),
    }).collect())
}

// ─── AppResult alias re-export for convenience ────────────────────────────────
pub type _AppResult<T> = AppResult<T>;
