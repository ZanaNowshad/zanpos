//! Chat threads: listing them, opening them, ending them.
//!
//! Kept apart from `ai_admin_repo`, which owns `ai_sessions` — the per-request
//! usage row. The two words look alike and mean different things, and merging
//! them is how a thread ends up billed as a request or a request ends up in the
//! history list.

use crate::domain::ai_admin::{AiChatMessage, AiConversation};
use crate::errors::{AppError, AppResult};
use sqlx::{Row, SqlitePool};

/// A title has to fit a 260px sidebar without becoming a paragraph.
const MAX_TITLE: usize = 80;

/// How many threads the list offers. Beyond this nobody scrolls; they search or
/// start a new one.
const MAX_LISTED: i64 = 60;

fn validate_id(label: &str, value: &str) -> AppResult<()> {
    if value.is_empty() || value.chars().count() > 128 {
        return Err(AppError::Validation(format!(
            "{label} must contain 1..=128 characters"
        )));
    }
    Ok(())
}

/// Name a thread after the first thing the operator said.
///
/// Collapsed to one line and cut on a word boundary: chat input arrives with
/// newlines in it, and a title that wraps to four lines turns the history list
/// into a wall.
pub fn title_from(message: &str) -> String {
    let flattened = message.split_whitespace().collect::<Vec<_>>().join(" ");
    if flattened.chars().count() <= MAX_TITLE {
        return flattened;
    }
    let mut cut = String::new();
    for word in flattened.split(' ') {
        if cut.chars().count() + word.chars().count() + 1 > MAX_TITLE - 1 {
            break;
        }
        if !cut.is_empty() {
            cut.push(' ');
        }
        cut.push_str(word);
    }
    // A single word longer than the limit leaves `cut` empty; fall back to a
    // hard cut so the thread still gets a name.
    if cut.is_empty() {
        cut = flattened.chars().take(MAX_TITLE - 1).collect();
    }
    format!("{cut}…")
}

/// Make sure the thread exists and belongs to this operator, then return it.
///
/// Ownership is checked on every write rather than trusted from the caller: the
/// conversation id arrives from the client, and a thread is the whole record of
/// what somebody asked the AI to do with the shop.
pub async fn ensure(
    pool: &SqlitePool,
    conversation_id: &str,
    branch_id: &str,
    user_id: &str,
) -> AppResult<()> {
    validate_id("conversation_id", conversation_id)?;
    validate_id("branch_id", branch_id)?;
    validate_id("user_id", user_id)?;

    let owner: Option<(String, String)> =
        sqlx::query_as("SELECT branch_id, user_id FROM ai_conversations WHERE conversation_id = ?")
            .bind(conversation_id)
            .fetch_optional(pool)
            .await?;

    match owner {
        Some((existing_branch, existing_user)) => {
            if existing_branch != branch_id || existing_user != user_id {
                return Err(AppError::Permission(
                    "That conversation belongs to another user".into(),
                ));
            }
            Ok(())
        }
        None => {
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                "INSERT INTO ai_conversations
                   (conversation_id, branch_id, user_id, title, message_count,
                    last_message_at, created_at, updated_at)
                 VALUES (?, ?, ?, '', 0, NULL, ?, ?)",
            )
            .bind(conversation_id)
            .bind(branch_id)
            .bind(user_id)
            .bind(&now)
            .bind(&now)
            .execute(pool)
            .await?;
            Ok(())
        }
    }
}

