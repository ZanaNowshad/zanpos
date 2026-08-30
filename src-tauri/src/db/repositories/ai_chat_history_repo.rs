use crate::errors::{AppError, AppResult};
use sqlx::SqlitePool;

pub async fn save_message(
    pool: &SqlitePool,
    session_id: &str,
    // The thread this belongs to. `ai_sessions` is one request's accounting
    // row, so it cannot group a chat — see `ai_conversation_repo`.
    conversation_id: &str,
    branch_id: &str,
    user_id: &str,
    role: &str,
    content: &str,
    message_type: &str,
) -> AppResult<String> {
    for (label, value) in [
        ("session_id", session_id),
        ("branch_id", branch_id),
        ("user_id", user_id),
    ] {
        if value.is_empty() || value.chars().count() > 128 {
            return Err(AppError::Validation(format!(
                "{label} must contain 1..=128 characters"
            )));
        }
    }
    if !matches!(role, "user" | "assistant" | "system_event") {
        return Err(AppError::Validation("Invalid AI chat role".into()));
    }
    if !matches!(message_type, "text" | "action_card" | "error") {
        return Err(AppError::Validation("Invalid AI chat message type".into()));
    }
    if content.trim().is_empty() {
        return Err(AppError::Validation(
            "AI chat content cannot be empty".into(),
        ));
    }
    let owned_session: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ai_sessions WHERE session_id=? AND branch_id=? AND user_id=?",
    )
    .bind(session_id)
    .bind(branch_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    if owned_session != 1 {
        return Err(AppError::Permission(
            "AI chat session does not belong to this user and branch".into(),
        ));
    }
    let stored_content = if content.chars().count() > 50_000 {
        format!(
            "{}\n\n[Response truncated for safe history storage]",
            content.chars().take(50_000).collect::<String>()
        )
    } else {
        content.to_string()
    };
    let message_id = ulid::Ulid::new().to_string();
    sqlx::query(
        "INSERT INTO ai_chat_messages
             (message_id, session_id, conversation_id, branch_id, user_id, role, content, message_type)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&message_id)
    .bind(session_id)
    .bind(conversation_id)
    .bind(branch_id)
    .bind(user_id)
    .bind(role)
    .bind(&stored_content)
    .bind(message_type)
    .execute(pool)
    .await?;
    Ok(message_id)
}

