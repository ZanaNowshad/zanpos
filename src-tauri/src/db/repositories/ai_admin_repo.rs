use crate::domain::ai_admin::{AiAction, UndoRecord};
use crate::errors::{AppError, AppResult};
use sqlx::{Row, SqlitePool};
use ulid::Ulid;

// ── App config ─────────────────────────────────────────────────────────────────

pub async fn get_config(pool: &SqlitePool, key: &str) -> AppResult<Option<String>> {
    let row = sqlx::query("SELECT value FROM app_config WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.get::<String, _>("value")))
}

pub async fn set_config(pool: &SqlitePool, key: &str, value: &str) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO app_config (key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(value)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

// ── AI actions ─────────────────────────────────────────────────────────────────

pub async fn create_action(
    pool: &SqlitePool,
    session_user_id: &str,
    tool_name: &str,
    tool_input_json: &str,
    tool_input_hash: &str,
    preview_text: &str,
    confirmation_token: &str,
) -> AppResult<AiAction> {
    let action_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    // Actions expire after 10 minutes
    let expires_at = (chrono::Utc::now() + chrono::Duration::minutes(10)).to_rfc3339();

    sqlx::query(
        "INSERT INTO ai_actions
         (action_id, session_user_id, tool_name, tool_input_json, tool_input_hash,
          preview_text, status, confirmation_token, prepared_at, expires_at, actor_type)
         VALUES (?, ?, ?, ?, ?, ?, 'prepared', ?, ?, ?, 'ai')",
    )
    .bind(&action_id)
    .bind(session_user_id)
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
        "SELECT action_id, session_user_id, tool_name, tool_input_json, tool_input_hash,
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
    sqlx::query(
        "UPDATE ai_actions SET status = 'cancelled' WHERE action_id = ? AND status = 'prepared'",
    )
    .bind(action_id)
    .execute(pool)
    .await?;
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
         (undo_id, action_id, entity_type, entity_id, snapshot_json,
          rollback_tool, rollback_input_json, status, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'available', ?)",
    )
    .bind(&undo_id)
    .bind(action_id)
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
