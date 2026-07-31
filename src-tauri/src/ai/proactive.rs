#![allow(dead_code)]
use crate::db::repositories::{
    ai_admin_repo,
    proactive_repo::{self, ProactiveAlert},
};
use rand::Rng;
use sqlx::{Row, SqlitePool};
use tauri::Emitter;
use ulid::Ulid;

const DETECTION_BASE_SECS: u64 = 300;
const DETECTION_MAX_SECS: u64 = 3_600;
const DETECTION_JITTER_MAX_SECS: u64 = 30;
const CHAT_MAINTENANCE_INITIAL_DELAY_SECS: u64 = 600;
const CHAT_MAINTENANCE_INTERVAL_SECS: u64 = 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetectionAction {
    Run,
}

fn initial_detection_action() -> DetectionAction {
    DetectionAction::Run
}

fn detection_delay_secs(consecutive_failures: u32, jitter_secs: u64) -> u64 {
    let exponent = consecutive_failures.saturating_sub(1).min(4);
    DETECTION_BASE_SECS
        .saturating_mul(1_u64 << exponent)
        .min(DETECTION_MAX_SECS)
        .saturating_add(jitter_secs.min(DETECTION_JITTER_MAX_SECS))
}

struct NewAlert {
    alert_type: &'static str,
    severity: &'static str,
    title: String,
    description: String,
    detail_json: Option<String>,
}

struct DetectionThresholds {
    expiry_lead_days: i64,            // default: 7
    margin_erosion_basis_points: i64, // default: 500
    dead_stock_days: i64,             // default: 90
    cash_discrepancy_minor: i64,      // default: 500 (0.500 BHD)
    sales_drop_pct: f64,              // default: 0.5 (50% drop)
    refund_spike_multiplier: f64,     // default: 2.0 (2x average)
    refund_spike_min_today: i64,      // default: 2
    overstock_days: f64,              // default: 90.0
    shift_too_long_hours: i64,        // default: 12
    sync_stuck_failures: i64,         // default: 3
    high_discount_pct: f64,           // default: 20.0
}

async fn load_thresholds(pool: &SqlitePool) -> DetectionThresholds {
    async fn cfg_i64(pool: &SqlitePool, key: &str, default: i64) -> i64 {
        sqlx::query_scalar::<_, String>("SELECT value FROM app_config WHERE key=?")
            .bind(key)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }
    async fn cfg_f64(pool: &SqlitePool, key: &str, default: f64) -> f64 {
        sqlx::query_scalar::<_, String>("SELECT value FROM app_config WHERE key=?")
            .bind(key)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }
    DetectionThresholds {
        expiry_lead_days: cfg_i64(pool, "alert_expiry_lead_days", 7)
            .await
            .clamp(0, 365),
        margin_erosion_basis_points: cfg_i64(pool, "alert_margin_erosion_bps", 500)
            .await
            .clamp(1, 10_000),
        dead_stock_days: cfg_i64(pool, "alert_dead_stock_days", 90)
            .await
            .clamp(30, 730),
        cash_discrepancy_minor: cfg_i64(pool, "alert_cash_discrepancy_minor", 500).await,
        sales_drop_pct: cfg_f64(pool, "alert_sales_drop_pct", 0.5).await,
        refund_spike_multiplier: cfg_f64(pool, "alert_refund_spike_multiplier", 2.0).await,
        refund_spike_min_today: cfg_i64(pool, "alert_refund_spike_min_today", 2).await,
        overstock_days: cfg_f64(pool, "alert_overstock_days", 90.0).await,
        shift_too_long_hours: cfg_i64(pool, "alert_shift_too_long_hours", 12).await,
        sync_stuck_failures: cfg_i64(pool, "alert_sync_stuck_failures", 3).await,
        high_discount_pct: cfg_f64(pool, "alert_high_discount_pct", 20.0).await,
    }
}

