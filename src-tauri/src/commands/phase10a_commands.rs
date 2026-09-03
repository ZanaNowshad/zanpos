use crate::commands::{rbac, sync_commands};
use crate::db::repositories::audit_hash;
use crate::errors::{AppError, AppResult};
use crate::sync::scope::report_scope;
use crate::AppState;
use serde::Serialize;
use sqlx::Row;
use tauri::Manager;
/// Phase 10a commands: session timeout config, DB backup, tax report, audit log viewer.
use tauri::State;

// ─── Session timeout ──────────────────────────────────────────────────────────

/// Returns the configured idle timeout in minutes (defaults to 5).
#[tauri::command]
pub async fn app_config_get_timeout(state: State<'_, AppState>) -> Result<i64, AppError> {
    let val: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'idle_timeout_minutes'")
            .fetch_optional(&state.db)
            .await?
            .flatten();

    Ok(val.and_then(|v| v.parse::<i64>().ok()).unwrap_or(5))
}

/// Sets the idle timeout in minutes (0–60). 0 = never lock. Requires owner or manager role.
#[tauri::command]
pub async fn app_config_set_timeout(
    minutes: i64,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    if !(0..=60).contains(&minutes) {
        return Err(AppError::Validation(
            "Timeout must be between 0 and 60 minutes (0 = never lock)".into(),
        ));
    }
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO app_config(key, value, updated_at) VALUES('idle_timeout_minutes', ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(minutes.to_string())
    .bind(&now)
    .execute(&state.db)
    .await?;
    sync_commands::schedule_immediate_sync(&state);
    Ok(())
}

// ─── DB Backup ────────────────────────────────────────────────────────────────

/// Copy the SQLite database file. Owner-only operation.
/// If dest_path is empty, saves to the user's Documents folder with a timestamp.
/// Returns the final destination path.
#[tauri::command]
pub async fn db_backup(
    dest_path: String,
    actor_user_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    rbac::owner_only(&state.db, &actor_user_id).await?;
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Internal(format!("Could not resolve app data dir: {e}")))?;
    let src = app_data.join("zanpos.db");

    let dest = if dest_path.trim().is_empty() {
        // Auto-generate path in Documents
        let docs = app
            .path()
            .document_dir()
            .map_err(|e| AppError::Internal(format!("Could not resolve documents dir: {e}")))?;
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        docs.join(format!("zanpos-backup-{ts}.db"))
    } else {
        std::path::PathBuf::from(&dest_path)
    };

    // `VACUUM INTO` writes a consistent snapshot in one read transaction.
    //
    // This was `PRAGMA wal_checkpoint(TRUNCATE)` then `fs::copy` — two steps,
    // with a window between them in which a sale can commit. A copy taken across
    // that window is not promised to be a valid database, and a backup that is
    // not a database is worse than none: it looks like safety.
    //
    // It refuses to overwrite, so an existing destination is cleared first; the
    // caller chose the path, and `fs::copy` overwrote too.
    let _ = std::fs::remove_file(&dest);
    let dest_sql = dest.display().to_string();
    if dest_sql.contains('\'') {
        return Err(AppError::Validation(
            "The backup path cannot contain a quote character.".into(),
        ));
    }
    sqlx::query(&format!("VACUUM INTO '{dest_sql}'"))
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(format!("Backup failed: {e}")))?;
    let _ = &src;

    Ok(dest.to_string_lossy().to_string())
}

// ─── Tax report ───────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct TaxDayRow {
    pub day: String,
    pub transaction_count: i64,
    pub tax_minor: i64,
    pub cumulative_minor: i64,
}

/// Return daily tax totals for a date range.
#[tauri::command]
pub async fn report_tax_by_day(
    branch_id: String,
    from_date: String,
    to_date: String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<TaxDayRow>, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    // Caller-supplied `branch_id` is not trusted for branch-scoped data;
    // the scope comes from the actor's own record. The parameter remains
    // only to preserve the existing invoke contract.
    let _ = branch_id;
    let branch_id = rbac::actor_branch_id(&state.db, &actor_user_id).await?;
    let (scope, origin_device_id) = report_scope(&state.db).await;
    let scope_str = scope.as_str();
    let rows = sqlx::query(
        "SELECT business_date AS day,
                COUNT(*) AS transaction_count,
                COALESCE(SUM(tax_total_minor), 0) AS tax_minor
         FROM sales
         WHERE branch_id = ? AND business_date BETWEEN ? AND ?
           AND status != 'voided'
           AND (? = 'all' OR origin_device_id = ?)
         GROUP BY business_date
         ORDER BY business_date ASC",
    )
    .bind(&branch_id)
    .bind(&from_date)
    .bind(&to_date)
    .bind(scope_str)
    .bind(&origin_device_id)
    .fetch_all(&state.db)
    .await?;

    let mut cumulative: i64 = 0;
    let result = rows
        .iter()
        .map(|r| {
            let tax: i64 = r.get("tax_minor");
            cumulative += tax;
            TaxDayRow {
                day: r.get("day"),
                transaction_count: r.get("transaction_count"),
                tax_minor: tax,
                cumulative_minor: cumulative,
            }
        })
        .collect();

    Ok(result)
}

