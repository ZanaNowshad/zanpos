//! Asking ZanAI whether the tills actually agree with each other.
//!
//! The shop has several terminals against one hub, and the question that comes
//! up on the floor is not "is sync running" — `get_sync_status` already answers
//! that — but "why does this till show a different price to that one". The tools
//! here answer it in the order a person would ask:
//!
//! 1. `get_terminal_roster` — which tills exist and when each last checked in.
//!    A till that stopped syncing on Tuesday is the commonest cause, and it is
//!    visible without comparing anything.
//! 2. `check_terminal_parity` — which tables differ from the hub, with a score.
//! 3. `find_diverged_rows` — the actual row ids in one of them.
//!
//! Step three is the one that did not exist. `hub_truth_compare` could say
//! `products: checksum_mismatch` on a 28,010-row catalogue and the only remedy
//! on offer was Pull Hub Truth — re-downloading 28,009 correct rows to fix one.
//!
//! All read-only. Nothing here repairs anything; the existing repair tools do
//! that, and keeping the diagnosis separate is what lets a manager look before
//! anyone resyncs.

use crate::errors::{AppError, AppResult};
use crate::sync_v2::parity;
use crate::sync_v2::reconcile::ReconciliationPlan;
use serde_json::Value;
use sqlx::{Row, SqlitePool};

pub fn handles(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "check_terminal_parity"
            | "find_diverged_rows"
            | "get_terminal_roster"
            | "get_catalogue_parity_summary"
            | "preview_reconciliation"
    )
}

/// Build the hub client the same way the sync worker does.
///
/// Reconstructed from `app_config` plus the OS credential store rather than
/// borrowed from `SyncWorker`, because a read tool is handed a pool and nothing
/// else. Returning None is a normal answer — a standalone shop has no hub, and
/// that is not a fault to report.
async fn hub_client(pool: &SqlitePool) -> Option<crate::sync_v2::client::HttpSyncClient> {
    let url: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key='hub_url'")
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .filter(|u: &String| !u.trim().is_empty());
    let url = url?;
    let token = crate::secure_store::get_secret("hub_store_token").unwrap_or_default();
    if token.is_empty() {
        return None;
    }
    let device_id: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key='device_id'")
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    Some(crate::sync_v2::client::HttpSyncClient::new(
        &url,
        &token,
        device_id.as_deref(),
    ))
}

pub async fn execute(pool: &SqlitePool, tool_name: &str, input: &Value) -> AppResult<String> {
    match tool_name {
        "get_terminal_roster" => terminal_roster(pool).await,
        "check_terminal_parity" => check_parity(pool).await,
        "get_catalogue_parity_summary" => catalogue_parity(pool).await,
        "find_diverged_rows" => diverged_rows(pool, input).await,
        "preview_reconciliation" => preview_reconciliation(pool, input).await,
        other => Err(AppError::Validation(format!(
            "Unknown parity tool: {other}"
        ))),
    }
}

async fn terminal_roster(pool: &SqlitePool) -> AppResult<String> {
    use crate::commands::device_state::{device_state, seconds_since};

    // Which record this installation has claimed. Binding is a separate fact
    // from heartbeats: a terminal can be correctly bound and still never have
    // reached the hub, which is the POS-7757Z case — record present, IP and
    // last-seen empty. Conflating the two would report it as unpaired and send
    // someone to re-register a device that is already registered.
    let own_device: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key='device_id'")
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();

    let rows = sqlx::query(
        "SELECT device_id, device_code, name, is_active, last_heartbeat_at,
                observed_ip, app_version, heartbeat_seq
           FROM devices WHERE deleted_at IS NULL
          ORDER BY is_active DESC, datetime(COALESCE(last_heartbeat_at,'')) DESC",
    )
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok("No terminals are registered for this branch.".into());
    }

    let now = chrono::Utc::now();
    let mut lines = vec![format!("Terminals registered: {}", rows.len())];
    lines.push("Code | Name | State | Last seen | IP | Version".into());

    for row in &rows {
        let beat: Option<String> = row.get("last_heartbeat_at");
        let age = seconds_since(beat.as_deref(), now);
        // Bound if this installation is that device, or if it has ever beaten.
        // `devices.status` is deliberately not consulted: it was a stored
        // string nobody updated, which is why terminals that had never once
        // contacted the hub displayed as "online".
        let device_id: String = row.get("device_id");
        let paired = own_device.as_deref() == Some(device_id.as_str())
            || row.get::<i64, _>("heartbeat_seq") > 0
            || beat.is_some();
        let state = device_state(paired, age);

        let seen = match age {
            None => "never".to_string(),
            Some(seconds) if seconds < 90 => "just now".to_string(),
            Some(seconds) if seconds < 3_600 => format!("{}m ago", seconds / 60),
            Some(seconds) if seconds < 172_800 => format!("{}h ago", seconds / 3_600),
            Some(seconds) => format!("{}d ago", seconds / 86_400),
        };

        lines.push(format!(
            "{} | {} | {} | {} | {} | {}",
            row.get::<String, _>("device_code"),
            row.get::<String, _>("name"),
            state.as_str(),
            seen,
            row.get::<Option<String>, _>("observed_ip")
                .unwrap_or_else(|| "—".into()),
            row.get::<Option<String>, _>("app_version")
                .unwrap_or_else(|| "—".into()),
        ));
        if row.get::<i64, _>("is_active") == 0 {
            lines.push("  (deactivated in the device roster)".into());
        }
        if !matches!(state, crate::commands::device_state::DeviceState::Online) {
            lines.push(format!("  {}", state.advice()));
        }
    }
    Ok(lines.join("\n"))
}