// P0-05: webhook escalation for critical alerts
async fn escalate_critical(pool: &SqlitePool, alert: &NewAlert) {
    let webhook_url: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key='alert_webhook_url'")
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();

    let Some(url) = webhook_url else { return };
    if url.is_empty() {
        return;
    }

    let payload = serde_json::json!({
        "alert_type": alert.alert_type,
        "severity": alert.severity,
        "title": alert.title,
        "description": alert.description,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    if let Err(e) = client.post(&url).json(&payload).send().await {
        tracing::warn!("Critical alert webhook delivery failed: {e}");
    } else {
        tracing::info!("Critical alert escalated via webhook: {}", alert.alert_type);
    }
}

pub async fn run_detection_loop(app_handle: tauri::AppHandle, pool: SqlitePool) {
    let mut consecutive_failures = 0_u32;
    let DetectionAction::Run = initial_detection_action();
    loop {
        match run_once(&app_handle, &pool).await {
            Ok(()) => consecutive_failures = 0,
            Err(error) => {
                consecutive_failures = consecutive_failures.saturating_add(1);
                tracing::warn!(consecutive_failures, "Proactive detection error: {error}");
                let detail = serde_json::json!({
                    "consecutive_failures": consecutive_failures,
                })
                .to_string();
                let _ = crate::diagnostics::record(
                    &pool,
                    "warning",
                    "ai_proactive_detection",
                    &error.to_string(),
                    None,
                    Some(&detail),
                )
                .await;
            }
        }
        let jitter = rand::thread_rng().gen_range(0..=DETECTION_JITTER_MAX_SECS);
        let delay = detection_delay_secs(consecutive_failures, jitter);
        tokio::time::sleep(tokio::time::Duration::from_secs(delay)).await;
    }
}

pub async fn run_chat_maintenance_loop(pool: SqlitePool) {
    tokio::time::sleep(tokio::time::Duration::from_secs(
        CHAT_MAINTENANCE_INITIAL_DELAY_SECS,
    ))
    .await;
    loop {
        if let Err(error) = ai_admin_repo::cleanup_old_messages(&pool, 30).await {
            tracing::warn!("AI chat retention maintenance failed: {error}");
            let _ = crate::diagnostics::record(
                &pool,
                "warning",
                "ai_chat_retention",
                &error.to_string(),
                None,
                None,
            )
            .await;
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(
            CHAT_MAINTENANCE_INTERVAL_SECS,
        ))
        .await;
    }
}

async fn run_once(
    app_handle: &tauri::AppHandle,
    pool: &SqlitePool,
) -> crate::errors::AppResult<()> {
    let branch_id: String =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key='branch_id'")
            .fetch_optional(pool)
            .await?
            .unwrap_or_else(|| "default".to_string());

    let thresholds = load_thresholds(pool).await;
    let now = chrono::Utc::now().to_rfc3339();
    let mut all_alerts: Vec<NewAlert> = Vec::new();

    all_alerts.extend(rule_stock_out(pool, &branch_id).await.unwrap_or_default());
    all_alerts.extend(rule_low_stock(pool, &branch_id).await.unwrap_or_default());
    all_alerts.extend(
        rule_near_expiry(pool, &branch_id, thresholds.expiry_lead_days)
            .await
            .unwrap_or_default(),
    );
    all_alerts.extend(
        rule_refund_spike(pool, &thresholds)
            .await
            .unwrap_or_default(),
    );
    all_alerts.extend(
        rule_cash_discrepancy(pool, &thresholds)
            .await
            .unwrap_or_default(),
    );
    all_alerts.extend(rule_sales_drop(pool, &thresholds).await.unwrap_or_default());
    all_alerts.extend(
        rule_overstock(pool, &branch_id, &thresholds)
            .await
            .unwrap_or_default(),
    );
    all_alerts.extend(
        rule_shift_too_long(pool, &thresholds)
            .await
            .unwrap_or_default(),
    );
    all_alerts.extend(rule_sync_stuck(pool, &thresholds).await.unwrap_or_default());
    all_alerts.extend(
        rule_high_discounts(pool, &thresholds)
            .await
            .unwrap_or_default(),
    );
    all_alerts.extend(rule_negative_margin(pool).await.unwrap_or_default());
    all_alerts.extend(
        rule_margin_erosion(pool, &branch_id, thresholds.margin_erosion_basis_points)
            .await
            .unwrap_or_default(),
    );
    all_alerts.extend(
        rule_dead_stock(pool, &branch_id, thresholds.dead_stock_days)
            .await
            .unwrap_or_default(),
    );

    let mut persisted = 0usize;
    for alert in all_alerts {
        // Persist watermark so we know this rule last ran at `now`.
        let _ = proactive_repo::set_watermark(pool, alert.alert_type, &now).await;

        if proactive_repo::has_active(pool, &branch_id, alert.alert_type)
            .await
            .unwrap_or(false)
        {
            // Condition already alerted — update title/description/detail if it worsened.
            let _ = proactive_repo::update_alert_details(
                pool,
                &branch_id,
                alert.alert_type,
                &alert.title,
                &alert.description,
                alert.detail_json.as_deref(),
                &now,
            )
            .await;
            continue;
        }
        let is_critical = alert.severity == "critical";
        let pa = ProactiveAlert {
            alert_id: Ulid::new().to_string(),
            branch_id: branch_id.clone(),
            alert_type: alert.alert_type.to_string(),
            severity: alert.severity.to_string(),
            title: alert.title.clone(),
            description: alert.description.clone(),
            detail_json: alert.detail_json.clone(),
            detected_at: now.clone(),
            dismissed_at: None,
            dismissed_by_user_id: None,
            created_at: now.clone(),
        };
        if proactive_repo::insert_alert(pool, &pa).await.is_ok() {
            persisted += 1;
            if is_critical {
                escalate_critical(pool, &alert).await;
            }
        }
    }

    if persisted > 0 {
        let alerts = proactive_repo::list_undismissed(pool, &branch_id)
            .await
            .unwrap_or_default();
        let domain_alerts: Vec<crate::domain::ai_admin::ProactiveAlert> = alerts
            .into_iter()
            .map(|a| crate::domain::ai_admin::ProactiveAlert {
                alert_id: a.alert_id,
                branch_id: a.branch_id,
                alert_type: a.alert_type,
                severity: a.severity,
                title: a.title,
                description: a.description,
                detail_json: a.detail_json,
                detected_at: a.detected_at,
                dismissed_at: a.dismissed_at,
                dismissed_by_user_id: a.dismissed_by_user_id,
                created_at: a.created_at,
            })
            .collect();
        let _ = app_handle.emit("proactive-alerts", &domain_alerts);
    }

    Ok(())
}

async fn rule_near_expiry(
    pool: &SqlitePool,
    branch_id: &str,
    lead_days: i64,
) -> crate::errors::AppResult<Vec<NewAlert>> {
    let lots = crate::inventory::lots::expiring_lots(pool, branch_id, lead_days).await?;
    if lots.is_empty() {
        return Ok(Vec::new());
    }
    let urgent = lots.iter().any(|lot| lot.days_until_expiry <= 2);
    let title = if urgent {
        format!("{} stock lot(s) expired or expiring urgently", lots.len())
    } else {
        format!("{} stock lot(s) nearing expiry", lots.len())
    };
    Ok(vec![NewAlert {
        alert_type: "near_expiry",
        severity: if urgent { "critical" } else { "warning" },
        title,
        description: format!(
            "Review FEFO placement, supplier returns, or a confirmed clearance markdown within {lead_days} days."
        ),
        detail_json: Some(
            serde_json::to_string(&lots)
                .map_err(|error| crate::errors::AppError::Internal(error.to_string()))?,
        ),
    }])
}

async fn rule_margin_erosion(
    pool: &SqlitePool,
    branch_id: &str,
    threshold_basis_points: i64,
) -> crate::errors::AppResult<Vec<NewAlert>> {
    let report =
        crate::ai::business_insights::margin_erosion(pool, branch_id, threshold_basis_points)
            .await?;
    if report.rows.is_empty() && report.unknown_cost_line_count == 0 {
        return Ok(Vec::new());
    }
    let severity = if report
        .rows
        .iter()
        .any(|row| row.current_margin_basis_points <= 0)
    {
        "critical"
    } else {
        "warning"
    };
    Ok(vec![NewAlert {
        alert_type: "margin_erosion",
        severity,
        title: format!(
            "{} product(s) lost margin after supplier cost changes",
            report.rows.len()
        ),
        description: format!(
            "{} recent sale line(s) have unknown cost; margin totals are incomplete while this is above zero.",
            report.unknown_cost_line_count
        ),
        detail_json: Some(
            serde_json::to_string(&report)
                .map_err(|error| crate::errors::AppError::Internal(error.to_string()))?,
        ),
    }])
}

async fn rule_dead_stock(
    pool: &SqlitePool,
    branch_id: &str,
    days: i64,
) -> crate::errors::AppResult<Vec<NewAlert>> {
    let row = sqlx::query(
        "SELECT COUNT(*) AS product_count,
                COALESCE(SUM(
                  CAST(sl.quantity_on_hand AS REAL) * COALESCE(p.cost_minor, 0)
                ), 0) AS value_minor
         FROM products p
         JOIN stock_levels sl ON sl.product_id = p.product_id AND sl.branch_id = ?
         WHERE p.is_active = 1 AND p.deleted_at IS NULL
           AND CAST(sl.quantity_on_hand AS REAL) > 0
           AND NOT EXISTS (
             SELECT 1 FROM sale_items si JOIN sales s ON s.sale_id = si.sale_id
             WHERE si.product_id = p.product_id AND si.voided = 0
               AND s.branch_id = ?
               AND date(s.business_date) >= date('now', '-' || ? || ' days')
           )",
    )
    .bind(branch_id)
    .bind(branch_id)
    .bind(days)
    .fetch_one(pool)
    .await?;
    let product_count: i64 = row.get("product_count");
    if product_count == 0 {
        return Ok(Vec::new());
    }
    let value_minor: f64 = row.get("value_minor");
    Ok(vec![NewAlert {
        alert_type: "dead_stock",
        severity: "warning",
        title: format!("{product_count} stocked product(s) had no sales for {days} days"),
        description: format!(
            "Approximate cost value tied up: {:.0} minor units. Review a confirmed clearance, supplier return, or archive plan.",
            value_minor
        ),
        detail_json: Some(
            serde_json::json!({
                "days": days,
                "product_count": product_count,
                "value_minor": value_minor,
            })
            .to_string(),
        ),
    }])
}

async fn rule_stock_out(
    pool: &SqlitePool,
    branch_id: &str,
) -> crate::errors::AppResult<Vec<NewAlert>> {
    let rows = sqlx::query(
        "SELECT p.name FROM products p
         LEFT JOIN stock_levels sl ON sl.product_id=p.product_id AND sl.branch_id=?
         WHERE p.is_active=1 AND p.track_inventory=1 AND p.deleted_at IS NULL
         AND CAST(COALESCE(sl.quantity_on_hand,'0') AS REAL)=0 LIMIT 20",
    )
    .bind(branch_id)
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(vec![]);
    }
    let names: Vec<String> = rows.iter().map(|r| r.get::<String, _>("name")).collect();
    let detail = serde_json::json!({ "products": names });
    Ok(vec![NewAlert {
        alert_type: "stock_out",
        severity: "critical",
        title: format!("{} product(s) out of stock", names.len()),
        description: names.join(", "),
        detail_json: Some(detail.to_string()),
    }])
}

async fn rule_low_stock(
    pool: &SqlitePool,
    branch_id: &str,
) -> crate::errors::AppResult<Vec<NewAlert>> {
    let rows = sqlx::query(
        "SELECT p.name, CAST(COALESCE(sl.quantity_on_hand,'0') AS REAL) AS qty, p.reorder_point
         FROM products p
         LEFT JOIN stock_levels sl ON sl.product_id=p.product_id AND sl.branch_id=?
         WHERE p.is_active=1 AND p.track_inventory=1 AND p.deleted_at IS NULL
         AND CAST(COALESCE(sl.quantity_on_hand,'0') AS REAL) > 0
         AND CAST(COALESCE(sl.quantity_on_hand,'0') AS REAL) <= p.reorder_point
         LIMIT 50",
    )
    .bind(branch_id)
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(vec![]);
    }
    let count = rows.len();
    let items: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "name": r.get::<String, _>("name"),
                "qty": r.get::<f64, _>("qty"),
                "reorder_point": r.get::<f64, _>("reorder_point"),
            })
        })
        .collect();
    let detail = serde_json::json!({ "products": items });
    Ok(vec![NewAlert {
        alert_type: "low_stock",
        severity: "warning",
        title: format!("{count} product(s) below reorder point"),
        description: rows
            .iter()
            .map(|r| r.get::<String, _>("name"))
            .collect::<Vec<_>>()
            .join(", "),
        detail_json: Some(detail.to_string()),
    }])
}

