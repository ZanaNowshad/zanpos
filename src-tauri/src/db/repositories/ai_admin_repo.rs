use crate::domain::ai_admin::AiSession;
use crate::errors::AppResult;
use sqlx::{Row, SqlitePool};

/// Actions and undo records now live in `ai_admin_actions_repo` — the split was
/// for file size, not for a change of interface, so their call sites keep
/// reaching them through this module.
pub use super::ai_admin_actions_repo::{
    claim_for_execution, create_action, create_undo_record, expire_old_actions, get_action,
    get_undo_availability, get_undo_record, list_actions, mark_cancelled, mark_executed,
    mark_undone, release_claim,
};

#[derive(Debug, Clone, serde::Serialize)]
pub struct AiToolMetricRow {
    pub tool_name: String,
    pub invocation_count: i64,
    pub success_count: i64,
    pub failure_count: i64,
    pub average_latency_ms: i64,
    pub last_latency_ms: i64,
    pub estimated_tokens: i64,
    pub last_error_at: Option<String>,
    pub updated_at: String,
}

pub async fn record_tool_metric(
    pool: &SqlitePool,
    tool_name: &str,
    success: bool,
    latency_ms: i64,
    estimated_tokens: i64,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO ai_tool_metrics
           (tool_name, invocation_count, success_count, failure_count,
            total_latency_ms, last_latency_ms, estimated_tokens, last_error_at, updated_at)
         VALUES (?, 1, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(tool_name) DO UPDATE SET
           invocation_count = invocation_count + 1,
           success_count = success_count + excluded.success_count,
           failure_count = failure_count + excluded.failure_count,
           total_latency_ms = total_latency_ms + excluded.total_latency_ms,
           last_latency_ms = excluded.last_latency_ms,
           estimated_tokens = estimated_tokens + excluded.estimated_tokens,
           last_error_at = COALESCE(excluded.last_error_at, last_error_at),
           updated_at = excluded.updated_at",
    )
    .bind(tool_name)
    .bind(i64::from(success))
    .bind(i64::from(!success))
    .bind(latency_ms.max(0))
    .bind(latency_ms.max(0))
    .bind(estimated_tokens.max(0))
    .bind((!success).then_some(now.as_str()))
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_tool_metrics(pool: &SqlitePool, limit: i64) -> AppResult<Vec<AiToolMetricRow>> {
    let rows = sqlx::query(
        "SELECT tool_name, invocation_count, success_count, failure_count,
                CASE WHEN invocation_count = 0 THEN 0
                     ELSE total_latency_ms / invocation_count END AS average_latency_ms,
                last_latency_ms, estimated_tokens, last_error_at, updated_at
         FROM ai_tool_metrics
         ORDER BY invocation_count DESC, tool_name ASC
         LIMIT ?",
    )
    .bind(limit.clamp(1, 500))
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| AiToolMetricRow {
            tool_name: row.get("tool_name"),
            invocation_count: row.get("invocation_count"),
            success_count: row.get("success_count"),
            failure_count: row.get("failure_count"),
            average_latency_ms: row.get("average_latency_ms"),
            last_latency_ms: row.get("last_latency_ms"),
            estimated_tokens: row.get("estimated_tokens"),
            last_error_at: row.get("last_error_at"),
            updated_at: row.get("updated_at"),
        })
        .collect())
}

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

// Retained: no caller: sessions are resolved through auth_session.
#[allow(dead_code)]
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
mod tests;
