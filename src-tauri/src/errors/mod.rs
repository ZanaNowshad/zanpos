use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Migration error: {0}")]
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
    /// Returns a user-safe error message that does not include internal details.
    pub fn user_message(&self) -> &str {
        match self {
            AppError::Database(_) => "A database error occurred. Check the application log for details.",
            AppError::Migration(_) => "A database migration error occurred. Check the application log for details.",
            AppError::Validation(m) => m,
            AppError::NotFound(m) => m,
            AppError::Permission(m) => m,
            AppError::Conflict(m) => m,
            AppError::GhostBarcode(barcode) => barcode,
            AppError::Internal(_) => "An internal error occurred. Check the application log for details.",
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