async fn rule_refund_spike(
    pool: &SqlitePool,
    thresholds: &DetectionThresholds,
) -> crate::errors::AppResult<Vec<NewAlert>> {
    let today: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sales WHERE sale_status='refunded' AND date(sold_at)=date('now')",
    )
    .fetch_one(pool)
    .await?;
    let avg: f64 = sqlx::query_scalar::<_, f64>(
        "SELECT COALESCE(AVG(cnt),0) FROM (
            SELECT COUNT(*) AS cnt FROM sales
            WHERE sale_status='refunded'
            AND sold_at >= datetime('now','-7 days') AND date(sold_at)<date('now')
            GROUP BY date(sold_at)
         )",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0.0);
    if today as f64 > avg * thresholds.refund_spike_multiplier
        && today > thresholds.refund_spike_min_today
    {
        let detail = serde_json::json!({
            "today": today,
            "avg_7day": avg,
            "multiplier": thresholds.refund_spike_multiplier,
        });
        Ok(vec![NewAlert {
            alert_type: "refund_spike",
            severity: "critical",
            title: "Refund Spike Detected".to_string(),
            description: format!("{today} refunds today vs {avg:.1} daily average (7-day)"),
            detail_json: Some(detail.to_string()),
        }])
    } else {
        Ok(vec![])
    }
}

