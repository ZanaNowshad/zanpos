pub mod helpers;
pub mod repositories;

use crate::errors::AppResult;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    SqlitePool,
};
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

pub async fn init_db(db_path: &str) -> AppResult<SqlitePool> {
    // Ensure parent directory exists
    if let Some(parent) = Path::new(db_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let url = format!("sqlite:{}?mode=rwc", db_path);

    // ── Per-connection settings ────────────────────────────────────────────────
    // PRAGMA settings are *per-connection* in SQLite. Using SqliteConnectOptions
    // ensures every connection spawned by the pool has them applied, not just the
    // first one that happens to be acquired at startup.
    //
    // Exceptions:
    //   journal_mode = WAL  — stored in the DB file header, persists across conns.
    //   foreign_keys        — MUST be per-connection; SQLite default is OFF.
    let connect_opts = SqliteConnectOptions::from_str(&url)?
        // WAL gives concurrent reads + write without blocking
        .journal_mode(SqliteJournalMode::Wal)
        // Enforce FK constraints on every connection — default is OFF in SQLite
        .foreign_keys(true)
        // NORMAL sync is safe in WAL mode and avoids a full-flush on every write
        .synchronous(SqliteSynchronous::Normal)
        // Wait up to 15 s for the write lock before returning SQLITE_BUSY. A large
        // catalog import and the sync worker both write; 3 s wasn't enough under that
        // contention (log showed 3.4 s waits → "database is locked"). 15 s lets the
        // writers queue and proceed instead of erroring.
        .busy_timeout(Duration::from_secs(15))
        // Increase default cache from 2 MB to 8 MB for faster repeated reads
        .pragma("cache_size", "-8000")
        // L2: WAL auto-checkpoint threshold — checkpoint when WAL exceeds ~1000 pages
        // (~4 MB at default page size).  Without this the WAL file grows unbounded until
        // the process exits and SQLite runs the passive checkpoint on shutdown.
        .pragma("wal_autocheckpoint", "1000");

    let pool = SqlitePoolOptions::new()
        // SQLite WAL allows multiple readers but only one writer at a time.
        // A small pool avoids write-lock contention across Tauri command threads.
        .max_connections(4)
        // Surface a clear error instead of hanging indefinitely
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(connect_opts)
        .await?;

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;

    tracing::info!("Database initialized at {}", db_path);
    Ok(pool)
}
