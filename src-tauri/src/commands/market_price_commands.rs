//! Market-price commands for the product form's Market panel.
//!
//! Every one of these reads. None writes a selling price, and there is
//! deliberately no command here that could: the panel fills the price box in
//! `ProductFormModal` and the operator saves through `update_product_price`,
//! which already carries the RBAC, the confirmation and the audit trail. A
//! second path to a selling price is exactly the thing that would make a
//! competitor's number able to become ours without anybody deciding.
//!
//! Confirming a match is the one write, and it records a human judgement rather
//! than a price.

use crate::commands::rbac;
use crate::errors::{AppError, AppResult};
use crate::price_intelligence::matching::{self, Candidate};
use crate::price_intelligence::observe::{self, Observation};
use crate::price_intelligence::service::{self, MarketPriceReport, SourceStatus};
use crate::AppState;
use tauri::State;

/// What is already known, without touching the network.
#[tauri::command]
pub async fn market_price_cached(
    actor_user_id: String,
    product_id: String,
    state: State<'_, AppState>,
) -> Result<MarketPriceReport, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    service::cached_report(&state.db, &product_id).await
}

/// Go and look. Manager-only because it spends somebody else's bandwidth and
/// this shop's time, not because the answer is sensitive.
#[tauri::command]
pub async fn market_price_search(
    actor_user_id: String,
    product_id: String,
    state: State<'_, AppState>,
) -> Result<MarketPriceReport, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    service::search(&state.db, &product_id).await
}

/// Record that a candidate really is this product.
///
/// The candidate is passed back verbatim from the search that produced it, so
/// the pairing is stored against the listing the operator actually looked at
/// rather than one re-fetched in between and possibly changed.
#[tauri::command]
pub async fn market_price_confirm_match(
    actor_user_id: String,
    product_id: String,
    candidate: Candidate,
    state: State<'_, AppState>,
) -> Result<MarketPriceReport, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let branch_id = crate::db::helpers::active_branch_id(&state.db).await?;
    service::confirm_match(
        &state.db,
        &product_id,
        &branch_id,
        &actor_user_id,
        &candidate,
    )
    .await
}

/// Mark a stored pairing as wrong so it stops being offered.
#[tauri::command]
pub async fn market_price_reject_match(
    actor_user_id: String,
    match_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    matching::reject(&state.db, &match_id).await
}

/// Observations over time, trusted matches only.
#[tauri::command]
pub async fn market_price_history(
    actor_user_id: String,
    product_id: String,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<Observation>, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    observe::history(&state.db, &product_id, limit.unwrap_or(100)).await
}

/// Which sources exist and which cannot currently be used.
///
/// Reports the unsupported source explicitly rather than omitting it. A source
/// that silently returned nothing would read as "nobody else sells this", which
/// is the opposite of what it means.
#[tauri::command]
pub async fn market_source_status(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<SourceStatus>, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    service::source_statuses(&state.db).await
}

/// Add or remove a product from the refresh watchlist.
#[tauri::command]
pub async fn market_watchlist_set(
    actor_user_id: String,
    product_id: String,
    tracked: bool,
    state: State<'_, AppState>,
) -> Result<bool, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let branch_id = crate::db::helpers::active_branch_id(&state.db).await?;
    set_watchlist(&state.db, &product_id, &branch_id, &actor_user_id, tracked).await
}

pub(crate) async fn set_watchlist(
    pool: &sqlx::SqlitePool,
    product_id: &str,
    branch_id: &str,
    actor_user_id: &str,
    tracked: bool,
) -> AppResult<bool> {
    let now = chrono::Utc::now().to_rfc3339();
    if tracked {
        sqlx::query(
            "INSERT INTO price_watchlist
                (watch_id, product_id, added_by_user_id, branch_id,
                 created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(product_id) WHERE deleted_at IS NULL DO UPDATE SET
                 added_by_user_id = excluded.added_by_user_id,
                 updated_at       = excluded.updated_at,
                 sync_status      = 'pending'",
        )
        .bind(ulid::Ulid::new().to_string())
        .bind(product_id)
        .bind(actor_user_id)
        .bind(branch_id)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
    } else {
        sqlx::query("DELETE FROM price_watchlist WHERE product_id = ?")
            .bind(product_id)
            .execute(pool)
            .await?;
    }
    Ok(tracked)
}

#[cfg(test)]
mod tests;
