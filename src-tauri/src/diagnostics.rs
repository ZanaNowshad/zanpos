//! Trust spine: passive crash/error diagnostics.
//!
//! Complementary to `commands::system_health_commands` (active self-checks
//! with fix actions, run on demand) — this module is a passive event log:
//! Rust panics, frontend JS errors, and hub auth failures land in the
//! `diagnostics` table (via the fallback file if the DB isn't reachable),
//! rate-limited per signature, and later flushed by `telemetry_uploader`.
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::{Mutex, RwLock};

/// Which terminal these diagnostics come from, resolved once at startup and
/// re-resolved when the terminal is re-keyed.
///
/// This was a `OnceLock`, which cannot be written twice. `device_rekey` exists
/// for the case where a database was cloned onto a second machine and the two
/// installs share one identity; it issues a fresh one and rewrites every table
/// that names the terminal. It could not rewrite this. So the machine that had
/// just been given a new identity carried on stamping every diagnostic and every
/// telemetry batch with the retired one until the app was restarted — attributing
/// them to precisely the identity it had been told to stop using, in the data an
/// operator would consult to confirm the re-key worked.
static DEVICE_ID: RwLock<Option<String>> = RwLock::new(None);
static RATE_LIMIT: Mutex<Vec<(u64, std::time::Instant)>> = Mutex::new(Vec::new());

#[derive(Debug, Serialize, Deserialize)]
pub struct DiagnosticEvent {
    pub id: String,
    pub ts: String,
    pub device_id: String,
    pub severity: String,
    pub kind: String,
    pub message: String,
    pub stack: Option<String>,
    pub app_version: String,
    pub extra_json: Option<String>,
}

/// Record which terminal this is. Called at startup and again after a re-key.
pub fn set_device_id(id: String) {
    if let Ok(mut slot) = DEVICE_ID.write() {
        *slot = Some(id);
    }
}

/// Crate-visible so `telemetry_uploader` can stamp outgoing batches with the
/// same device id, without re-querying the DB on every event.
pub(crate) fn device_id() -> String {
    DEVICE_ID
        .read()
        .ok()
        .and_then(|slot| slot.clone())
        .unwrap_or_else(|| "unknown".into())
}

fn fallback_path() -> std::path::PathBuf {
    std::env::temp_dir().join("zanpos_diagnostics_pending.jsonl")
}

fn signature_hash(kind: &str, message: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    kind.hash(&mut h);
    message.chars().take(120).collect::<String>().hash(&mut h);
    h.finish()
}

/// Caps a tight loop at `MAX_PER_WINDOW` diagnostics per signature per
/// `WINDOW_SECS`. Recovers from a poisoned lock instead of unwrapping — this
/// is called directly from the panic hook (via `record`'s callers and from
/// `install_panic_hook` itself), so a second panic here would abort the
/// process. `into_inner()` on the poison error just accepts whatever state
/// the buffer was left in; that's fine for a best-effort rate limiter.
fn should_rate_limit(sig: u64) -> bool {
    const WINDOW_SECS: u64 = 60;
    const MAX_PER_WINDOW: usize = 5;
    let mut buf = RATE_LIMIT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let now = std::time::Instant::now();
    buf.retain(|(_, t)| now.duration_since(*t).as_secs() < WINDOW_SECS);
    if buf.iter().filter(|(s, _)| *s == sig).count() >= MAX_PER_WINDOW {
        return true;
    }
    buf.push((sig, now));
    if buf.len() > 200 {
        buf.remove(0);
    }
    false
}

/// Installs the process-wide panic hook. Must be called before
/// `tauri::Builder::default()` so it also covers panics during `setup()`.
/// Every step here is infallible or swallowed — a panic hook that itself
/// panics double-panics the process (abort), so nothing here may unwrap.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic (no message)".into());
        let loc = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_default();
        let stack = std::backtrace::Backtrace::force_capture().to_string();
        let sig = signature_hash("rust_panic", &message);
        if should_rate_limit(sig) {
            return;
        }
        let event = DiagnosticEvent {
            id: ulid::Ulid::new().to_string(),
            ts: chrono::Utc::now().to_rfc3339(),
            device_id: device_id(),
            severity: "panic".into(),
            kind: "rust_panic".into(),
            message,
            stack: Some(format!("{loc}\n{stack}")),
            app_version: env!("CARGO_PKG_VERSION").into(),
            extra_json: None,
        };
        if let Ok(line) = serde_json::to_string(&event) {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(fallback_path())
            {
                let _ = writeln!(f, "{line}");
            }
        }
    }));
}

/// Records one diagnostics row, rate-limited per (kind, message) signature.
/// Callers (the `log_diagnostic` command, the hub 401 path) never propagate
/// a failure here into user-facing behavior — telemetry writes are silent.
pub async fn record(
    pool: &SqlitePool,
    severity: &str,
    kind: &str,
    message: &str,
    stack: Option<&str>,
    extra_json: Option<&str>,
) -> Result<(), sqlx::Error> {
    let sig = signature_hash(kind, message);
    if should_rate_limit(sig) {
        return Ok(());
    }
    sqlx::query(
        "INSERT OR IGNORE INTO diagnostics (id, ts, device_id, severity, kind, message, stack, app_version, extra_json) VALUES (?,?,?,?,?,?,?,?,?)",
    )
    .bind(ulid::Ulid::new().to_string())
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(device_id())
    .bind(severity)
    .bind(kind)
    .bind(message)
    .bind(stack)
    .bind(env!("CARGO_PKG_VERSION"))
    .bind(extra_json)
    .execute(pool)
    .await?;
    Ok(())
}

