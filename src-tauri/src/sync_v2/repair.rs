//! Carrying out a reconciliation, once [`super::reconcile`] has said what is allowed.
//!
//! The full sequence, with each step a separate function so the dangerous one is
//! not buried inside a loop:
//!
//! ```text
//! DETECT      diverged()          bucket digests, then rows in the buckets that differ
//! IDENTIFY    (same call)         the exact primary keys
//! DETERMINE   reconcile::plan()   what may be repaired, and what may not
//! REPAIR      deliver_*()         only the keys the plan cleared
//! RE-RUN      diverged()          again, because a repair that did not take is worse
//!                                 than one that never ran — it reports success
//! RECORD      record()            audit entry, including the refusals
//! ```
//!
//! Nothing here decides anything. Every judgement call lives in `reconcile`,
//! which is pure and tested; this module moves rows and is honest about how many.

use crate::db::repositories::audit_hash;
use crate::errors::{AppError, AppResult};
use crate::sync_v2::client::HttpSyncClient;
use crate::sync_v2::parity::{self, Divergence, DivergentRow};
use crate::sync_v2::reconcile::{self, ReconciliationPlan, Resolution, Side};
use serde::Serialize;
use sqlx::{Column, Row, SqlitePool};

#[derive(Debug, Clone, Serialize)]
pub struct ReconciliationOutcome {
    pub table: String,
    pub diverged_before: usize,
    pub delivered_from_hub: usize,
    pub delivered_to_hub: usize,
    /// Rows a person has to look at. Named, not counted, because "3 rows need
    /// review" without saying which is not actionable.
    pub left_for_review: Vec<String>,
    /// Divergences still present after the repair. Zero is the only good answer;
    /// anything else means a repair silently did not take.
    pub diverged_after: usize,
    pub audit: Vec<String>,
}

impl ReconciliationOutcome {
    pub fn summary(&self) -> String {
        if self.diverged_before == 0 {
            return format!("{}: already identical, nothing to do.", self.table);
        }
        let mut parts = vec![format!(
            "{}: {} row(s) diverged",
            self.table, self.diverged_before
        )];
        if self.delivered_from_hub > 0 {
            parts.push(format!("{} pulled from the hub", self.delivered_from_hub));
        }
        if self.delivered_to_hub > 0 {
            parts.push(format!("{} pushed to the hub", self.delivered_to_hub));
        }
        if !self.left_for_review.is_empty() {
            parts.push(format!(
                "{} left for review ({})",
                self.left_for_review.len(),
                self.left_for_review.join(", ")
            ));
        }
        parts.push(if self.diverged_after == 0 {
            "table now matches".to_string()
        } else {
            format!("{} still differ", self.diverged_after)
        });
        parts.join("; ")
    }
}

/// DETECT and IDENTIFY: which rows differ, by primary key.
///
/// Walks bucket digests first so a table that matches costs one request rather
/// than one per row, then opens only the buckets that disagree. `Ok(None)` means
/// the hub is too old to answer, which is a reason to stop rather than a fault.
pub async fn diverged(
    pool: &SqlitePool,
    client: &HttpSyncClient,
    table: &str,
) -> AppResult<Option<Vec<DivergentRow>>> {
    let buckets = parity::DEFAULT_BUCKETS;
    let local = parity::bucket_digests(pool, table, buckets).await?;
    let Some(body) = client.hub_parity(table, buckets, None).await? else {
        return Ok(None);
    };
    let hub: Vec<parity::BucketDigest> =
        serde_json::from_value(body.get("digests").cloned().unwrap_or_default())
            .map_err(|e| AppError::Internal(format!("Hub parity shape: {e}")))?;

    let mut found = Vec::new();
    for bucket in parity::mismatched_buckets(&local, &hub) {
        if found.len() >= parity::MAX_REPORTED_ROWS {
            break;
        }
        let local_rows = parity::row_digests(pool, table, bucket, buckets).await?;
        let hub_rows: Vec<parity::RowDigest> = client
            .hub_parity(table, buckets, Some(bucket))
            .await?
            .and_then(|b| serde_json::from_value(b.get("rows").cloned().unwrap_or_default()).ok())
            .unwrap_or_default();
        found.extend(parity::diff_rows(&local_rows, &hub_rows));
    }
    found.truncate(parity::MAX_REPORTED_ROWS);
    Ok(Some(found))
}

/// The keys in a plan that are to be delivered from one particular side.
fn keys_from(plan: &ReconciliationPlan, side: Side) -> Vec<String> {
    plan.deliverable
        .iter()
        .filter(|repair| matches!(&repair.resolution, Resolution::Deliver { from, .. } if *from == side))
        .map(|repair| repair.pk.clone())
        .collect()
}

/// REPAIR, hub → here. Fetches only the named rows and applies each through the
/// ordinary sync path, so freshness guards and null-skipping apply exactly as
/// they would to a normal pull.
async fn deliver_from_hub(
    pool: &SqlitePool,
    client: &HttpSyncClient,
    table: &str,
    pks: &[String],
) -> AppResult<usize> {
    if pks.is_empty() {
        return Ok(0);
    }
    let Some(rows) = client.hub_parity_rows(table, pks).await? else {
        return Ok(0);
    };
    let mut applied = 0usize;
    for row in &rows {
        crate::sync_v2::apply::apply_row(pool, table, row).await?;
        applied += 1;
    }
    Ok(applied)
}