pub async fn clear_history(pool: &SqlitePool, branch_id: &str, user_id: &str) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM ai_feedback WHERE user_id=? AND message_id IN (SELECT message_id FROM ai_chat_messages WHERE branch_id=? AND user_id=?)")
        .bind(user_id).bind(branch_id).bind(user_id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM ai_chat_messages WHERE branch_id = ? AND user_id = ?")
        .bind(branch_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn submit_feedback(
    pool: &SqlitePool,
    session_id: &str,
    message_id: &str,
    user_id: &str,
    branch_id: &str,
    rating: &str,
    comment: Option<&str>,
) -> AppResult<()> {
    let rows = sqlx::query(
        "INSERT INTO ai_feedback (feedback_id, session_id, user_id, message_id, rating, comment, created_at)
         SELECT ?, ?, ?, ?, ?, ?, ? FROM ai_chat_messages m
         JOIN ai_sessions s ON s.session_id=m.session_id
         WHERE m.message_id=? AND m.session_id=? AND m.user_id=? AND m.branch_id=? AND m.role='assistant'
           AND s.user_id=m.user_id AND s.branch_id=m.branch_id
         ON CONFLICT(user_id, message_id) DO UPDATE SET rating=excluded.rating, comment=excluded.comment, created_at=excluded.created_at",
    )
    .bind(ulid::Ulid::new().to_string()).bind(session_id).bind(user_id).bind(message_id)
    .bind(rating).bind(comment).bind(chrono::Utc::now().to_rfc3339())
    .bind(message_id).bind(session_id).bind(user_id).bind(branch_id)
    .execute(pool).await?.rows_affected();
    if rows == 0 {
        return Err(AppError::Permission(
            "Feedback target is not an owned assistant message".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    /// Reading a thread replaced the flat per-user history. Same guarantees —
    /// newest N, oldest first, scoped to its owner — so the assertions below
    /// carry over unchanged.
    async fn conv(
        pool: &SqlitePool,
        branch: &str,
        user: &str,
        limit: i64,
    ) -> Vec<crate::domain::ai_admin::AiChatMessage> {
        crate::db::repositories::ai_conversation_repo::messages(pool, "C1", branch, user, limit)
            .await
            .unwrap()
    }

    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn saved_messages_have_stable_public_ids_and_owner_scope() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO ai_sessions(session_id,branch_id,user_id,provider,model,status,started_at) VALUES('S1','B1','U1','test','test','ended',datetime('now'))").execute(&pool).await.unwrap();
        assert!(save_message(
            &pool,
            "missing",
            "C1",
            "B1",
            "U1",
            "assistant",
            "hello",
            "text"
        )
        .await
        .is_err());
        assert!(
            save_message(&pool, "S1", "C1", "B2", "U1", "assistant", "hello", "text")
                .await
                .is_err()
        );
        assert!(
            save_message(&pool, "S1", "C1", "B1", "U2", "assistant", "hello", "text")
                .await
                .is_err()
        );
        assert!(
            save_message(&pool, "S1", "C1", "B1", "U1", "tool", "hello", "text")
                .await
                .is_err()
        );
        assert!(
            save_message(&pool, "S1", "C1", "B1", "U1", "assistant", "hello", "html")
                .await
                .is_err()
        );
        assert!(
            save_message(&pool, "S1", "C1", "B1", "U1", "assistant", "   ", "text")
                .await
                .is_err()
        );
        let message_id = save_message(&pool, "S1", "C1", "B1", "U1", "assistant", "hello", "text")
            .await
            .unwrap();
        assert_eq!(message_id.len(), 26);
        let own = conv(&pool, "B1", "U1", 30).await;
        assert_eq!(own[0].message_id, message_id);
        // Isolation is enforced in the query, not by the caller.
        assert!(conv(&pool, "B2", "U1", 30).await.is_empty());
        assert!(conv(&pool, "B1", "U2", 30).await.is_empty());
        sqlx::query(
            "UPDATE ai_chat_messages SET created_at='2000-01-01 00:00:00' WHERE message_id=?",
        )
        .bind(&message_id)
        .execute(&pool)
        .await
        .unwrap();
        for index in 0..31 {
            let id = save_message(
                &pool,
                "S1",
                "C1",
                "B1",
                "U1",
                "user",
                &format!("ordered-{index}"),
                "text",
            )
            .await
            .unwrap();
            sqlx::query(
                "UPDATE ai_chat_messages SET created_at='2026-01-01 00:00:00' WHERE message_id=?",
            )
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        }
        let limited = conv(&pool, "B1", "U1", 30).await;
        assert_eq!(limited.len(), 30);
        assert_eq!(limited.first().unwrap().content, "ordered-1");
        assert_eq!(limited.last().unwrap().content, "ordered-30");

        let oversized = "x".repeat(50_001);
        let oversized_id = save_message(
            &pool,
            "S1",
            "C1",
            "B1",
            "U1",
            "assistant",
            &oversized,
            "text",
        )
        .await
        .unwrap();
        let stored: String =
            sqlx::query_scalar("SELECT content FROM ai_chat_messages WHERE message_id=?")
                .bind(oversized_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(stored.starts_with(&"x".repeat(50_000)));
        assert!(stored.ends_with("[Response truncated for safe history storage]"));
        assert!(stored.chars().count() < 50_100);
    }

    #[tokio::test]
    async fn feedback_requires_real_owned_assistant_session_and_clear_cascades() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO ai_sessions(session_id,branch_id,user_id,provider,model,status,started_at) VALUES('S1','B1','U1','test','test','ended',datetime('now'))").execute(&pool).await.unwrap();
        let assistant = save_message(&pool, "S1", "C1", "B1", "U1", "assistant", "answer", "text")
            .await
            .unwrap();
        let user = save_message(&pool, "S1", "C1", "B1", "U1", "user", "question", "text")
            .await
            .unwrap();
        assert!(
            submit_feedback(&pool, "FAKE", &assistant, "U1", "B1", "up", None)
                .await
                .is_err()
        );
        assert!(
            submit_feedback(&pool, "S1", &assistant, "U2", "B1", "up", None)
                .await
                .is_err()
        );
        assert!(submit_feedback(&pool, "S1", &user, "U1", "B1", "up", None)
            .await
            .is_err());
        submit_feedback(&pool, "S1", &assistant, "U1", "B1", "up", None)
            .await
            .unwrap();
        submit_feedback(&pool, "S1", &assistant, "U1", "B1", "down", Some("bad"))
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ai_feedback WHERE message_id=?")
            .bind(&assistant)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
        clear_history(&pool, "B1", "U1").await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ai_feedback WHERE message_id=?")
            .bind(&assistant)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn chat_migration_deduplicates_legacy_feedback() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE ai_feedback(feedback_id TEXT PRIMARY KEY,session_id TEXT NOT NULL,user_id TEXT NOT NULL,message_id TEXT NOT NULL,rating TEXT NOT NULL,comment TEXT,created_at TEXT NOT NULL)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE ai_chat_messages(id INTEGER PRIMARY KEY AUTOINCREMENT,message_id TEXT NOT NULL UNIQUE,session_id TEXT NOT NULL,branch_id TEXT NOT NULL,user_id TEXT NOT NULL,role TEXT NOT NULL,content TEXT NOT NULL,message_type TEXT NOT NULL,created_at TEXT NOT NULL)").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO ai_chat_messages(message_id,session_id,branch_id,user_id,role,content,message_type,created_at) VALUES('M1','S1','B1','U1','assistant','answer','text','2026-01-01')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO ai_feedback VALUES('F1','S1','U1','M1','up',NULL,'2026-01-01'),('F2','S1','U1','M1','down','latest','2026-01-02'),('F3','S1','U1','M2','up','orphan','2026-01-03')").execute(&pool).await.unwrap();
        sqlx::raw_sql(include_str!(
            "../../../migrations/0032_ai_chat_messages.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        let row: (i64, String) = sqlx::query_as(
            "SELECT COUNT(*), MAX(rating) FROM ai_feedback WHERE user_id='U1' AND message_id='M1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, 1);
        assert_eq!(row.1, "down");
        let orphan_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ai_feedback WHERE message_id='M2'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(orphan_count, 0);
        let exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='ai_chat_messages'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(exists, 1);
    }
}
