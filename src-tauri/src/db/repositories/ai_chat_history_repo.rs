use crate::domain::ai_admin::AiChatMessage;
use crate::errors::AppResult;
use sqlx::{Row, SqlitePool};

pub async fn save_message(
    pool: &SqlitePool,
    session_id: &str,
    branch_id: &str,
    user_id: &str,
    role: &str,
    content: &str,
    message_type: &str,
) -> AppResult<i64> {
    let id = sqlx::query(
        "INSERT INTO ai_chat_messages
             (session_id, branch_id, user_id, role, content, message_type)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(session_id)
    .bind(branch_id)
    .bind(user_id)
    .bind(role)
    .bind(content)
    .bind(message_type)
    .execute(pool)
    .await?
    .last_insert_rowid();
    Ok(id)
}

/// Load the last `limit` messages for a user, returned oldest-first for display.
pub async fn load_history(
    pool: &SqlitePool,
    branch_id: &str,
    user_id: &str,
    limit: i64,
) -> AppResult<Vec<AiChatMessage>> {
    // Inner subquery grabs the newest N; outer sorts oldest-first
    let rows = sqlx::query(
        "SELECT id, session_id, branch_id, user_id, role, content, message_type, created_at
         FROM (
             SELECT * FROM ai_chat_messages
             WHERE branch_id = ? AND user_id = ?
             ORDER BY created_at DESC
             LIMIT ?
         )
         ORDER BY created_at ASC",
    )
    .bind(branch_id)
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| AiChatMessage {
            id: r.get("id"),
            session_id: r.get("session_id"),
            branch_id: r.get("branch_id"),
            user_id: r.get("user_id"),
            role: r.get("role"),
            content: r.get("content"),
            message_type: r.get("message_type"),
            created_at: r.get("created_at"),
        })
        .collect())
}

pub async fn clear_history(
    pool: &SqlitePool,
    branch_id: &str,
    user_id: &str,
) -> AppResult<()> {
    sqlx::query(
        "DELETE FROM ai_chat_messages WHERE branch_id = ? AND user_id = ?",
    )
    .bind(branch_id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}