/// Rows this terminal has knowingly set aside, as a line to prepend to any
/// health answer.
///
/// Parity compares what the two sides hold. A quarantined row is one this
/// terminal *chose* not to hold, so parity would score it as a plain difference
/// or — once the hub also sets it aside — as agreement. Neither reading is
/// honest, so the count is reported separately and first.
async fn quarantine_warning(pool: &SqlitePool) -> Option<String> {
    let count = crate::sync_v2::dead_letter::pending_count(pool).await;
    if count == 0 {
        return None;
    }
    let listed = crate::sync_v2::dead_letter::pending(pool, 10).await.ok()?;
    let mut lines = vec![format!(
        "NOT HEALTHY: {count} row(s) could not be applied and have been set aside. \
         They are stored in full and can be replayed, but this terminal is missing \
         them right now:"
    )];
    for (table, entity_id, reason) in &listed {
        lines.push(format!("  {table}/{entity_id} — {reason}"));
    }
    Some(lines.join("\n"))
}

async fn check_parity(pool: &SqlitePool) -> AppResult<String> {
    let quarantined = quarantine_warning(pool).await;
    let local = crate::sync_v2::consistency::snapshot(pool).await?;
    let Some(client) = hub_client(pool).await else {
        return Ok(
            "This terminal has no hub configured, so there is nothing to compare against. \
             A standalone till is its own source of truth."
                .into(),
        );
    };
    let hub = client.hub_consistency().await?;

    let mut lines = Vec::new();
    let mut mismatched = Vec::new();
    for local_table in &local.tables {
        let hub_table = hub.tables.iter().find(|t| t.table == local_table.table);
        let (hub_count, hub_checksum) = hub_table
            .map(|t| (t.count, t.checksum.clone()))
            .unwrap_or((0, String::new()));
        let status = if hub_table.is_none() {
            "missing on hub"
        } else if local_table.count != hub_count {
            "count differs"
        } else if local_table.checksum != hub_checksum {
            "contents differ"
        } else {
            continue;
        };
        mismatched.push(local_table.table.clone());
        lines.push(format!(
            "{} — {} (this terminal {}, hub {})",
            local_table.table, status, local_table.count, hub_count
        ));
    }

    let score =
        crate::sync_v2::consistency::consistency_score(local.tables.len(), mismatched.len());
    if mismatched.is_empty() {
        // Every table can match and the terminal still be missing rows it set
        // aside, so the quarantine line comes first and 100% is not claimed.
        if let Some(warning) = quarantined {
            return Ok(format!(
                "{warning}\n\nApart from those, all {} synced tables match the hub, \
                 schema version {}.",
                local.tables.len(),
                local.schema_version
            ));
        }
        return Ok(format!(
            "Parity 100%. All {} synced tables match the hub, schema version {}.",
            local.tables.len(),
            local.schema_version
        ));
    }
    let mut out = Vec::new();
    if let Some(warning) = quarantined {
        out.push(warning);
    }
    out.push(format!(
        "Parity {score}%. {} of {} tables differ from the hub:",
        mismatched.len(),
        local.tables.len()
    ));
    out.extend(lines);
    if local.schema_version != hub.schema_version {
        out.push(format!(
            "Schema versions differ: this terminal {} vs hub {} — upgrade before comparing rows.",
            local.schema_version, hub.schema_version
        ));
    }
    out.push(
        "Call find_diverged_rows on one of these tables for the row IDs, rather than a full resync."
            .to_string(),
    );
    Ok(out.join("\n"))
}

async fn catalogue_parity(pool: &SqlitePool) -> AppResult<String> {
    let mut lines = vec!["Catalogue parity (this terminal vs hub):".to_string()];
    let hub = hub_client(pool).await;

    for table in [
        "products",
        "product_barcodes",
        "product_prices",
        "categories",
    ] {
        let local = crate::sync_v2::consistency::table_snapshot(pool, table).await?;
        match &hub {
            None => lines.push(format!("{table}: {} rows (no hub configured)", local.count)),
            Some(client) => {
                let hub_snapshot = client.hub_consistency().await?;
                let matched = hub_snapshot.tables.iter().find(|t| t.table == table);
                let (hub_count, same) = matched
                    .map(|t| (t.count, t.checksum == local.checksum))
                    .unwrap_or((0, false));
                lines.push(format!(
                    "{table}: {} here / {} on hub — {}",
                    local.count,
                    hub_count,
                    if same { "identical" } else { "DIFFERENT" }
                ));
            }
        }
    }
    if hub.is_some() {
        lines.push(
            "Where a line reads DIFFERENT, find_diverged_rows names the individual records.".into(),
        );
    }
    Ok(lines.join("\n"))
}

