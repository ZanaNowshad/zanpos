use crate::errors::AppResult;
use sqlx::{Row, SqlitePool};

/// The one alert type.
///
/// This module declared its own, field-for-field identical to
/// `domain::ai_admin::ProactiveAlert`, and `ai::proactive` copied between them a
/// field at a time. Two identical structs with a hand-written mapping is a field
/// waiting to be added to one of them: the compiler is satisfied by a mapping
/// that silently omits the new column, and the alert arrives at the UI with it
/// missing. There is now one definition, and the mapping is gone with it.
pub use crate::domain::ai_admin::ProactiveAlert;

pub async fn insert_alert(pool: &SqlitePool, a: &ProactiveAlert) -> AppResult<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO proactive_alerts
         (alert_id, branch_id, alert_type, severity, title, description, detail_json, detected_at, created_at)
         VALUES (?,?,?,?,?,?,?,?,?)",
    )
    .bind(&a.alert_id)
    .bind(&a.branch_id)
    .bind(&a.alert_type)
    .bind(&a.severity)
    .bind(&a.title)
    .bind(&a.description)
    .bind(&a.detail_json)
    .bind(&a.detected_at)
    .bind(&a.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn has_active(pool: &SqlitePool, branch_id: &str, alert_type: &str) -> AppResult<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM proactive_alerts WHERE branch_id=? AND alert_type=? AND dismissed_at IS NULL",
    )
    .bind(branch_id)
    .bind(alert_type)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

pub async fn list_undismissed(
    pool: &SqlitePool,
    branch_id: &str,
) -> AppResult<Vec<ProactiveAlert>> {
    let rows = sqlx::query(
        "SELECT alert_id,branch_id,alert_type,severity,title,description,detail_json,
                detected_at,dismissed_at,dismissed_by_user_id,created_at
         FROM proactive_alerts WHERE branch_id=? AND dismissed_at IS NULL
         ORDER BY detected_at DESC LIMIT 100",
    )
    .bind(branch_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| ProactiveAlert {
            alert_id: r.get("alert_id"),
            branch_id: r.get("branch_id"),
            alert_type: r.get("alert_type"),
            severity: r.get("severity"),
            title: r.get("title"),
            description: r.get("description"),
            detail_json: r.get("detail_json"),
            detected_at: r.get("detected_at"),
            dismissed_at: r.get("dismissed_at"),
            dismissed_by_user_id: r.get("dismissed_by_user_id"),
            created_at: r.get("created_at"),
        })
        .collect())
}

pub async fn dismiss(pool: &SqlitePool, alert_id: &str, user_id: &str, now: &str) -> AppResult<()> {
    sqlx::query(
        "UPDATE proactive_alerts SET dismissed_at=?, dismissed_by_user_id=? WHERE alert_id=?",
    )
    .bind(now)
    .bind(user_id)
    .bind(alert_id)
    .execute(pool)
    .await?;
    Ok(())
}

// Retained: never called — `proactive_watermark` is written by set_watermark and read by nothing. Either wire the throttle it was meant for or drop both halves.
#[allow(dead_code)]
pub async fn get_watermark(pool: &SqlitePool, rule: &str) -> AppResult<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT last_checked FROM proactive_watermark WHERE rule_name=?")
            .bind(rule)
            .fetch_optional(pool)
            .await?,
    )
}

/// Update an existing undismissed alert's description and detail_json when the
/// underlying condition has worsened (e.g. more products out of stock).
pub async fn update_alert_details(
    pool: &SqlitePool,
    branch_id: &str,
    alert_type: &str,
    title: &str,
    description: &str,
    detail_json: Option<&str>,
    now: &str,
) -> AppResult<bool> {
    let rows = sqlx::query(
        "UPDATE proactive_alerts SET title=?, description=?, detail_json=?, detected_at=?
         WHERE branch_id=? AND alert_type=? AND dismissed_at IS NULL",
    )
    .bind(title)
    .bind(description)
    .bind(detail_json)
    .bind(now)
    .bind(branch_id)
    .bind(alert_type)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(rows > 0)
}

pub async fn set_watermark(pool: &SqlitePool, rule: &str, ts: &str) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO proactive_watermark (rule_name, last_checked) VALUES (?,?)
         ON CONFLICT(rule_name) DO UPDATE SET last_checked=excluded.last_checked",
    )
    .bind(rule)
    .bind(ts)
    .execute(pool)
    .await?;
    Ok(())
}
