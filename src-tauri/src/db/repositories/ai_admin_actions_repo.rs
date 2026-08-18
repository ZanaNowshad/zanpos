//! Proposed AI actions and their undo records.
//!
//! Split from `ai_admin_repo` for size. The seam follows the lifecycle: an
//! action is proposed, executed or cancelled, and — because every mutation the
//! assistant makes must be reversible — carries an undo record that outlives
//! it. Sessions, usage and config stay next door; they describe the
//! conversation, not what it changed.

use crate::domain::ai_admin::{AiAction, AiActionSummary, UndoAvailability, UndoRecord};
use crate::errors::{AppError, AppResult};
use sqlx::{Row, SqlitePool};
use ulid::Ulid;

// ── AI actions ─────────────────────────────────────────────────────────────────

pub async fn create_action(
    pool: &SqlitePool,
    session_user_id: &str,
    branch_id: &str,
    tool_name: &str,
    tool_input_json: &str,
    tool_input_hash: &str,
    preview_text: &str,
    confirmation_token: &str,
    expiry_minutes: i64,
) -> AppResult<AiAction> {
    let action_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let expires_at = (chrono::Utc::now() + chrono::Duration::minutes(expiry_minutes)).to_rfc3339();

    sqlx::query(
        "INSERT INTO ai_actions
         (action_id, session_user_id, branch_id, tool_name, tool_input_json, tool_input_hash,
          preview_text, status, confirmation_token, prepared_at, expires_at, actor_type)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'prepared', ?, ?, ?, 'ai')",
    )
    .bind(&action_id)
    .bind(session_user_id)
    .bind(branch_id)
    .bind(tool_name)
    .bind(tool_input_json)
    .bind(tool_input_hash)
    .bind(preview_text)
    .bind(confirmation_token)
    .bind(&now)
    .bind(&expires_at)
    .execute(pool)
    .await?;

    get_action(pool, &action_id)
        .await?
        .ok_or_else(|| AppError::Internal("Action not found after insert".into()))
}

pub async fn get_action(pool: &SqlitePool, action_id: &str) -> AppResult<Option<AiAction>> {
    let row = sqlx::query(
        "SELECT action_id, session_user_id, branch_id, tool_name, tool_input_json, tool_input_hash,
                preview_text, status, confirmation_token, prepared_at, confirmed_at,
                executed_at, expires_at, result_json, error_message
         FROM ai_actions WHERE action_id = ?",
    )
    .bind(action_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| AiAction {
        action_id: r.get("action_id"),
        session_user_id: r.get("session_user_id"),
        branch_id: r.get("branch_id"),
        tool_name: r.get("tool_name"),
        tool_input_json: r.get("tool_input_json"),
        tool_input_hash: r.get("tool_input_hash"),
        preview_text: r.get("preview_text"),
        status: r.get("status"),
        confirmation_token: r.get("confirmation_token"),
        prepared_at: r.get("prepared_at"),
        confirmed_at: r.get("confirmed_at"),
        executed_at: r.get("executed_at"),
        expires_at: r.get("expires_at"),
        result_json: r.get("result_json"),
        error_message: r.get("error_message"),
    }))
}

/// Maximum rows a single `list_actions` call may return.
/// Prevents an unbounded scan of `ai_actions` from the UI.
pub const ACTION_LIST_MAX_LIMIT: i64 = 200;
const ACTION_LIST_DEFAULT_LIMIT: i64 = 50;

/// List persisted AI actions for one branch.
///
/// Branch scope is a required argument rather than an option: every caller must
/// state whose actions it is reading, and the command layer supplies the
/// authenticated actor's branch so a client cannot widen it.
///
/// `statuses` is an allow-list filter. An empty slice means "any status".
/// Unknown status strings simply match nothing — they are not an error, which
/// keeps the API forward-compatible if a new state is ever added.
///
/// Ordering is deterministic: newest preparation first, `action_id` breaking
/// ties (ULIDs are monotonic, so this is stable across equal timestamps).
pub async fn list_actions(
    pool: &SqlitePool,
    branch_id: &str,
    statuses: &[String],
    limit: i64,
    offset: i64,
) -> AppResult<Vec<AiActionSummary>> {
    let limit = if limit <= 0 {
        ACTION_LIST_DEFAULT_LIMIT
    } else {
        limit.min(ACTION_LIST_MAX_LIMIT)
    };
    let offset = offset.max(0);

    // Statuses are bound as parameters, never interpolated, so the filter
    // cannot be used for injection.
    let mut sql = String::from(
        "SELECT action_id, session_user_id, branch_id, tool_name, preview_text, status,
                prepared_at, confirmed_at, executed_at, expires_at, result_json, error_message
         FROM ai_actions WHERE branch_id = ?",
    );
    if !statuses.is_empty() {
        sql.push_str(" AND status IN (");
        sql.push_str(&vec!["?"; statuses.len()].join(","));
        sql.push(')');
    }
    sql.push_str(" ORDER BY prepared_at DESC, action_id DESC LIMIT ? OFFSET ?");

    let mut query = sqlx::query(&sql).bind(branch_id);
    for status in statuses {
        query = query.bind(status);
    }
    let rows = query.bind(limit).bind(offset).fetch_all(pool).await?;

    Ok(rows
        .into_iter()
        .map(|r| AiActionSummary {
            action_id: r.get("action_id"),
            session_user_id: r.get("session_user_id"),
            branch_id: r.get("branch_id"),
            tool_name: r.get("tool_name"),
            preview_text: r.get("preview_text"),
            status: r.get("status"),
            prepared_at: r.get("prepared_at"),
            confirmed_at: r.get("confirmed_at"),
            executed_at: r.get("executed_at"),
            expires_at: r.get("expires_at"),
            result_json: r.get("result_json"),
            error_message: r.get("error_message"),
        })
        .collect())
}

