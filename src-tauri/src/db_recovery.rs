use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedDatabase {
    pub effective_path: PathBuf,
    pub recovery_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExistingDatabaseState {
    Missing,
    Valid,
    Incompatible,
    Unreadable,
}

pub async fn prepare_existing_database(
    app_data_dir: &Path,
    db_path: &Path,
) -> std::io::Result<PreparedDatabase> {
    if let Some(prepared) = import_legacy_database_if_needed(app_data_dir, db_path).await? {
        return Ok(prepared);
    }

    match classify_existing_database(db_path).await {
        ExistingDatabaseState::Missing | ExistingDatabaseState::Valid => Ok(PreparedDatabase {
            effective_path: db_path.to_path_buf(),
            recovery_dir: None,
        }),
        ExistingDatabaseState::Incompatible | ExistingDatabaseState::Unreadable => {
            let recovery_dir = copy_sqlite_family_to_recovery(app_data_dir, db_path, "startup")?;
            remove_sqlite_family(db_path)?;
            let effective_path = if db_path.exists() {
                app_data_dir.join("zanpos_v2.db")
            } else {
                db_path.to_path_buf()
            };
            Ok(PreparedDatabase {
                effective_path,
                recovery_dir: Some(recovery_dir),
            })
        }
    }
}

async fn import_legacy_database_if_needed(
    app_data_dir: &Path,
    db_path: &Path,
) -> std::io::Result<Option<PreparedDatabase>> {
    let current_state = classify_existing_database(db_path).await;
    let current_setup_complete = database_setup_complete(db_path).await;
    let can_import = matches!(current_state, ExistingDatabaseState::Missing)
        || matches!(current_setup_complete, Some(false) | None);
    if !can_import {
        return Ok(None);
    }

    let Some(legacy_db) = best_legacy_database(app_data_dir, db_path).await else {
        return Ok(None);
    };

    let recovery_dir = if db_path.exists()
        || db_path.with_extension("db-wal").exists()
        || db_path.with_extension("db-shm").exists()
        || db_path.with_extension("db-journal").exists()
    {
        Some(copy_sqlite_family_to_recovery(
            app_data_dir,
            db_path,
            "pre-legacy-import",
        )?)
    } else {
        None
    };

    remove_sqlite_family(db_path)?;
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    copy_sqlite_family_to_target(&legacy_db, db_path)?;
    tracing::info!(
        "Imported legacy ZanPOS database from {} into {}",
        legacy_db.display(),
        db_path.display()
    );

    Ok(Some(PreparedDatabase {
        effective_path: db_path.to_path_buf(),
        recovery_dir,
    }))
}

async fn best_legacy_database(app_data_dir: &Path, current_db_path: &Path) -> Option<PathBuf> {
    let mut candidates = legacy_database_candidates(app_data_dir, current_db_path);
    candidates.sort();
    candidates.dedup();

    let mut valid = Vec::new();
    for candidate in candidates {
        if candidate == current_db_path || !candidate.exists() {
            continue;
        }
        if classify_existing_database(&candidate).await == ExistingDatabaseState::Valid
            && database_setup_complete(&candidate).await == Some(true)
        {
            let modified = std::fs::metadata(&candidate)
                .and_then(|m| m.modified())
                .ok();
            valid.push((modified, candidate));
        }
    }

    valid.sort_by_key(|(modified, _)| *modified);
    valid.pop().map(|(_, path)| path)
}

fn legacy_database_candidates(app_data_dir: &Path, current_db_path: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(parent) = app_data_dir.parent() {
        roots.push(parent.to_path_buf());
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        roots.push(PathBuf::from(appdata));
    }
    if let Some(localappdata) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(localappdata));
    }

    let names = [
        "com.zanpos.app",
        "com.zanpos.zanpos",
        "com.zanpos.pos",
        "com.zanpos",
        "ZANPOS",
        "zanpos",
        "Ruflo",
        "ruflo",
    ];

    let mut out = Vec::new();
    for root in roots {
        for name in names {
            out.push(root.join(name).join("zanpos.db"));
        }
    }
    out.retain(|path| path != current_db_path);
    out
}

async fn database_setup_complete(db_path: &Path) -> Option<bool> {
    if !db_path.exists() {
        return None;
    }
    let url = format!("sqlite:{}?mode=ro", db_path.to_string_lossy());
    let pool = sqlx::SqlitePool::connect(&url).await.ok()?;
    let value: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key='setup_complete'")
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten();
    pool.close().await;
    Some(value.as_deref() == Some("1"))
}