/// Records one analytics event. Fire-and-forget by design: the sale path calls
/// this, so a failure is logged and swallowed rather than returned. Deliberately
/// not rate-limited — these are counted business events, not error storms, and
/// `telemetry_uploader` already caps the table's local size.
pub async fn record_event(pool: &SqlitePool, name: &str, props: Option<serde_json::Value>) {
    let props_json = props.and_then(|value| serde_json::to_string(&value).ok());
    let written = sqlx::query(
        "INSERT OR IGNORE INTO analytics_events (id, ts, name, props_json) VALUES (?,?,?,?)",
    )
    .bind(ulid::Ulid::new().to_string())
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(name)
    .bind(props_json)
    .execute(pool)
    .await;
    if let Err(e) = written {
        tracing::warn!("analytics: recording {name} failed: {e}");
    }
}

/// Flushes any panic events the hook wrote to the fallback file (because the
/// DB wasn't reachable from the panic hook) into the `diagnostics` table on
/// next launch, then clears the file. Best-effort: a malformed line or a
/// failed insert is skipped, never propagated.
pub async fn reconcile_pending(pool: &SqlitePool) {
    let path = fallback_path();
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return;
    };
    for line in contents.lines() {
        if let Ok(ev) = serde_json::from_str::<DiagnosticEvent>(line) {
            let _ = sqlx::query(
                "INSERT OR IGNORE INTO diagnostics (id, ts, device_id, severity, kind, message, stack, app_version, extra_json) VALUES (?,?,?,?,?,?,?,?,?)",
            )
            .bind(ev.id)
            .bind(ev.ts)
            .bind(ev.device_id)
            .bind(ev.severity)
            .bind(ev.kind)
            .bind(ev.message)
            .bind(ev.stack)
            .bind(ev.app_version)
            .bind(ev.extra_json)
            .execute(pool)
            .await;
        }
    }
    let _ = std::fs::remove_file(&path);
}

#[tauri::command]
pub async fn log_diagnostic(
    state: tauri::State<'_, crate::AppState>,
    kind: String,
    severity: Option<String>,
    message: String,
    stack: Option<String>,
    extra_json: Option<String>,
) -> Result<(), String> {
    if kind.trim().is_empty() || message.trim().is_empty() {
        return Err("kind and message are required".into());
    }
    let sev = match severity.as_deref() {
        Some("panic") => "panic",
        Some("warn") => "warn",
        _ => "error",
    };
    record(
        &state.db,
        sev,
        &kind,
        &message,
        stack.as_deref(),
        extra_json.as_deref(),
    )
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("memory db");
        sqlx::query(
            "CREATE TABLE diagnostics (
                id TEXT PRIMARY KEY NOT NULL,
                ts TEXT NOT NULL,
                device_id TEXT NOT NULL,
                severity TEXT NOT NULL,
                kind TEXT NOT NULL,
                message TEXT NOT NULL,
                stack TEXT,
                app_version TEXT NOT NULL,
                extra_json TEXT,
                uploaded_at TEXT
            )",
        )
        .execute(&pool)
        .await
        .expect("diagnostics table");
        pool
    }

    // NOTE: RATE_LIMIT is a process-wide static shared by every test in this
    // binary, and cargo runs test functions concurrently on separate threads.
    // Tests below deliberately avoid clearing that shared buffer (a reset in
    // one test would corrupt another test's in-flight count) and instead
    // rely on each test using a `kind`/`message` signature no other test in
    // this file reuses, since should_rate_limit() filters by exact signature
    // match — concurrent tests never share a bucket.

    #[tokio::test]
    async fn record_inserts_a_row() {
        let pool = test_pool().await;
        record(
            &pool,
            "error",
            "diag_test_single_insert",
            "single insert probe",
            None,
            None,
        )
        .await
        .unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM diagnostics")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn rate_limiter_caps_a_tight_loop_at_five_per_signature() {
        let pool = test_pool().await;
        for _ in 0..20 {
            record(
                &pool,
                "error",
                "diag_test_flood_signature",
                "flood probe",
                None,
                None,
            )
            .await
            .unwrap();
        }
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM diagnostics")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn reconcile_pending_flushes_fallback_file_and_clears_it() {
        let pool = test_pool().await;
        let path = fallback_path();
        let event = DiagnosticEvent {
            id: "01TESTULID00000000000000".into(),
            ts: "2026-01-01T00:00:00Z".into(),
            device_id: "test-device".into(),
            severity: "panic".into(),
            kind: "rust_panic".into(),
            message: "reconcile test".into(),
            stack: None,
            app_version: "0.0.0".into(),
            extra_json: None,
        };
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string(&event).unwrap()),
        )
        .expect("write fallback file");

        reconcile_pending(&pool).await;

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM diagnostics")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
        assert!(!path.exists());
    }

    /// A re-keyed terminal must stop stamping its old identity on diagnostics.
    ///
    /// `DEVICE_ID` was a `OnceLock`, so the second write was silently dropped
    /// and every event for the rest of the session named the retired terminal —
    /// in the data an operator reads to confirm the re-key took effect. The
    /// re-key exists for the cloned-database case, which is exactly when two
    /// installs are already claiming one identity.
    #[test]
    fn the_device_id_can_be_reissued_after_a_rekey() {
        set_device_id("BEFORE-REKEY".into());
        assert_eq!(device_id(), "BEFORE-REKEY");

        set_device_id("AFTER-REKEY".into());
        assert_eq!(
            device_id(),
            "AFTER-REKEY",
            "the re-keyed identity was ignored; diagnostics still name the retired terminal"
        );
    }
}