async fn rule_cash_discrepancy(
    pool: &SqlitePool,
    thresholds: &DetectionThresholds,
) -> crate::errors::AppResult<Vec<NewAlert>> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM shifts WHERE date(started_at)=date('now')
         AND cash_difference_minor IS NOT NULL AND ABS(cash_difference_minor) > ?",
    )
    .bind(thresholds.cash_discrepancy_minor)
    .fetch_one(pool)
    .await?;
    if count > 0 {
        let threshold_bhd = thresholds.cash_discrepancy_minor as f64 / 1000.0;
        let detail = serde_json::json!({
            "count": count,
            "threshold_fils": thresholds.cash_discrepancy_minor,
            "threshold_bhd": threshold_bhd,
        });
        Ok(vec![NewAlert {
            alert_type: "cash_discrepancy",
            severity: "critical",
            title: format!("{count} cash discrepancy in today's shifts"),
            description: format!(
                "One or more shifts have cash variance exceeding {threshold_bhd:.3} BHD."
            ),
            detail_json: Some(detail.to_string()),
        }])
    } else {
        Ok(vec![])
    }
}

async fn rule_sales_drop(
    pool: &SqlitePool,
    thresholds: &DetectionThresholds,
) -> crate::errors::AppResult<Vec<NewAlert>> {
    let today: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(total_minor),0) FROM sales WHERE sale_status='completed' AND date(sold_at)=date('now')",
    )
    .fetch_one(pool)
    .await?;
    let last_week: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(total_minor),0) FROM sales WHERE sale_status='completed' AND date(sold_at)=date('now','-7 days')",
    )
    .fetch_one(pool)
    .await?;
    if last_week > 1000 && (today as f64) < (last_week as f64) * thresholds.sales_drop_pct {
        let pct = (thresholds.sales_drop_pct * 100.0) as u64;
        let detail = serde_json::json!({
            "today_fils": today,
            "last_week_fils": last_week,
            "drop_ratio": (today as f64) / (last_week as f64),
            "threshold_pct": pct,
        });
        Ok(vec![NewAlert {
            alert_type: "sales_drop",
            severity: "warning",
            title: "Sales Drop Detected".to_string(),
            description: format!(
                "Today's revenue is less than {pct}% of same day last week ({today} vs {last_week} fils)"
            ),
            detail_json: Some(detail.to_string()),
        }])
    } else {
        Ok(vec![])
    }
}