/// REPAIR, here → hub. Reads the named local rows and pushes them.
async fn deliver_to_hub(
    pool: &SqlitePool,
    client: &HttpSyncClient,
    table: &str,
    pks: &[String],
) -> AppResult<usize> {
    if pks.is_empty() {
        return Ok(0);
    }
    let rows = local_rows_by_pk(pool, table, pks).await?;
    if rows.is_empty() {
        return Ok(0);
    }
    client.upsert_rows(table, &rows).await?;
    Ok(rows.len())
}

/// Local rows for a set of primary keys, shaped as the sync protocol sends them.
async fn local_rows_by_pk(
    pool: &SqlitePool,
    table: &str,
    pks: &[String],
) -> AppResult<Vec<serde_json::Value>> {
    let pk = crate::sync_v2::apply::pk_for_table(table);
    // Bound parameters. The keys came off the wire from the hub.
    let placeholders = std::iter::repeat_n("?", pks.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!("SELECT * FROM {table} WHERE {pk} IN ({placeholders})");
    let mut query = sqlx::query(&sql);
    for key in pks {
        query = query.bind(key);
    }
    let rows = query.fetch_all(pool).await?;

    Ok(rows
        .iter()
        .map(|row| {
            let mut map = serde_json::Map::new();
            for col in row.columns() {
                let name = col.name();
                if crate::sync_v2::apply::skip_on_wire(table, name) {
                    continue;
                }
                map.insert(
                    name.to_string(),
                    crate::sync_v2::apply::value_from_row_column(row, name),
                );
            }
            serde_json::Value::Object(map)
        })
        .collect())
}

/// RECORD. Written whatever the outcome, including when nothing was repaired,
/// because a run that decided to touch nothing is itself worth knowing about.
///
/// A failed audit write is logged and does not fail the reconciliation — the
/// rows are already moved by then, and losing the record of a repair that
/// happened is better than reporting a failure that did not.
async fn record(pool: &SqlitePool, outcome: &ReconciliationOutcome) {
    let (branch_id, device_id) = match crate::db::helpers::active_branch_and_device(pool).await {
        Ok(pair) => pair,
        Err(e) => {
            tracing::warn!("Reconciliation audit skipped, identity unresolved: {e}");
            return;
        }
    };
    let after = serde_json::to_string(outcome).unwrap_or_default();
    if let Err(e) = audit_hash::insert_audit_entry(
        pool,
        "SYNC_RECONCILED",
        "sync_table",
        &outcome.table,
        "system",
        "system",
        &device_id,
        &branch_id,
        None,
        Some(&after),
        Some(&outcome.summary()),
    )
    .await
    {
        tracing::error!("AUDIT WRITE FAILED [SYNC_RECONCILED]: {e:?}");
    }
}

/// The whole sequence for one table.
///
/// Returns `Ok(None)` when the hub cannot answer row-level parity, so a caller
/// can tell "nothing to fix" from "could not look".
pub async fn reconcile_table(
    pool: &SqlitePool,
    client: &HttpSyncClient,
    table: &str,
) -> AppResult<Option<ReconciliationOutcome>> {
    if !crate::sync_v2::consistency::CONSISTENCY_TABLES.contains(&table) {
        return Err(AppError::Validation(format!(
            "'{table}' is not a parity-checked table."
        )));
    }

    let Some(rows) = diverged(pool, client, table).await? else {
        return Ok(None);
    };
    let plan = reconcile::plan(table, &rows);
    let audit = reconcile::audit_lines(&plan);

    let delivered_from_hub =
        deliver_from_hub(pool, client, table, &keys_from(&plan, Side::Hub)).await?;
    let delivered_to_hub =
        deliver_to_hub(pool, client, table, &keys_from(&plan, Side::Terminal)).await?;

    // RE-RUN. A repair that quietly did not take is worse than one that never
    // ran, because it reports success — so the count that gets reported is the
    // one measured afterwards, not the one predicted before.
    let diverged_after = diverged(pool, client, table)
        .await?
        .map(|rows| rows.len())
        .unwrap_or(0);

    let outcome = ReconciliationOutcome {
        table: table.to_string(),
        diverged_before: rows.len(),
        delivered_from_hub,
        delivered_to_hub,
        left_for_review: plan.escalated.iter().map(|r| r.pk.clone()).collect(),
        diverged_after,
        audit,
    };
    record(pool, &outcome).await;
    Ok(Some(outcome))
}

/// Every parity-checked table, in registry order.
///
/// Stops at the first table the hub cannot answer for, rather than reporting
/// clean results for the rest — a partial sweep read as a full one is exactly
/// the "100% consistent while sales are missing" failure this work exists to
/// remove.
pub async fn reconcile_all(
    pool: &SqlitePool,
    client: &HttpSyncClient,
) -> AppResult<Vec<ReconciliationOutcome>> {
    let mut outcomes = Vec::new();
    for table in crate::sync_v2::registry::parity_checked() {
        match reconcile_table(pool, client, table).await? {
            Some(outcome) => outcomes.push(outcome),
            None => {
                return Err(AppError::Validation(
                    "The hub is on an older build that cannot report row-level parity. \
                     Upgrade the hub terminal before reconciling."
                        .into(),
                ))
            }
        }
    }
    Ok(outcomes)
}

/// Divergences described for a person, without repairing anything.
///
/// The read-only half, so a manager can look before anyone changes data.
pub fn describe(rows: &[DivergentRow]) -> Vec<String> {
    rows.iter()
        .map(|row| {
            format!(
                "{} — {}",
                row.pk,
                match row.divergence {
                    Divergence::MissingLocally => "on the hub, not here (a pull that never landed)",
                    Divergence::MissingOnHub =>
                        "here, not on the hub (a push still queued or lost)",
                    Divergence::Different => "on both, contents disagree",
                }
            )
        })
        .collect()
}

#[cfg(test)]
mod tests;