pub fn recover_after_init_failure(
    app_data_dir: &Path,
    db_path: &Path,
) -> std::io::Result<PreparedDatabase> {
    let recovery_dir = if db_path.exists()
        || db_path.with_extension("db-wal").exists()
        || db_path.with_extension("db-shm").exists()
        || db_path.with_extension("db-journal").exists()
    {
        Some(copy_sqlite_family_to_recovery(
            app_data_dir,
            db_path,
            "init-failed",
        )?)
    } else {
        None
    };
    remove_sqlite_family(db_path)?;
    let effective_path = if db_path.exists() {
        app_data_dir.join("zanpos_v2.db")
    } else {
        db_path.to_path_buf()
    };
    Ok(PreparedDatabase {
        effective_path,
        recovery_dir,
    })
}

async fn classify_existing_database(db_path: &Path) -> ExistingDatabaseState {
    if !db_path.exists() {
        return ExistingDatabaseState::Missing;
    }

    let url = format!("sqlite:{}?mode=ro", db_path.to_string_lossy());
    let pool = match sqlx::SqlitePool::connect(&url).await {
        Ok(pool) => pool,
        Err(e) => {
            tracing::warn!("Could not inspect database before startup: {e}");
            return ExistingDatabaseState::Unreadable;
        }
    };

    let has_marker: Result<String, _> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='table' AND name='sync_watermark'",
    )
    .fetch_one(&pool)
    .await;
    pool.close().await;

    if has_marker.is_ok() {
        ExistingDatabaseState::Valid
    } else {
        ExistingDatabaseState::Incompatible
    }
}