/// The named table, validated, plus the divergent rows in it.
///
/// Both tools below need exactly this, and when the bucket-walk was written out
/// twice the two copies were free to drift — the same failure that produced two
/// hand-kept table lists. Detection lives in `sync_v2::repair`, which is also
/// what performs the repair, so a tool can never describe a divergence by
/// different rules than the ones used to fix it.
async fn rows_for_named_table(
    pool: &SqlitePool,
    input: &Value,
) -> AppResult<Result<(String, Vec<parity::DivergentRow>), String>> {
    let table = input
        .get("table")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if !crate::sync_v2::consistency::CONSISTENCY_TABLES.contains(&table.as_str()) {
        return Err(AppError::Validation(format!(
            "'{table}' is not a synced table. Synced tables: {}",
            crate::sync_v2::consistency::CONSISTENCY_TABLES.join(", ")
        )));
    }
    let Some(client) = hub_client(pool).await else {
        return Ok(Err(
            "No hub is configured, so there is nothing to diverge from.".into(),
        ));
    };
    match crate::sync_v2::repair::diverged(pool, &client, &table).await? {
        Some(rows) => Ok(Ok((table, rows))),
        None => Ok(Err(
            "The hub is on an older build that cannot report row-level parity yet. \
             Upgrade the hub terminal, or use check_terminal_parity for the table-level answer."
                .into(),
        )),
    }
}

async fn diverged_rows(pool: &SqlitePool, input: &Value) -> AppResult<String> {
    let (table, rows) = match rows_for_named_table(pool, input).await? {
        Ok(found) => found,
        Err(message) => return Ok(message),
    };
    if rows.is_empty() {
        return Ok(format!(
            "{table} is identical on this terminal and the hub."
        ));
    }

    let mut lines = vec![format!(
        "{table}: {} row(s) differ between this terminal and the hub.",
        rows.len()
    )];
    lines.extend(crate::sync_v2::repair::describe(&rows));
    if rows.len() >= parity::MAX_REPORTED_ROWS {
        lines.push(format!(
            "At least {} rows differ; at that point a full resync is the honest answer rather than a list.",
            parity::MAX_REPORTED_ROWS
        ));
    }
    Ok(lines.join("\n"))
}

/// What a reconciliation would do, without doing any of it.
///
/// The distinction this exists to communicate: a row only one side holds can be
/// delivered unattended, while a row both sides hold with different contents is
/// a decision. Reporting both as "3 rows differ" invites someone to resync and
/// silently overwrite whichever copy loses — which for a payment may be the only
/// record that a customer handed over money.
async fn preview_reconciliation(pool: &SqlitePool, input: &Value) -> AppResult<String> {
    use crate::sync_v2::reconcile;

    let (table, rows) = match rows_for_named_table(pool, input).await? {
        Ok(found) => found,
        Err(message) => return Ok(message),
    };
    if rows.is_empty() {
        return Ok(format!(
            "{table} already matches the hub — there is nothing to reconcile."
        ));
    }

    let plan = reconcile::plan(&table, &rows);
    let mut lines = vec![format!(
        "{table}: {} row(s) differ. Nothing has been changed — this is what a \
         reconciliation would do.",
        rows.len()
    )];

    if plan.deliverable.is_empty() {
        lines.push("Nothing can be repaired automatically.".into());
    } else {
        lines.push(format!(
            "{} row(s) can be delivered without anyone deciding, because only one \
             side holds them and no data is overwritten:",
            plan.deliverable.len()
        ));
        lines.extend(reconcile::audit_lines(&ReconciliationPlan {
            table: table.clone(),
            deliverable: plan.deliverable.clone(),
            escalated: Vec::new(),
        }));
    }

    if plan.escalated.is_empty() {
        lines.push("No row needs review.".into());
    } else {
        lines.push(format!(
            "{} row(s) must NOT be repaired automatically — both nodes hold them with \
             different contents, so repairing would discard one of two real edits:",
            plan.escalated.len()
        ));
        lines.extend(reconcile::audit_lines(&ReconciliationPlan {
            table: table.clone(),
            deliverable: Vec::new(),
            escalated: plan.escalated.clone(),
        }));
        if reconcile::is_financial(&table) {
            lines.push(
                "This is financial data. Compare the two copies against the printed \
                 receipts or the cash count before changing either."
                    .into(),
            );
        }
    }
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests;