async fn rule_overstock(
    pool: &SqlitePool,
    branch_id: &str,
    thresholds: &DetectionThresholds,
) -> crate::errors::AppResult<Vec<NewAlert>> {
    let rows = sqlx::query(
        "SELECT p.name,
                CAST(COALESCE(sl.quantity_on_hand,'0') AS REAL) AS qty,
                COALESCE(v.daily_rate, 0.0) AS daily_rate
         FROM products p
         LEFT JOIN stock_levels sl ON sl.product_id=p.product_id AND sl.branch_id=?
         LEFT JOIN (
             SELECT si.product_id, SUM(si.quantity)/14.0 AS daily_rate
             FROM sale_items si JOIN sales s ON s.sale_id=si.sale_id
             WHERE s.sale_status='completed' AND s.sold_at >= datetime('now','-14 days')
             GROUP BY si.product_id
         ) v ON v.product_id=p.product_id
         WHERE p.is_active=1 AND p.deleted_at IS NULL AND v.daily_rate > 0
         AND CAST(COALESCE(sl.quantity_on_hand,'0') AS REAL) / v.daily_rate > ?
         LIMIT 20",
    )
    .bind(branch_id)
    .bind(thresholds.overstock_days)
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(vec![]);
    }
    let count = rows.len();
    let days = thresholds.overstock_days as u64;
    let items: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "name": r.get::<String, _>("name"),
                "qty": r.get::<f64, _>("qty"),
                "daily_rate": r.get::<f64, _>("daily_rate"),
                "days_supply": r.get::<f64, _>("qty") / r.get::<f64, _>("daily_rate").max(0.001),
            })
        })
        .collect();
    let detail = serde_json::json!({ "products": items });
    Ok(vec![NewAlert {
        alert_type: "overstock",
        severity: "info",
        title: format!("{count} product(s) overstocked (>{days} day supply)"),
        description: rows
            .iter()
            .map(|r| r.get::<String, _>("name"))
            .take(5)
            .collect::<Vec<_>>()
            .join(", "),
        detail_json: Some(detail.to_string()),
    }])
}

