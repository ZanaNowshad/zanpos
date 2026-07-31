use serde::Serialize;
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
    pub fn user_message(&self) -> &str {
        match self {
            AppError::Database(e) => {
                tracing::error!("DB error: {}", e);
                // Give actionable guidance based on SQLite error code without leaking SQL.
                if let Some(db_err) = e.as_database_error() {
                    let code = db_err.code().unwrap_or_default();
                    match code.as_ref() {
                        "5" | "261" | "262" => {
                            // SQLITE_BUSY (5) / SQLITE_BUSY_RECOVERY (261) / SQLITE_BUSY_SNAPSHOT (262)
                            "Database is busy — please wait a moment and try again."
                        }
                        "19" | "787" | "1555" | "2067" => {
                            // SQLITE_CONSTRAINT family: generic (19), FK (787), PK (1555), UNIQUE (2067)
                            "This change conflicts with existing data. Check for duplicates and try again."
                        }
                        "1299" => {
                            // SQLITE_CONSTRAINT_NOTNULL — a required field was not provided.
                            // (Previously fell through to the generic message below, masking real bugs.)
                            "A required field was missing — please try again or report this."
                        }
                        "1043" => {
                            // SQLITE_CONSTRAINT_CHECK — a value failed a table CHECK rule.
                            "A value didn't pass validation — please adjust it and try again."
                        }
                        "11" | "266" | "267" => {
                            // SQLITE_CORRUPT family
                            "Database appears damaged — please restore from a recent backup."
                        }
                        _ => {
                            "Something went wrong — please try again. If the problem persists, restart the app."
                        }
                    }
                } else {
                    "Something went wrong — please try again. If the problem persists, restart the app."
                }
            }
            AppError::Migration(e) => {
                tracing::error!("Migration error: {}", e);
                "The app needs to update its database — please restart the app."
            }
            AppError::Validation(m) => m,
            AppError::NotFound(m) => m,
            AppError::Permission(m) => m,
            AppError::Conflict(m) => m,
            AppError::GhostBarcode(_) => {
                "Unknown barcode recorded — your manager can resolve it in Alerts; keep selling."
            }
            AppError::Internal(_) => "An unexpected error occurred — please try again.",
        }
    }
}

// Tauri commands must return Serialize errors
impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.user_message())
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
