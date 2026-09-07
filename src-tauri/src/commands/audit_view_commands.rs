use crate::commands::rbac;
use crate::db::repositories::audit_hash;
use crate::errors::{AppError, AppResult};
use crate::sync::scope::report_scope;
use crate::AppState;
use serde::Serialize;
use sqlx::Row;
use tauri::State;

/// Reading back what the till recorded: the tax report a shop files from, the
/// audit log viewer, and the hash-chain verification behind it.
///
/// Split out of `phase10a_commands` when that file passed the 500-line limit.
/// The limit is a proxy for the fat-LTO discipline in CLAUDE.md — deep inline
/// chains through oversized code are what produced a release-only stack
/// overflow once — so the cut is by subject rather than by line count.

// ─── Tax report ───────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct TaxDayRow {
    pub day: String,
    pub transaction_count: i64,
    pub tax_minor: i64,
    pub cumulative_minor: i64,
}

/// Return daily tax totals for a date range.
#[tauri::command]
pub async fn report_tax_by_day(
    branch_id: String,
    from_date: String,
    to_date: String,
    session_token: String,
    state: State<'_, AppState>,
) -> Result<Vec<TaxDayRow>, AppError> {
    let actor = rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::MANAGER_OR_OWNER)
        .await?;
    // Caller-supplied `branch_id` is not trusted for branch-scoped data;
    // the scope comes from the actor's own record. The parameter remains
    // only to preserve the existing invoke contract.
    let _ = branch_id;
    let branch_id = actor.branch_id.clone();
    let (scope, origin_device_id) = report_scope(&state.db).await;
    let scope_str = scope.as_str();
    let rows = sqlx::query(
        "SELECT business_date AS day,
                COUNT(*) AS transaction_count,
                COALESCE(SUM(tax_total_minor), 0) AS tax_minor
         FROM sales
         WHERE branch_id = ? AND business_date BETWEEN ? AND ?
           AND status != 'voided'
           AND (? = 'all' OR origin_device_id = ?)
         GROUP BY business_date
         ORDER BY business_date ASC",
    )
    .bind(&branch_id)
    .bind(&from_date)
    .bind(&to_date)
    .bind(scope_str)
    .bind(&origin_device_id)
    .fetch_all(&state.db)
    .await?;

    let mut cumulative: i64 = 0;
    let result = rows
        .iter()
        .map(|r| {
            let tax: i64 = r.get("tax_minor");
            cumulative += tax;
            TaxDayRow {
                day: r.get("day"),
                transaction_count: r.get("transaction_count"),
                tax_minor: tax,
                cumulative_minor: cumulative,
            }
        })
        .collect();

    Ok(result)
}

// ─── Audit log viewer ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AuditLogRow {
    pub audit_log_id: String,
    pub event_type: String,
    pub entity_type: String,
    pub entity_id: Option<String>,
    pub actor_user_id: Option<String>,
    pub created_at: String,
}

/// List audit log entries with date-range filter and offset-based pagination.
/// Returns up to 50 rows per page (page is 0-based). Requires manager or owner.
#[tauri::command]
pub async fn audit_log_list(
    from: String,
    to: String,
    page: i64,
    session_token: String,
    state: State<'_, AppState>,
) -> Result<Vec<AuditLogRow>, AppError> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::MANAGER_OR_OWNER)
        .await?;
    let limit: i64 = 50;
    let offset = page * limit;

    // Append time bounds so date strings work as ISO8601 prefixes
    let from_ts = format!("{} 00:00:00", from);
    let to_ts = format!("{} 23:59:59", to);

    let rows = sqlx::query(
        "SELECT audit_log_id, event_type, entity_type, entity_id, actor_user_id, created_at
         FROM audit_logs
         WHERE created_at >= ? AND created_at <= ?
         ORDER BY created_at DESC
         LIMIT ? OFFSET ?",
    )
    .bind(&from_ts)
    .bind(&to_ts)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    Ok(rows
        .iter()
        .map(|r| AuditLogRow {
            audit_log_id: r.get("audit_log_id"),
            event_type: r.get("event_type"),
            entity_type: r.get("entity_type"),
            entity_id: r.get("entity_id"),
            actor_user_id: r.get("actor_user_id"),
            created_at: r.get("created_at"),
        })
        .collect())
}

// ─── Audit hash-chain verification ───────────────────────────────────────────

/// Verify the SHA-256 hash chain for audit_logs written by this device.
/// Returns a summary: total rows, legacy rows (pre-chain), verified count,
/// and counts of broken-hash or broken-link anomalies.
/// Requires manager or owner role.
#[tauri::command]
pub async fn audit_verify_chain(
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<audit_hash::ChainVerifyResult> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::MANAGER_OR_OWNER)
        .await?;

    let device_id: Option<String> = sqlx::query_scalar(
        "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .flatten();

    let device_id = device_id.unwrap_or_default();
    audit_hash::verify_chain(&state.db, &device_id).await
}