async fn rule_shift_too_long(
    pool: &SqlitePool,
    thresholds: &DetectionThresholds,
) -> crate::errors::AppResult<Vec<NewAlert>> {
    let hours = thresholds.shift_too_long_hours;
    let interval = format!("-{hours} hours");
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM shifts WHERE ended_at IS NULL AND started_at < datetime('now', ?)",
    )
    .bind(&interval)
    .fetch_one(pool)
    .await?;
    if count > 0 {
        let detail = serde_json::json!({
            "count": count,
            "threshold_hours": hours,
        });
        Ok(vec![NewAlert {
            alert_type: "shift_too_long",
            severity: "warning",
            title: format!("{count} shift(s) open for more than {hours} hours"),
            description: format!(
                "One or more shifts have been open longer than {hours} hours. Please verify."
            ),
            detail_json: Some(detail.to_string()),
        }])
    } else {
        Ok(vec![])
    }
}

async fn rule_sync_stuck(
    pool: &SqlitePool,
    thresholds: &DetectionThresholds,
) -> crate::errors::AppResult<Vec<NewAlert>> {
    let failures: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key='sync_consecutive_failures'")
            .fetch_optional(pool)
            .await?;
    let count: i64 = failures.and_then(|s| s.parse().ok()).unwrap_or(0);
    if count >= thresholds.sync_stuck_failures {
        let detail = serde_json::json!({
            "consecutive_failures": count,
            "threshold": thresholds.sync_stuck_failures,
        });
        Ok(vec![NewAlert {
            alert_type: "sync_stuck",
            severity: "critical",
            title: "Sync Failing".to_string(),
            description: format!(
                "Cloud sync has failed {count} consecutive times. Check your connection and Supabase credentials."
            ),
            detail_json: Some(detail.to_string()),
        }])
    } else {
        Ok(vec![])
    }
}