pub async fn mark_executed(pool: &SqlitePool, action_id: &str, result_json: &str) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let rows = sqlx::query(
        "UPDATE ai_actions SET status = 'executed', executed_at = ?, result_json = ?
         WHERE action_id = ? AND status = 'prepared'",
    )
    .bind(&now)
    .bind(result_json)
    .bind(action_id)
    .execute(pool)
    .await?;

    if rows.rows_affected() == 0 {
        return Err(AppError::Conflict(
            "Action is not in prepared state or already executed".into(),
        ));
    }
    Ok(())
}

pub async fn mark_cancelled(pool: &SqlitePool, action_id: &str) -> AppResult<()> {
    let rows = sqlx::query(
        "UPDATE ai_actions SET status = 'cancelled' WHERE action_id = ? AND status = 'prepared'",
    )
    .bind(action_id)
    .execute(pool)
    .await?;
    if rows.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "Action {action_id} not found or not in prepared state"
        )));
    }
    Ok(())
}

pub async fn expire_old_actions(pool: &SqlitePool) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE ai_actions SET status = 'expired' WHERE status = 'prepared' AND expires_at < ?",
    )
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

// ── Undo records ───────────────────────────────────────────────────────────────

pub async fn create_undo_record(
    pool: &SqlitePool,
    action_id: &str,
    entity_type: &str,
    entity_id: &str,
    snapshot_json: &str,
    rollback_tool: &str,
    rollback_input_json: &str,
) -> AppResult<UndoRecord> {
    let undo_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO undo_records
         (undo_id, action_id, before_json, entity_type, entity_id, snapshot_json,
          rollback_tool, rollback_input_json, status, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'available', ?)",
    )
    .bind(&undo_id)
    .bind(action_id)
    // `before_json` is the legacy 0001 column: TEXT NOT NULL with no default.
    // Migration 0008 added snapshot_json/rollback_* but left before_json in place,
    // so it MUST still be populated or the INSERT fails with a NOT NULL violation
    // (SQLite extended code 1299) on every AI mutation confirm. Mirror snapshot_json
    // into it; current reads use snapshot_json, not before_json.
    .bind(snapshot_json)
    .bind(entity_type)
    .bind(entity_id)
    .bind(snapshot_json)
    .bind(rollback_tool)
    .bind(rollback_input_json)
    .bind(&now)
    .execute(pool)
    .await?;

    get_undo_record(pool, &undo_id)
        .await?
        .ok_or_else(|| AppError::Internal("Undo record not found after insert".into()))
}

pub async fn get_undo_record(pool: &SqlitePool, undo_id: &str) -> AppResult<Option<UndoRecord>> {
    let row = sqlx::query(
        "SELECT undo_id, action_id, entity_type, entity_id, snapshot_json,
                rollback_tool, rollback_input_json, status, created_at, undone_at, undone_by_user_id
         FROM undo_records WHERE undo_id = ?",
    )
    .bind(undo_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| UndoRecord {
        undo_id: r.get("undo_id"),
        action_id: r.get("action_id"),
        entity_type: r.get("entity_type"),
        entity_id: r.get("entity_id"),
        snapshot_json: r.get("snapshot_json"),
        rollback_tool: r.get("rollback_tool"),
        rollback_input_json: r.get("rollback_input_json"),
        status: r.get("status"),
        created_at: r.get("created_at"),
        undone_at: r.get("undone_at"),
        undone_by_user_id: r.get("undone_by_user_id"),
    }))
}

/// Undo availability for one executed action, scoped by branch.
///
/// The join through `ai_actions` is what enforces scope: an undo record is only
/// visible to a caller whose branch owns the action that produced it, so an
/// undo id alone cannot be used to probe another branch.
pub async fn get_undo_availability(
    pool: &SqlitePool,
    branch_id: &str,
    action_id: &str,
) -> AppResult<Option<UndoAvailability>> {
    let row = sqlx::query(
        "SELECT u.undo_id, u.action_id, u.entity_type, u.entity_id, u.status,
                u.created_at, u.undone_at
         FROM undo_records u
         JOIN ai_actions a ON a.action_id = u.action_id
         WHERE u.action_id = ? AND a.branch_id = ?
         ORDER BY u.created_at DESC
         LIMIT 1",
    )
    .bind(action_id)
    .bind(branch_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| {
        let status: String = r.get("status");
        UndoAvailability {
            undo_id: r.get("undo_id"),
            action_id: r.get("action_id"),
            entity_type: r.get("entity_type"),
            entity_id: r.get("entity_id"),
            available: status == "available",
            status,
            created_at: r.get("created_at"),
            undone_at: r.get("undone_at"),
        }
    }))
}

pub async fn mark_undone(pool: &SqlitePool, undo_id: &str, user_id: &str) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let rows = sqlx::query(
        "UPDATE undo_records SET status = 'undone', undone_at = ?, undone_by_user_id = ?
         WHERE undo_id = ? AND status = 'available'",
    )
    .bind(&now)
    .bind(user_id)
    .bind(undo_id)
    .execute(pool)
    .await?;

    if rows.rows_affected() == 0 {
        return Err(AppError::Conflict(
            "Undo record is not available or already used".into(),
        ));
    }
    Ok(())
}