// ─── Audit log viewer ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AuditLogRow {
    pub audit_log_id: String,
    pub event_type: String,
    pub entity_type: String,
    pub entity_id: Option<String>,
    pub actor_user_id: Option<String>,
    pub created_at: String,
}

/// List audit log entries with date-range filter and offset-based pagination.
/// Returns up to 50 rows per page (page is 0-based). Requires manager or owner.
#[tauri::command]
pub async fn audit_log_list(
    from: String,
    to: String,
    page: i64,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<AuditLogRow>, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let limit: i64 = 50;
    let offset = page * limit;

    // Append time bounds so date strings work as ISO8601 prefixes
    let from_ts = format!("{} 00:00:00", from);
    let to_ts = format!("{} 23:59:59", to);

    let rows = sqlx::query(
        "SELECT audit_log_id, event_type, entity_type, entity_id, actor_user_id, created_at
         FROM audit_logs
         WHERE created_at >= ? AND created_at <= ?
         ORDER BY created_at DESC
         LIMIT ? OFFSET ?",
    )
    .bind(&from_ts)
    .bind(&to_ts)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    Ok(rows
        .iter()
        .map(|r| AuditLogRow {
            audit_log_id: r.get("audit_log_id"),
            event_type: r.get("event_type"),
            entity_type: r.get("entity_type"),
            entity_id: r.get("entity_id"),
            actor_user_id: r.get("actor_user_id"),
            created_at: r.get("created_at"),
        })
        .collect())
}

// ─── Audit hash-chain verification ───────────────────────────────────────────