async fn rule_high_discounts(
    pool: &SqlitePool,
    thresholds: &DetectionThresholds,
) -> crate::errors::AppResult<Vec<NewAlert>> {
    let rows = sqlx::query(
        "SELECT u.name AS cashier_name,
                SUM(si.line_discount_minor) * 100.0 / NULLIF(SUM(si.line_total_minor + si.line_discount_minor),0) AS discount_rate
         FROM sale_items si
         JOIN sales s ON s.sale_id=si.sale_id
         JOIN users u ON u.user_id=s.user_id
         WHERE s.sale_status='completed' AND date(s.sold_at)=date('now')
         GROUP BY s.user_id
         HAVING discount_rate > ?",
    )
    .bind(thresholds.high_discount_pct)
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(vec![]);
    }
    let names: Vec<String> = rows
        .iter()
        .map(|r| r.get::<String, _>("cashier_name"))
        .collect();
    let pct = thresholds.high_discount_pct as u64;
    let rates: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "cashier": r.get::<String, _>("cashier_name"),
                "discount_rate": r.get::<f64, _>("discount_rate"),
            })
        })
        .collect();
    let detail = serde_json::json!({
        "cashiers": rates,
        "threshold_pct": pct,
    });
    Ok(vec![NewAlert {
        alert_type: "high_discounts",
        severity: "warning",
        title: "High Discount Rate Today".to_string(),
        description: format!(
            "Cashier(s) with >{pct}% discount rate: {}",
            names.join(", ")
        ),
        detail_json: Some(detail.to_string()),
    }])
}

async fn rule_negative_margin(pool: &SqlitePool) -> crate::errors::AppResult<Vec<NewAlert>> {
    let rows = sqlx::query(
        "SELECT p.name FROM products p
         JOIN product_prices pp ON pp.product_id=p.product_id
             AND pp.price_type='selling' AND pp.effective_to IS NULL
         WHERE p.is_active=1 AND p.cost_minor IS NOT NULL
         AND p.cost_minor > pp.price_minor AND p.deleted_at IS NULL
         LIMIT 20",
    )
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(vec![]);
    }
    let names: Vec<String> = rows.iter().map(|r| r.get::<String, _>("name")).collect();
    let detail = serde_json::json!({ "products": names });
    Ok(vec![NewAlert {
        alert_type: "negative_margin",
        severity: "warning",
        title: format!("{} product(s) selling below cost", names.len()),
        description: names.join(", "),
        detail_json: Some(detail.to_string()),
    }])
}

#[cfg(test)]
mod scheduling_tests {
    use super::{detection_delay_secs, initial_detection_action, DetectionAction};

    #[test]
    fn detection_runs_before_the_first_sleep() {
        assert_eq!(initial_detection_action(), DetectionAction::Run);
    }

    #[test]
    fn detection_backoff_is_bounded_and_resets_after_success() {
        assert_eq!(detection_delay_secs(0, 0), 300);
        assert_eq!(detection_delay_secs(1, 0), 300);
        assert_eq!(detection_delay_secs(2, 0), 600);
        assert_eq!(detection_delay_secs(3, 0), 1_200);
        assert_eq!(detection_delay_secs(20, 0), 3_600);
    }

    #[test]
    fn detection_jitter_stays_within_thirty_seconds() {
        assert_eq!(detection_delay_secs(0, 0), 300);
        assert_eq!(detection_delay_secs(0, 30), 330);
        assert_eq!(detection_delay_secs(0, 31), 330);
        assert_eq!(detection_delay_secs(2, 30), 630);
    }
}
