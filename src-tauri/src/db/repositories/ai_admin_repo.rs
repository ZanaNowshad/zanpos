#![allow(dead_code)]
use crate::domain::ai_admin::{AiAction, AiSession, UndoRecord};
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

// ── Sessions ──────────────────────────────────────────────────────────────────

pub async fn create_session(
    pool: &SqlitePool,
    session_id: &str,
    branch_id: &str,
    user_id: &str,
    provider: &str,
    model: &str,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO ai_sessions (session_id, branch_id, user_id, provider, model, started_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(session_id)
    .bind(branch_id)
    .bind(user_id)
    .bind(provider)
    .bind(model)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_session(pool: &SqlitePool, session_id: &str) -> AppResult<Option<AiSession>> {
    let row = sqlx::query(
        "SELECT session_id, branch_id, user_id, provider, model, status,
                total_turns, tokens_in, tokens_out, cost_estimate_usd,
                total_latency_ms, started_at, ended_at
         FROM ai_sessions WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| AiSession {
        session_id: r.get("session_id"),
        branch_id: r.get("branch_id"),
        user_id: r.get("user_id"),
        provider: r.get("provider"),
        model: r.get("model"),
        status: r.get("status"),
        total_turns: r.get("total_turns"),
        tokens_in: r.get("tokens_in"),
        tokens_out: r.get("tokens_out"),
        cost_estimate_usd: r.get("cost_estimate_usd"),
        total_latency_ms: r.get("total_latency_ms"),
        started_at: r.get("started_at"),
        ended_at: r.get("ended_at"),
    }))
}

pub async fn end_session(pool: &SqlitePool, session_id: &str, status: &str) -> AppResult<()> {
    if !matches!(status, "active" | "ended" | "error" | "cancelled") {
        return Err(crate::errors::AppError::Validation(
            "Invalid AI session status".into(),
        ));
    }
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE ai_sessions SET status = ?, ended_at = ? WHERE session_id = ?")
        .bind(status)
        .bind(&now)
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn record_usage(
    pool: &SqlitePool,
    session_id: &str,
    turn: i32,
    tokens_in: i32,
    tokens_out: i32,
    latency_ms: i32,
    provider: &str,
    model: &str,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO ai_usage_log (session_id, turn, tokens_in, tokens_out, latency_ms, provider, model, logged_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(session_id)
    .bind(turn)
    .bind(tokens_in)
    .bind(tokens_out)
    .bind(latency_ms)
    .bind(provider)
    .bind(model)
    .bind(&now)
    .execute(pool)
    .await?;

    // Update session aggregates
    sqlx::query(
        "UPDATE ai_sessions
         SET total_turns = total_turns + 1,
             tokens_in = tokens_in + ?,
             tokens_out = tokens_out + ?,
             total_latency_ms = total_latency_ms + ?
         WHERE session_id = ?",
    )
    .bind(tokens_in)
    .bind(tokens_out)
    .bind(latency_ms)
    .bind(session_id)
    .execute(pool)
    .await?;

    Ok(())
}

// ── Chat history cleanup ──────────────────────────────────────────────────────

pub async fn cleanup_old_messages(pool: &SqlitePool, retention_days: i64) -> AppResult<u64> {
    if !(1..=3_650).contains(&retention_days) {
        return Err(crate::errors::AppError::Validation(
            "AI chat retention_days must be between 1 and 3650".into(),
        ));
    }
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(retention_days)).to_rfc3339();
    let per_user_cap: i64 = 200;

    // Enforce both a hard age limit and a hard per-owner row cap.
    let ranked_delete = "datetime(created_at) < datetime(?)
        OR id IN (
            SELECT id FROM (
                SELECT id, ROW_NUMBER() OVER (
                    PARTITION BY branch_id, user_id ORDER BY datetime(created_at) DESC, id DESC
                ) AS row_num FROM ai_chat_messages
            ) ranked WHERE row_num > ?
        )";
    let mut tx = pool.begin().await?;
    let feedback_sql = format!(
        "DELETE FROM ai_feedback WHERE message_id IN (SELECT message_id FROM ai_chat_messages WHERE {ranked_delete})"
    );
    sqlx::query(&feedback_sql)
        .bind(&cutoff)
        .bind(per_user_cap)
        .execute(&mut *tx)
        .await?;
    let message_sql = format!("DELETE FROM ai_chat_messages WHERE {ranked_delete}");
    let rows = sqlx::query(&message_sql)
        .bind(&cutoff)
        .bind(per_user_cap)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn prepared_action_persists_authenticated_branch() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        let action = create_action(
            &pool,
            "U1",
            "BRANCH1",
            "product_update",
            "{}",
            "hash",
            "preview",
            "confirmation",
            10,
        )
        .await
        .unwrap();

        assert_eq!(action.branch_id, "BRANCH1");
    }

    #[tokio::test]
    async fn cancelled_chat_session_has_one_terminal_status() {
        // Migration 0033 expands the persisted terminal-state constraint.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        create_session(&pool, "S-CANCEL", "B1", "U1", "test", "test")
            .await
            .unwrap();
        end_session(&pool, "S-CANCEL", "cancelled").await.unwrap();
        let session = get_session(&pool, "S-CANCEL").await.unwrap().unwrap();
        assert_eq!(session.status, "cancelled");
        assert!(session.ended_at.is_some());
        assert!(end_session(&pool, "S-CANCEL", "invented").await.is_err());
    }

    #[tokio::test]
    async fn chat_cleanup_enforces_age_and_per_owner_cap() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        for index in 0..201 {
            sqlx::query("INSERT INTO ai_chat_messages(message_id,session_id,branch_id,user_id,role,content,message_type,created_at) VALUES(?, 'S1','B1','U1','user','x','text',datetime('now'))")
                .bind(format!("M{index}"))
                .execute(&pool).await.unwrap();
        }
        sqlx::query("INSERT INTO ai_chat_messages(message_id,session_id,branch_id,user_id,role,content,message_type,created_at) VALUES('OLD','S2','B2','U2','user','x','text','2000-01-01T00:00:00Z')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO ai_chat_messages(message_id,session_id,branch_id,user_id,role,content,message_type,created_at) VALUES('BEFORE','S2','B2','U2','user','x','text',datetime('now','-31 days')),('AFTER','S2','B2','U2','user','x','text',datetime('now','-29 days'))")
            .execute(&pool).await.unwrap();
        cleanup_old_messages(&pool, 30).await.unwrap();
        let u1: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ai_chat_messages WHERE user_id='U1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let old: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ai_chat_messages WHERE message_id='OLD'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(u1, 200);
        assert_eq!(old, 0);
        let before: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ai_chat_messages WHERE message_id='BEFORE'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let after: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ai_chat_messages WHERE message_id='AFTER'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(before, 0);
        assert_eq!(after, 1);
        assert!(cleanup_old_messages(&pool, 0).await.is_err());
        assert!(cleanup_old_messages(&pool, 3_651).await.is_err());
    }
}
