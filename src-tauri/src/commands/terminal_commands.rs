//! What the Command Center shows about the other tills, and what it may do.
//!
//! Two things that belong together because they answer one question in
//! sequence: which terminals are there and are they keeping up, then — when one
//! is not — what exactly differs and what may safely be repaired.
//!
//! The split between preview and execute is deliberate and is not a
//! confirmation dialog. `reconciliation_preview` writes nothing and can be run
//! by anyone who can read sync state; `reconciliation_run` moves rows and is
//! manager-only. A reconciliation that finds a financial conflict refuses it in
//! both, because whichever copy loses may be the only record that a customer
//! handed over money — see `sync_v2::reconcile`.

use crate::commands::rbac;
use crate::db::helpers::active_branch_id;
use crate::errors::{AppError, AppResult};
use crate::sync_v2::repair::{self, ReconciliationOutcome};
use crate::AppState;
use serde::Serialize;
use tauri::State;

pub use crate::commands::device_state::TerminalRow;

/// Every terminal, with its state derived from heartbeat evidence.
///
/// Any role: a cashier who can see that the till next to them stopped checking
/// in an hour ago is a cashier who can tell somebody, and that is the whole
/// value of the screen.
#[tauri::command]
pub async fn terminal_roster(
    session_token: String,
    state: State<'_, AppState>,
) -> Result<Vec<TerminalRow>, AppError> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE).await?;
    let hub_url: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_config WHERE key = 'hub_url' AND TRIM(value) <> ''",
    )
    .fetch_optional(&state.db)
    .await?;
    if hub_url.is_some() {
        let client = state.sync_worker.load_client().await.ok_or_else(|| {
            AppError::Validation(
                "Hub credentials are unavailable; reconnect this terminal in Settings → Hub".into(),
            )
        })?;
        return client.terminal_roster().await;
    }
    crate::commands::device_state::roster(&state.db).await
}

/// What differs from the hub for one table, and what a repair would do about it.
#[derive(Debug, Serialize)]
pub struct ReconciliationPreview {
    pub table: String,
    pub diverged: usize,
    /// Rows that can be delivered without anyone deciding — one side simply
    /// does not have them, so nothing is overwritten.
    pub deliverable: Vec<String>,
    /// Rows both sides hold with different contents. Named, not counted,
    /// because "3 rows need review" gives nobody anything to look at.
    pub needs_review: Vec<String>,
    /// One line per decision, including the refusals.
    pub audit: Vec<String>,
    /// True when the hub is too old to answer row-level parity — which is a
    /// reason to stop rather than a fault to report as a clean result.
    pub hub_too_old: bool,
}

async fn hub_client(state: &AppState) -> AppResult<crate::sync_v2::client::HttpSyncClient> {
    state
        .sync_worker
        .load_client()
        .await
        .ok_or_else(|| AppError::Validation("This terminal has no hub configured.".into()))
}

/// Look, and change nothing.
#[tauri::command]
pub async fn reconciliation_preview(
    session_token: String,
    table: String,
    state: State<'_, AppState>,
) -> Result<ReconciliationPreview, AppError> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE).await?;
    let client = hub_client(&state).await?;

    let Some(rows) = repair::diverged(&state.db, &client, &table).await? else {
        return Ok(ReconciliationPreview {
            table,
            diverged: 0,
            deliverable: Vec::new(),
            needs_review: Vec::new(),
            audit: Vec::new(),
            hub_too_old: true,
        });
    };

    let plan = crate::sync_v2::reconcile::plan(&table, &rows);
    Ok(ReconciliationPreview {
        diverged: rows.len(),
        deliverable: plan.deliverable.iter().map(|r| r.pk.clone()).collect(),
        needs_review: plan.escalated.iter().map(|r| r.pk.clone()).collect(),
        audit: crate::sync_v2::reconcile::audit_lines(&plan),
        hub_too_old: false,
        table,
    })
}

/// Repair what may be repaired, and report what was refused.
///
/// Manager-only: it moves rows between this terminal and the hub. It cannot
/// overwrite a contested financial row whatever the caller's role — that
/// refusal lives in `reconcile::resolve`, not here — but deciding to run a
/// repair at all is a judgement, and an audit entry is written naming who made
/// it.
#[tauri::command]
pub async fn reconciliation_run(
    session_token: String,
    table: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<ReconciliationOutcome>, AppError> {
    let actor = rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;
    let client = hub_client(&state).await?;

    let outcomes = match table {
        Some(table) => repair::reconcile_table(&state.db, &client, &table)
            .await?
            .map(|outcome| vec![outcome])
            .unwrap_or_default(),
        None => repair::reconcile_all(&state.db, &client).await?,
    };

    // Who ran it, not just what it did. `repair` already records the per-table
    // decisions; this names the person, which those cannot.
    let branch_id = active_branch_id(&state.db).await.unwrap_or_default();
    let device_id = crate::db::helpers::active_device_id(&state.db)
        .await
        .unwrap_or_default();
    let summary = outcomes
        .iter()
        .map(ReconciliationOutcome::summary)
        .collect::<Vec<_>>()
        .join(" | ");
    if let Err(error) = crate::db::repositories::audit_hash::insert_audit_entry(
        &state.db,
        "SYNC_RECONCILE_REQUESTED",
        "sync",
        "reconciliation",
        &actor.user_id,
        "user",
        &device_id,
        &branch_id,
        None,
        Some(&serde_json::to_string(&outcomes).unwrap_or_default()),
        Some(&summary),
    )
    .await
    {
        tracing::error!("AUDIT WRITE FAILED [SYNC_RECONCILE_REQUESTED]: {error:?}");
    }

    Ok(outcomes)
}

#[cfg(test)]
mod tests;
