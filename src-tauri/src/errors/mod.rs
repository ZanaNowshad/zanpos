use serde::Serialize;
use std::borrow::Cow;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    // Display intentionally hides the raw sqlx error — internal details go to
    // the tracing log only.  The Serialize impl delegates to user_message() which
    // also returns a generic string, so callers never see SQL or constraint names.
    #[error("A database error occurred")]
    Database(#[from] sqlx::Error),

    #[error("A database migration error occurred")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Permission denied: {0}")]
    Permission(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Unknown barcode: {0}")]
    GhostBarcode(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}

impl AppError {
    /// Internal-only database detail for diagnostics and sync classification.
    /// Tauri serialization still uses `user_message()` and never exposes this.
    pub(crate) fn internal_database_detail(&self) -> String {
        match self {
            AppError::Database(error) => error.to_string(),
            _ => self.to_string(),
        }
    }

    pub(crate) fn is_foreign_key_constraint(&self) -> bool {
        match self {
            AppError::Database(error) => error.as_database_error().is_some_and(|db| {
                db.code().as_deref() == Some("787") || db.message().contains("FOREIGN KEY")
            }),
            _ => false,
        }
    }

    pub(crate) fn is_database_busy(&self) -> bool {
        match self {
            AppError::Database(error) => error.as_database_error().is_some_and(|db| {
                matches!(db.code().as_deref(), Some("5" | "261" | "262"))
                    || db.message().contains("database is locked")
            }),
            _ => false,
        }
    }

    pub(crate) fn is_duplicate_barcode_constraint(&self) -> bool {
        match self {
            AppError::Database(error) => error
                .as_database_error()
                .is_some_and(|db| db.message().contains("barcode already in use")),
            _ => false,
        }
    }

    /// Returns a user-safe error message that does not include internal details.
    pub fn user_message(&self) -> Cow<'_, str> {
        match self {
            AppError::Database(e) => {
                tracing::error!("DB error: {}", e);
                // Give actionable guidance based on SQLite error code without leaking SQL.
                if let Some(db_err) = e.as_database_error() {
                    let code = db_err.code().unwrap_or_default();
                    match code.as_ref() {
                        "5" | "261" | "262" => {
                            // SQLITE_BUSY (5) / SQLITE_BUSY_RECOVERY (261) / SQLITE_BUSY_SNAPSHOT (262)
                            Cow::Borrowed("Database is busy — please wait a moment and try again.")
                        }
                        "19" | "787" | "1555" | "2067" => {
                            // SQLITE_CONSTRAINT family: generic (19), FK (787), PK (1555), UNIQUE (2067)
                            Cow::Borrowed(
                                "This change conflicts with existing data. Check for duplicates and try again.",
                            )
                        }
                        "1299" => {
                            // SQLITE_CONSTRAINT_NOTNULL — a required field was not provided.
                            // (Previously fell through to the generic message below, masking real bugs.)
                            Cow::Borrowed(
                                "A required field was missing — please try again or report this.",
                            )
                        }
                        "1043" => {
                            // SQLITE_CONSTRAINT_CHECK — a value failed a table CHECK rule.
                            Cow::Borrowed(
                                "A value didn't pass validation — please adjust it and try again.",
                            )
                        }
                        "11" | "266" | "267" => {
                            // SQLITE_CORRUPT family
                            Cow::Borrowed(
                                "Database appears damaged — please restore from a recent backup.",
                            )
                        }
                        other => Cow::Owned(unexpected_database_message(other)),
                    }
                } else {
                    // Not a SQLite failure at all — a pool timeout or a closed
                    // connection. There is no code to name, so say only what is known.
                    Cow::Borrowed(GENERIC_DATABASE_MESSAGE)
                }
            }
            AppError::Migration(e) => {
                tracing::error!("Migration error: {}", e);
                Cow::Borrowed("The app needs to update its database — please restart the app.")
            }
            AppError::Validation(m) => Cow::Borrowed(m),
            AppError::NotFound(m) => Cow::Borrowed(m),
            AppError::Permission(m) => Cow::Borrowed(m),
            AppError::Conflict(m) => Cow::Borrowed(m),
            AppError::GhostBarcode(_) => Cow::Borrowed(
                "Unknown barcode recorded — your manager can resolve it in Alerts; keep selling.",
            ),
            AppError::Internal(_) => {
                Cow::Borrowed("An unexpected error occurred — please try again.")
            }
        }
    }
}

const GENERIC_DATABASE_MESSAGE: &str =
    "Something went wrong — please try again. If the problem persists, restart the app.";

/// Fallback for a SQLite condition there is no specific advice for.
///
/// The bare sentence had to be diagnosed from the log, and the log is on the
/// till — a screenshot is usually all the evidence that reaches anyone who can
/// act on it. Naming the numeric code makes the report self-sufficient. A code
/// is a SQLite condition, not SQL, data, or a constraint name, so this keeps
/// the promise made on `AppError::Database`.
fn unexpected_database_message(code: &str) -> String {
    if code.is_empty() {
        GENERIC_DATABASE_MESSAGE.to_string()
    } else {
        format!("{GENERIC_DATABASE_MESSAGE} (database code {code})")
    }
}

// Tauri commands must return Serialize errors
impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.user_message().as_ref())
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ghost_barcode_message_tells_cashier_what_to_do() {
        let err = AppError::GhostBarcode("6291000000012".into());
        let msg = err.user_message();
        assert!(msg.contains("Unknown barcode recorded"));
        assert!(msg.contains("manager"));
        assert!(msg.contains("keep selling"));
    }

    #[test]
    fn an_unrecognised_database_code_is_named_so_a_screenshot_is_enough() {
        // 1811 is SQLITE_CONSTRAINT_TRIGGER: real, and deliberately not one of
        // the codes that carries its own advice, so it takes the fallback.
        let msg = unexpected_database_message("1811");
        assert!(msg.starts_with("Something went wrong"), "{msg}");
        assert!(msg.contains("database code 1811"), "{msg}");
    }

    #[test]
    fn a_missing_code_does_not_produce_an_empty_parenthetical() {
        assert_eq!(unexpected_database_message(""), GENERIC_DATABASE_MESSAGE);
    }

    #[tokio::test]
    async fn database_constraint_kind_is_available_to_internal_sync_logic() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE parent(id TEXT PRIMARY KEY)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE child(id TEXT PRIMARY KEY, parent_id TEXT NOT NULL REFERENCES parent(id))",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();

        let err = sqlx::query("INSERT INTO child(id, parent_id) VALUES ('C1', 'MISSING')")
            .execute(&pool)
            .await
            .unwrap_err();
        let app_error = AppError::Database(err);

        assert!(app_error.is_foreign_key_constraint());
        assert!(app_error.internal_database_detail().contains("FOREIGN KEY"));
        assert_eq!(app_error.to_string(), "A database error occurred");
    }
}