/// Record that a message landed: bump the counters and, the first time, name it.
///
/// The title is set only while empty, so a thread keeps the name it was given
/// even as it wanders — and a rename by the operator is never overwritten by
/// the next message.
pub async fn note_message(
    pool: &SqlitePool,
    conversation_id: &str,
    first_user_message: Option<&str>,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let title = first_user_message.map(title_from).unwrap_or_default();
    sqlx::query(
        "UPDATE ai_conversations
            SET message_count   = message_count + 1,
                last_message_at = ?,
                updated_at      = ?,
                title           = CASE
                                    WHEN TRIM(title) = '' AND ? <> '' THEN ?
                                    ELSE title
                                  END
          WHERE conversation_id = ?",
    )
    .bind(&now)
    .bind(&now)
    .bind(&title)
    .bind(&title)
    .bind(conversation_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list(
    pool: &SqlitePool,
    branch_id: &str,
    user_id: &str,
) -> AppResult<Vec<AiConversation>> {
    let rows = sqlx::query(
        "SELECT conversation_id, branch_id, user_id, title, message_count,
                last_message_at, created_at, updated_at
           FROM ai_conversations
          WHERE branch_id = ? AND user_id = ? AND archived_at IS NULL
            -- An empty thread is one the operator opened and never used. It is
            -- not history and listing it would push real threads down.
            AND message_count > 0
          ORDER BY datetime(COALESCE(last_message_at, created_at)) DESC
          LIMIT ?",
    )
    .bind(branch_id)
    .bind(user_id)
    .bind(MAX_LISTED)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| AiConversation {
            conversation_id: r.get("conversation_id"),
            branch_id: r.get("branch_id"),
            user_id: r.get("user_id"),
            title: r.get("title"),
            message_count: r.get("message_count"),
            last_message_at: r.get("last_message_at"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        })
        .collect())
}

/// Every message in one thread, oldest first.
///
/// Scoped by owner in the same query rather than checked afterwards, so a
/// conversation id guessed from elsewhere returns nothing instead of somebody
/// else's chat.
pub async fn messages(
    pool: &SqlitePool,
    conversation_id: &str,
    branch_id: &str,
    user_id: &str,
    limit: i64,
) -> AppResult<Vec<AiChatMessage>> {
    let rows = sqlx::query(
        "SELECT id, message_id, session_id, branch_id, user_id, role, content,
                message_type, created_at
           FROM (
             SELECT * FROM ai_chat_messages
              WHERE conversation_id = ? AND branch_id = ? AND user_id = ?
              ORDER BY id DESC
              LIMIT ?
           )
          ORDER BY id ASC",
    )
    .bind(conversation_id)
    .bind(branch_id)
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| AiChatMessage {
            id: r.get("id"),
            message_id: r.get("message_id"),
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

/// The thread to reopen when ZanAI is opened: the one last spoken to.
pub async fn most_recent(
    pool: &SqlitePool,
    branch_id: &str,
    user_id: &str,
) -> AppResult<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT conversation_id FROM ai_conversations
          WHERE branch_id = ? AND user_id = ? AND archived_at IS NULL
            AND message_count > 0
          ORDER BY datetime(COALESCE(last_message_at, created_at)) DESC
          LIMIT 1",
    )
    .bind(branch_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?)
}

pub async fn rename(
    pool: &SqlitePool,
    conversation_id: &str,
    branch_id: &str,
    user_id: &str,
    title: &str,
) -> AppResult<()> {
    let title = title_from(title);
    if title.trim().is_empty() {
        return Err(AppError::Validation("A title cannot be empty".into()));
    }
    let changed = sqlx::query(
        "UPDATE ai_conversations SET title = ?, updated_at = ?
          WHERE conversation_id = ? AND branch_id = ? AND user_id = ?",
    )
    .bind(&title)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(conversation_id)
    .bind(branch_id)
    .bind(user_id)
    .execute(pool)
    .await?
    .rows_affected();
    if changed == 0 {
        return Err(AppError::NotFound("Conversation not found".into()));
    }
    Ok(())
}

/// Take a thread out of the list without destroying what was asked.
///
/// Archived, not deleted. A thread is the record of instructions given to
/// something that can change prices and stock, and tidying the sidebar is not a
/// reason for that record to stop existing. Clearing everything is a separate,
/// explicit act.
pub async fn archive(
    pool: &SqlitePool,
    conversation_id: &str,
    branch_id: &str,
    user_id: &str,
) -> AppResult<()> {
    let changed = sqlx::query(
        "UPDATE ai_conversations SET archived_at = ?, updated_at = ?
          WHERE conversation_id = ? AND branch_id = ? AND user_id = ?
            AND archived_at IS NULL",
    )
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(conversation_id)
    .bind(branch_id)
    .bind(user_id)
    .execute(pool)
    .await?
    .rows_affected();
    if changed == 0 {
        return Err(AppError::NotFound("Conversation not found".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