/// Verify the SHA-256 hash chain for audit_logs written by this device.
/// Returns a summary: total rows, legacy rows (pre-chain), verified count,
/// and counts of broken-hash or broken-link anomalies.
/// Requires manager or owner role.
#[tauri::command]
pub async fn audit_verify_chain(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<audit_hash::ChainVerifyResult> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;

    let device_id: Option<String> = sqlx::query_scalar(
        "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .flatten();

    let device_id = device_id.unwrap_or_default();
    audit_hash::verify_chain(&state.db, &device_id).await
}

// ─── AppResult alias re-export for convenience ────────────────────────────────
pub type _AppResult<T> = AppResult<T>;

// ─────────────────────────────────────────────────────────────────────────────
// Backup/restore drill tests
// Tests the underlying file-copy logic that `db_backup` relies on.
// The Tauri AppHandle cannot be constructed in unit tests; these tests
// validate the backup file is a valid SQLite database and can be used
// to restore state into a new in-memory pool — exactly what a restore drill does.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    /// SQLite file magic bytes: every valid .db file starts with these 16 bytes.
    const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";

    /// Convert a PathBuf to a SQLite connection URL with forward slashes (required on Windows).
    fn sqlite_url(path: &std::path::Path) -> String {
        let s = path.to_str().unwrap_or("").replace('\\', "/");
        format!("sqlite:///{}?mode=rwc", s.trim_start_matches('/'))
    }

    // ── 1. A backup copy has valid SQLite magic bytes ─────────────────────────
    #[tokio::test]
    async fn test_backup_file_is_valid_sqlite() {
        let dir = std::env::temp_dir();
        let src = dir.join(format!("zanpos-test-src-{}.db", ulid::Ulid::new()));
        let dst = dir.join(format!("zanpos-test-dst-{}.db", ulid::Ulid::new()));

        // Bootstrap: create a minimal SQLite DB at src path
        {
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect(&sqlite_url(&src))
                .await
                .expect("create src db");
            sqlx::query("CREATE TABLE t (id INTEGER PRIMARY KEY)")
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO t VALUES (42)")
                .execute(&pool)
                .await
                .unwrap();
            pool.close().await;
        }

        // Simulate db_backup: copy src → dst
        std::fs::copy(&src, &dst).expect("backup copy");

        // Verify dst starts with SQLite magic
        let header = std::fs::read(&dst).expect("read backup");
        assert!(
            header.starts_with(SQLITE_MAGIC),
            "backup file must begin with SQLite magic bytes"
        );
        assert!(
            header.len() >= 4096,
            "backup file must be at least one SQLite page (4096 bytes), got {}",
            header.len()
        );

        // Cleanup
        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_file(&dst);
    }

    // ── 2. A restore: opening the backup as a pool returns the original data ──
    #[tokio::test]
    async fn test_backup_restores_original_data() {
        let dir = std::env::temp_dir();
        let src = dir.join(format!("zanpos-rtest-src-{}.db", ulid::Ulid::new()));
        let bak = dir.join(format!("zanpos-rtest-bak-{}.db", ulid::Ulid::new()));

        // Seed the "live" DB
        {
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect(&sqlite_url(&src))
                .await
                .expect("live db");
            sqlx::query("CREATE TABLE restore_check (val TEXT)")
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO restore_check VALUES ('hello-restore')")
                .execute(&pool)
                .await
                .unwrap();
            pool.close().await;
        }

        // Backup
        std::fs::copy(&src, &bak).expect("backup");

        // Simulate restore: open the backup file as the "restored" pool
        let restored = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&sqlite_url(&bak))
            .await
            .expect("restored pool");

        let val: String = sqlx::query_scalar("SELECT val FROM restore_check LIMIT 1")
            .fetch_one(&restored)
            .await
            .expect("query restored");

        assert_eq!(
            val, "hello-restore",
            "restored DB must contain the original data"
        );

        // Cleanup
        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_file(&bak);
    }

    // ── 3. Backup to an invalid path returns an error (not a panic) ───────────
    #[test]
    fn test_backup_to_invalid_path_errors_not_panics() {
        let result = std::fs::copy(
            "C:\\does-not-exist\\zanpos.db",
            "C:\\also-does-not-exist\\backup.db",
        );
        assert!(result.is_err(), "copy to invalid path must return Err");
    }

    // ── 4. Incremental write: new data written after backup is NOT in backup ───
    // Validates that backup captures point-in-time state.
    #[tokio::test]
    async fn test_backup_is_point_in_time() {
        let dir = std::env::temp_dir();
        let src = dir.join(format!("zanpos-pit-src-{}.db", ulid::Ulid::new()));
        let bak = dir.join(format!("zanpos-pit-bak-{}.db", ulid::Ulid::new()));

        {
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect(&sqlite_url(&src))
                .await
                .unwrap();
            sqlx::query("CREATE TABLE pit (n INTEGER)")
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO pit VALUES (1)")
                .execute(&pool)
                .await
                .unwrap();
            pool.close().await;
        }

        // Backup taken here (only row 1 exists)
        std::fs::copy(&src, &bak).unwrap();

        // Write more data to live DB AFTER backup
        {
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect(&sqlite_url(&src))
                .await
                .unwrap();
            sqlx::query("INSERT INTO pit VALUES (2)")
                .execute(&pool)
                .await
                .unwrap();
            pool.close().await;
        }

        // Open backup — must NOT contain row 2
        let restored = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&sqlite_url(&bak))
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pit")
            .fetch_one(&restored)
            .await
            .unwrap();
        assert_eq!(
            count, 1,
            "backup must not include writes made after the backup was taken"
        );

        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_file(&bak);
    }

    // ── 5. WAL checkpoint before backup produces a complete backup ─────────────
    // Validates the checkpoint step that must precede any file-copy backup.
    #[tokio::test]
    async fn test_wal_checkpoint_before_backup_captures_all_writes() {
        let dir = std::env::temp_dir();
        let src = dir.join(format!("zanpos-wal-{}.db", ulid::Ulid::new()));
        let bak = dir.join(format!("zanpos-wal-bak-{}.db", ulid::Ulid::new()));

        {
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect(&sqlite_url(&src))
                .await
                .unwrap();
            sqlx::query("PRAGMA journal_mode = WAL")
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("CREATE TABLE wal_t (x INTEGER)")
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO wal_t VALUES (99)")
                .execute(&pool)
                .await
                .unwrap();
            // Checkpoint before copy — flushes WAL into main db file
            sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
                .execute(&pool)
                .await
                .unwrap();
            pool.close().await;
        }

        std::fs::copy(&src, &bak).unwrap();

        let restored = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&sqlite_url(&bak))
            .await
            .unwrap();
        let val: i64 = sqlx::query_scalar("SELECT x FROM wal_t LIMIT 1")
            .fetch_one(&restored)
            .await
            .unwrap();
        assert_eq!(
            val, 99,
            "WAL checkpoint must flush committed data before backup"
        );

        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_file(bak.with_extension("db-wal"));
        let _ = std::fs::remove_file(bak.with_extension("db-shm"));
        let _ = std::fs::remove_file(&bak);
        let _ = std::fs::remove_file(src.with_extension("db-wal"));
        let _ = std::fs::remove_file(src.with_extension("db-shm"));
    }
}