fn copy_sqlite_family_to_recovery(
    app_data_dir: &Path,
    db_path: &Path,
    reason: &str,
) -> std::io::Result<PathBuf> {
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let recovery_dir = app_data_dir
        .join("recovery")
        .join(format!("{stamp}-{reason}-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&recovery_dir)?;

    for source in sqlite_family_paths(db_path) {
        if source.exists() {
            let target = recovery_dir.join(
                source
                    .file_name()
                    .unwrap_or_else(|| std::ffi::OsStr::new("zanpos.db")),
            );
            std::fs::copy(&source, target)?;
        }
    }

    Ok(recovery_dir)
}

fn copy_sqlite_family_to_target(source_db: &Path, target_db: &Path) -> std::io::Result<()> {
    let source_base = source_db.to_string_lossy();
    let target_base = target_db.to_string_lossy();
    let pairs = [
        (source_db.to_path_buf(), target_db.to_path_buf()),
        (
            PathBuf::from(format!("{source_base}-wal")),
            PathBuf::from(format!("{target_base}-wal")),
        ),
        (
            PathBuf::from(format!("{source_base}-shm")),
            PathBuf::from(format!("{target_base}-shm")),
        ),
        (
            PathBuf::from(format!("{source_base}-journal")),
            PathBuf::from(format!("{target_base}-journal")),
        ),
    ];

    for (source, target) in pairs {
        if source.exists() {
            std::fs::copy(source, target)?;
        }
    }
    Ok(())
}

fn remove_sqlite_family(db_path: &Path) -> std::io::Result<()> {
    for path in sqlite_family_paths(db_path) {
        if path.exists() {
            if let Err(e) = std::fs::remove_file(&path) {
                tracing::warn!(
                    "Could not remove recovered database file {}: {e}",
                    path.display()
                );
            }
        }
    }
    Ok(())
}

fn sqlite_family_paths(db_path: &Path) -> Vec<PathBuf> {
    let base = db_path.to_string_lossy();
    vec![
        db_path.to_path_buf(),
        PathBuf::from(format!("{base}-wal")),
        PathBuf::from(format!("{base}-shm")),
        PathBuf::from(format!("{base}-journal")),
    ]
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    fn temp_app_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("zanpos-db-recovery-{name}-{}", ulid::Ulid::new()));
        std::fs::create_dir_all(&dir).expect("temp app dir");
        dir
    }

    #[tokio::test]
    async fn valid_database_is_left_in_place() {
        let app_dir = temp_app_dir("valid");
        let db_path = app_dir.join("zanpos.db");
        let pool = sqlx::SqlitePool::connect(&format!("sqlite:{}?mode=rwc", db_path.display()))
            .await
            .expect("create sqlite db");
        sqlx::query("CREATE TABLE sync_watermark (table_name TEXT PRIMARY KEY)")
            .execute(&pool)
            .await
            .expect("create marker");
        pool.close().await;

        let prepared = super::prepare_existing_database(&app_dir, &db_path)
            .await
            .expect("prepare valid db");

        assert_eq!(prepared.effective_path, db_path);
        assert!(prepared.recovery_dir.is_none());
        assert!(db_path.exists(), "valid database must stay in place");
    }

    #[tokio::test]
    async fn incompatible_database_is_copied_to_recovery_before_reuse() {
        let app_dir = temp_app_dir("incompatible");
        let db_path = app_dir.join("zanpos.db");
        let pool = sqlx::SqlitePool::connect(&format!("sqlite:{}?mode=rwc", db_path.display()))
            .await
            .expect("create sqlite db");
        sqlx::query("CREATE TABLE legacy_sync_queue (id TEXT PRIMARY KEY)")
            .execute(&pool)
            .await
            .expect("create legacy marker");
        pool.close().await;

        let prepared = super::prepare_existing_database(&app_dir, &db_path)
            .await
            .expect("prepare incompatible db");

        let recovery_dir = prepared.recovery_dir.expect("recovery dir");
        assert_eq!(prepared.effective_path, db_path);
        assert!(
            recovery_dir.join("zanpos.db").exists(),
            "original database must be copied before reuse"
        );
        assert!(
            !db_path.exists(),
            "incompatible original is removed only after recovery copy exists"
        );
    }

    async fn create_minimal_valid_db(path: &std::path::Path, setup_complete: bool) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("parent dir");
        }
        let pool = sqlx::SqlitePool::connect(&format!("sqlite:{}?mode=rwc", path.display()))
            .await
            .expect("create sqlite db");
        sqlx::query("CREATE TABLE sync_watermark (table_name TEXT PRIMARY KEY)")
            .execute(&pool)
            .await
            .expect("create marker");
        sqlx::query("CREATE TABLE app_config (key TEXT PRIMARY KEY, value TEXT)")
            .execute(&pool)
            .await
            .expect("create app config");
        sqlx::query("INSERT INTO app_config (key, value) VALUES ('setup_complete', ?)")
            .bind(if setup_complete { "1" } else { "0" })
            .execute(&pool)
            .await
            .expect("insert setup flag");
        pool.close().await;
    }

    async fn setup_flag(path: &std::path::Path) -> String {
        let pool = sqlx::SqlitePool::connect(&format!("sqlite:{}?mode=ro", path.display()))
            .await
            .expect("open sqlite db");
        let value: String =
            sqlx::query_scalar("SELECT value FROM app_config WHERE key='setup_complete'")
                .fetch_one(&pool)
                .await
                .expect("setup flag");
        pool.close().await;
        value
    }

    #[tokio::test]
    async fn missing_current_database_imports_completed_legacy_database() {
        let root = temp_app_dir("legacy-missing");
        let app_dir = root.join("com.super.zanpos");
        let legacy_dir = root.join("com.zanpos.app");
        let current_db = app_dir.join("zanpos.db");
        let legacy_db = legacy_dir.join("zanpos.db");
        create_minimal_valid_db(&legacy_db, true).await;

        let prepared = super::prepare_existing_database(&app_dir, &current_db)
            .await
            .expect("prepare db");

        assert_eq!(prepared.effective_path, current_db);
        assert!(prepared.recovery_dir.is_none());
        assert_eq!(setup_flag(&current_db).await, "1");
        assert!(legacy_db.exists(), "legacy source must not be removed");
    }

    #[tokio::test]
    async fn completed_current_database_is_not_replaced_by_legacy_database() {
        let root = temp_app_dir("legacy-completed");
        let app_dir = root.join("com.super.zanpos");
        let legacy_dir = root.join("com.zanpos.app");
        let current_db = app_dir.join("zanpos.db");
        let legacy_db = legacy_dir.join("zanpos.db");
        create_minimal_valid_db(&current_db, true).await;
        create_minimal_valid_db(&legacy_db, true).await;

        sqlx::SqlitePool::connect(&format!("sqlite:{}?mode=rwc", current_db.display()))
            .await
            .expect("open current")
            .close()
            .await;

        let prepared = super::prepare_existing_database(&app_dir, &current_db)
            .await
            .expect("prepare db");

        assert_eq!(prepared.effective_path, current_db);
        assert!(prepared.recovery_dir.is_none());
        assert_eq!(setup_flag(&current_db).await, "1");
    }
}
