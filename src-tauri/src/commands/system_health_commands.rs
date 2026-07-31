use crate::commands::{rbac, sync_commands};
use crate::errors::{AppError, AppResult};
use crate::sync_v2::client::HttpSyncClient;
use crate::AppState;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthSeverity {
    Ok,
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthFinding {
    pub code: String,
    pub severity: HealthSeverity,
    pub area: String,
    pub title: String,
    pub detail: String,
    pub fix_action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthDevice {
    pub device_id: String,
    pub label: String,
    pub role: String,
    pub status: String,
    pub ip: Option<String>,
    pub last_seen: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSummary {
    pub ok: bool,
    pub db_integrity: String,
    pub migration_count: i64,
    pub pending_sync_rows: i64,
    pub stuck_sync_rows: i64,
    pub device_count: i64,
    pub hub_mode: String,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealthReport {
    pub summary: HealthSummary,
    pub findings: Vec<HealthFinding>,
    pub devices: Vec<HealthDevice>,
    pub tables: Vec<SyncHealthTable>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncHealthTable {
    pub table: String,
    pub pending: i64,
    pub stuck: i64,
    pub max_attempts: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HealthFixInput {
    pub actor_user_id: String,
    pub fix_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthFixResult {
    pub fix_action: String,
    pub rows_changed: u64,
    pub message: String,
}

pub type HubSeenSnapshot = HashMap<String, (String, String)>;

fn safe_table_name(table: &str) -> bool {
    !table.is_empty() && table.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

async fn table_exists(pool: &SqlitePool, table: &str) -> bool {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?")
        .bind(table)
        .fetch_one(pool)
        .await
        .unwrap_or(0)
        > 0
}

async fn app_config(pool: &SqlitePool, key: &str) -> Option<String> {
    sqlx::query_scalar("SELECT value FROM app_config WHERE key=?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

pub async fn run_local_health_check(
    pool: &SqlitePool,
    sync_tables: &[&str],
) -> AppResult<SystemHealthReport> {
    let checked_at = chrono::Utc::now().to_rfc3339();
    let mut findings = Vec::new();
    let mut tables = Vec::new();

    let db_integrity = sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| format!("check failed: {e}"));
    if db_integrity != "ok" {
        findings.push(HealthFinding {
            code: "database.integrity".into(),
            severity: HealthSeverity::Critical,
            area: "Database".into(),
            title: "SQLite integrity check failed".into(),
            detail: db_integrity.clone(),
            fix_action: Some("backup_database".into()),
        });
    }

    let migration_count = if table_exists(pool, "_sqlx_migrations").await {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM _sqlx_migrations")
            .fetch_one(pool)
            .await
            .unwrap_or(0)
    } else {
        findings.push(HealthFinding {
            code: "migration.table_missing".into(),
            severity: HealthSeverity::Warning,
            area: "Migrations".into(),
            title: "Migration history table is missing".into(),
            detail: "The database can run, but schema-version diagnostics are incomplete.".into(),
            fix_action: None,
        });
        0
    };

    let hub_mode_raw = app_config(pool, "hub_mode").await.unwrap_or_default();
    let hub_url = app_config(pool, "hub_url").await.unwrap_or_default();
    let hub_mode = if hub_mode_raw == "1" {
        "hub"
    } else if !hub_url.is_empty() {
        "terminal"
    } else {
        "standalone"
    }
    .to_string();

    let mut pending_total = 0i64;
    let mut stuck_total = 0i64;
    for table in sync_tables {
        if !safe_table_name(table) || !table_exists(pool, table).await {
            continue;
        }
        let pending: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM {table} WHERE sync_status='pending' AND sync_attempts < 10"
        ))
        .fetch_one(pool)
        .await
        .unwrap_or(0);
        let stuck: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM {table} WHERE sync_status='pending' AND sync_attempts >= 10"
        ))
        .fetch_one(pool)
        .await
        .unwrap_or(0);
        let max_attempts: i64 = sqlx::query_scalar(&format!(
            "SELECT COALESCE(MAX(sync_attempts),0) FROM {table}"
        ))
        .fetch_one(pool)
        .await
        .unwrap_or(0);
        pending_total += pending;
        stuck_total += stuck;
        if pending > 0 || stuck > 0 {
            tables.push(SyncHealthTable {
                table: table.to_string(),
                pending,
                stuck,
                max_attempts,
            });
        }
    }
    if stuck_total > 0 {
        findings.push(HealthFinding {
            code: "sync.stuck_rows".into(),
            severity: HealthSeverity::Warning,
            area: "Sync".into(),
            title: format!("{stuck_total} sync row(s) are stuck"),
            detail: "Rows with 10 or more sync attempts will not retry until their attempt counters are reset.".into(),
            fix_action: Some("reset_stuck_sync".into()),
        });
    }
    if pending_total > 0 {
        findings.push(HealthFinding {
            code: "sync.pending_rows".into(),
            severity: HealthSeverity::Info,
            area: "Sync".into(),
            title: format!("{pending_total} sync row(s) are pending"),
            detail: "Pending rows are waiting for the next hub sync cycle.".into(),
            fix_action: Some("trigger_sync_now".into()),
        });
    }

    if table_exists(pool, "stock_movements").await && table_exists(pool, "stock_levels").await {
        let drift = crate::inventory::movements::stock_drift_report(pool).await?;
        if !drift.is_empty() {
            findings.push(HealthFinding {
                code: "inventory.stock_drift".into(),
                severity: HealthSeverity::Warning,
                area: "Inventory".into(),
                title: format!("{} stock balance(s) differ from movement history", drift.len()),
                detail: "The movement ledger is intact, but the cached stock level needs reconciliation.".into(),
                fix_action: Some("reconcile_stock_drift".into()),
            });
        }
    }

    let stuck_ai_runs: i64 = if table_exists(pool, "ai_runs").await {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM ai_runs
             WHERE status='running' AND updated_at < datetime('now','-5 minutes')",
        )
        .fetch_one(pool)
        .await
        .unwrap_or(0)
    } else {
        0
    };
    if stuck_ai_runs > 0 {
        findings.push(HealthFinding {
            code: "ai.stuck_runs".into(),
            severity: HealthSeverity::Warning,
            area: "AI".into(),
            title: format!("{stuck_ai_runs} AI run(s) appear stuck"),
            detail: "A run has been marked running for more than five minutes without completion."
                .into(),
            fix_action: Some("clear_stuck_ai_runs".into()),
        });
    }

    let stuck_ai_actions: i64 = if table_exists(pool, "ai_actions").await {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM ai_actions
             WHERE status='executing' AND created_at < datetime('now','-10 minutes')",
        )
        .fetch_one(pool)
        .await
        .unwrap_or(0)
    } else {
        0
    };
    if stuck_ai_actions > 0 {
        findings.push(HealthFinding {
            code: "ai.stuck_actions".into(),
            severity: HealthSeverity::Warning,
            area: "AI".into(),
            title: format!("{stuck_ai_actions} AI action(s) appear stuck"),
            detail: "An action has been executing for more than ten minutes.".into(),
            fix_action: Some("clear_stuck_ai_actions".into()),
        });
    }

    let device_count: i64 = if table_exists(pool, "devices").await {
        sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE is_active=1")
            .fetch_one(pool)
            .await
            .unwrap_or(0)
    } else {
        0
    };
    if device_count == 0 {
        findings.push(HealthFinding {
            code: "devices.none_active".into(),
            severity: HealthSeverity::Critical,
            area: "Terminals".into(),
            title: "No active devices are configured".into(),
            detail: "The terminal identity is required for sales, sync, and audit trails.".into(),
            fix_action: None,
        });
    }

    let mut devices = Vec::new();
    if table_exists(pool, "devices").await {
        let rows = sqlx::query(
            "SELECT device_id, device_code, name, status, is_active
             FROM devices ORDER BY is_active DESC, device_code",
        )
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        for r in rows {
            let active: i64 = r.try_get("is_active").unwrap_or(0);
            devices.push(HealthDevice {
                device_id: r.try_get("device_id").unwrap_or_default(),
                label: format!(
                    "{} — {}",
                    r.try_get::<String, _>("device_code").unwrap_or_default(),
                    r.try_get::<String, _>("name").unwrap_or_default()
                ),
                role: if hub_mode == "hub" {
                    "known device"
                } else {
                    "local record"
                }
                .into(),
                status: if active == 1 {
                    r.try_get("status").unwrap_or_else(|_| "active".into())
                } else {
                    "inactive".into()
                },
                ip: None,
                last_seen: None,
            });
        }
    }

    let ok = db_integrity == "ok"
        && stuck_total == 0
        && stuck_ai_runs == 0
        && stuck_ai_actions == 0
        && device_count > 0;

    Ok(SystemHealthReport {
        summary: HealthSummary {
            ok,
            db_integrity,
            migration_count,
            pending_sync_rows: pending_total,
            stuck_sync_rows: stuck_total,
            device_count,
            hub_mode,
            checked_at,
        },
        findings,
        devices,
        tables,
    })
}

pub async fn apply_health_fix(
    pool: &SqlitePool,
    fix_action: &str,
    sync_tables: &[&str],
) -> AppResult<HealthFixResult> {
    let mut rows_changed = 0u64;
    match fix_action {
        "reset_stuck_sync" => {
            for table in sync_tables {
                if !safe_table_name(table) || !table_exists(pool, table).await {
                    continue;
                }
                let rows = sqlx::query(&format!(
                    "UPDATE {table}
                     SET sync_attempts=0
                     WHERE sync_status='pending' AND sync_attempts >= 10"
                ))
                .execute(pool)
                .await?
                .rows_affected();
                rows_changed += rows;
            }
            Ok(HealthFixResult {
                fix_action: fix_action.into(),
                rows_changed,
                message: format!("Reset {rows_changed} stuck sync row(s)."),
            })
        }
        "clear_stuck_ai_runs" => {
            if table_exists(pool, "ai_runs").await {
                rows_changed = sqlx::query(
                    "UPDATE ai_runs
                     SET status='failed',
                         error='Cleared by confirmed system health fix',
                         updated_at=?
                     WHERE status='running' AND updated_at < datetime('now','-5 minutes')",
                )
                .bind(chrono::Utc::now().to_rfc3339())
                .execute(pool)
                .await?
                .rows_affected();
            }
            Ok(HealthFixResult {
                fix_action: fix_action.into(),
                rows_changed,
                message: format!("Cleared {rows_changed} stuck AI run(s)."),
            })
        }
        "clear_stuck_ai_actions" => {
            if table_exists(pool, "ai_actions").await {
                rows_changed = sqlx::query(
                    "UPDATE ai_actions
                     SET status='failed',
                         error_message='Cleared by confirmed system health fix',
                         completed_at=?
                     WHERE status='executing' AND created_at < datetime('now','-10 minutes')",
                )
                .bind(chrono::Utc::now().to_rfc3339())
                .execute(pool)
                .await?
                .rows_affected();
            }
            Ok(HealthFixResult {
                fix_action: fix_action.into(),
                rows_changed,
                message: format!("Cleared {rows_changed} stuck AI action(s)."),
            })
        }
        "reconcile_stock_drift" => {
            rows_changed = crate::inventory::movements::reconcile_stock_drift(pool).await?;
            Ok(HealthFixResult {
                fix_action: fix_action.into(),
                rows_changed,
                message: format!(
                    "Reconciled {rows_changed} stock balance(s) from movement history."
                ),
            })
        }
        other => Err(AppError::Validation(format!(
            "Unknown or unsupported health fix action: {other}"
        ))),
    }
}

pub fn merge_seen_devices(report: &mut SystemHealthReport, seen: &HubSeenSnapshot) {
    for (device_id, (ip, last_seen)) in seen.iter() {
        if let Some(existing) = report
            .devices
            .iter_mut()
            .find(|d| d.device_id == *device_id)
        {
            existing.ip = Some(ip.clone());
            existing.last_seen = Some(last_seen.clone());
            existing.role = "hub terminal".into();
            existing.status = "seen on hub".into();
        } else {
            report.devices.push(HealthDevice {
                device_id: device_id.clone(),
                label: device_id.clone(),
                role: "hub terminal".into(),
                status: "seen on hub".into(),
                ip: Some(ip.clone()),
                last_seen: Some(last_seen.clone()),
            });
        }
    }
    report.summary.device_count = report.devices.len() as i64;
}

#[tauri::command]
pub async fn system_health_check(
    state: State<'_, AppState>,
    actor_user_id: String,
) -> AppResult<SystemHealthReport> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let mut report = run_local_health_check(&state.db, sync_commands::SYNC_TABLES).await?;

    let hub = state.hub.lock().await;
    if let Some(handle) = &hub.handle {
        if let Ok(seen) = handle.seen.lock() {
            merge_seen_devices(&mut report, &seen);
        }
    } else if report.summary.hub_mode == "hub" {
        report.findings.push(HealthFinding {
            code: "hub.not_running".into(),
            severity: HealthSeverity::Critical,
            area: "Hub".into(),
            title: "This device is configured as hub but the hub server is not running".into(),
            detail: hub
                .last_error
                .clone()
                .unwrap_or_else(|| "No hub runtime handle is active.".into()),
            fix_action: None,
        });
        report.summary.ok = false;
    } else if report.summary.hub_mode == "terminal" {
        let hub_url = app_config(&state.db, "hub_url").await.unwrap_or_default();
        let token = crate::secure_store::get_secret("hub_store_token").unwrap_or_default();
        let device_id = app_config(&state.db, "device_id").await.unwrap_or_default();
        if hub_url.is_empty() || token.is_empty() {
            report.findings.push(HealthFinding {
                code: "hub.credentials_missing".into(),
                severity: HealthSeverity::Critical,
                area: "Hub".into(),
                title: "Terminal is missing hub URL or store token".into(),
                detail: "This terminal is configured as a hub terminal but cannot authenticate to the hub.".into(),
                fix_action: None,
            });
            report.summary.ok = false;
        } else {
            let client = HttpSyncClient::new(&hub_url, &token, Some(&device_id));
            match client.hub_health().await {
                Ok(hub_report) => {
                    report = merge_hub_report(report, hub_report);
                }
                Err(e) => {
                    report.findings.push(HealthFinding {
                        code: "hub.health_unreachable".into(),
                        severity: HealthSeverity::Critical,
                        area: "Hub".into(),
                        title: "Could not retrieve health report from the hub".into(),
                        detail: e.to_string(),
                        fix_action: None,
                    });
                    report.summary.ok = false;
                }
            }
        }
    }
    Ok(report)
}

fn merge_hub_report(
    mut local: SystemHealthReport,
    mut hub_report: SystemHealthReport,
) -> SystemHealthReport {
    for finding in &mut hub_report.findings {
        finding.area = format!("Hub {}", finding.area);
        if finding.fix_action.is_some() {
            finding
                .detail
                .push_str(" Run this fix on the hub terminal.");
            finding.fix_action = None;
        }
    }
    local.findings.extend(hub_report.findings);
    local.tables.extend(hub_report.tables);
    for device in hub_report.devices {
        if !local
            .devices
            .iter()
            .any(|d| d.device_id == device.device_id)
        {
            local.devices.push(device);
        }
    }
    local.summary.pending_sync_rows += hub_report.summary.pending_sync_rows;
    local.summary.stuck_sync_rows += hub_report.summary.stuck_sync_rows;
    local.summary.device_count = local.devices.len() as i64;
    local.summary.ok = local.summary.ok && hub_report.summary.ok;
    local
}

#[tauri::command]
pub async fn system_health_apply_fix(
    state: State<'_, AppState>,
    input: HealthFixInput,
) -> AppResult<HealthFixResult> {
    rbac::manager_or_owner(&state.db, &input.actor_user_id).await?;
    if input.fix_action == "trigger_sync_now" {
        state.sync_worker.run_once().await;
        return Ok(HealthFixResult {
            fix_action: input.fix_action,
            rows_changed: 0,
            message: "Triggered a sync cycle.".into(),
        });
    }
    let result = apply_health_fix(&state.db, &input.fix_action, sync_commands::SYNC_TABLES).await?;
    if result.rows_changed > 0 {
        sync_commands::schedule_immediate_sync(&state);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn test_pool() -> sqlx::SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("memory db");
        sqlx::query(
            "CREATE TABLE products (
                product_id TEXT PRIMARY KEY,
                sync_status TEXT NOT NULL DEFAULT 'synced',
                sync_attempts INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .expect("products table");
        sqlx::query(
            "INSERT INTO products (product_id, sync_status, sync_attempts, created_at)
             VALUES ('P1', 'pending', 12, '2026-07-05T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .expect("stuck row");
        pool
    }

    #[tokio::test]
    async fn health_check_reports_stuck_sync_rows_with_fix_action() {
        let pool = test_pool().await;

        let report = run_local_health_check(&pool, &["products"]).await.unwrap();

        assert_eq!(report.summary.stuck_sync_rows, 1);
        assert!(report
            .findings
            .iter()
            .any(|f| f.code == "sync.stuck_rows"
                && f.fix_action.as_deref() == Some("reset_stuck_sync")));
    }

    #[tokio::test]
    async fn apply_reset_stuck_sync_fix_resets_attempts() {
        let pool = test_pool().await;

        let result = apply_health_fix(&pool, "reset_stuck_sync", &["products"])
            .await
            .unwrap();

        let attempts: i64 =
            sqlx::query_scalar("SELECT sync_attempts FROM products WHERE product_id = 'P1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(result.rows_changed, 1);
        assert_eq!(attempts, 0);
    }

    #[test]
    fn merge_seen_devices_marks_hub_connected_terminals() {
        let mut report = SystemHealthReport {
            summary: HealthSummary {
                ok: true,
                db_integrity: "ok".into(),
                migration_count: 1,
                pending_sync_rows: 0,
                stuck_sync_rows: 0,
                device_count: 0,
                hub_mode: "hub".into(),
                checked_at: "2026-07-05T00:00:00Z".into(),
            },
            findings: vec![],
            devices: vec![],
            tables: vec![],
        };
        let mut seen = HubSeenSnapshot::new();
        seen.insert(
            "DEV2".into(),
            ("192.168.1.22".into(), "2026-07-05T01:00:00Z".into()),
        );

        merge_seen_devices(&mut report, &seen);

        assert_eq!(report.summary.device_count, 1);
        assert_eq!(report.devices[0].device_id, "DEV2");
        assert_eq!(report.devices[0].role, "hub terminal");
        assert_eq!(report.devices[0].ip.as_deref(), Some("192.168.1.22"));
    }
}
