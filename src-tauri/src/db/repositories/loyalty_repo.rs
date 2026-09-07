//! Loyalty points, recorded as events and derived from them.
//!
//! The counter this replaces was written with
//! `loyalty_points = loyalty_points + ?` and synced as an absolute number.
//! Two tills serving the same customer
//! in the same shift each computed a total from what they could see, and the
//! merge kept one of them. `apply_customer` softened that with
//! `MAX(local, incoming)` so the larger survives, which bounds the damage
//! without fixing it: a till that awarded 5 points still loses them to one that
//! awarded 10.
//!
//! Addition is order-independent, so a ledger converges where a counter cannot.
//! Two terminals holding the same events reach the same balance regardless of
//! arrival order — which is exactly the property `stock_movements` already
//! relies on, and this deliberately mirrors it: append the event, then
//! recompute the cached figure the till reads.

use crate::errors::AppResult;
use sqlx::{Sqlite, SqlitePool};
use ulid::Ulid;

/// Why points moved. Stored as text so a reader of the table can tell without
/// a lookup, and so an unknown value from a newer terminal is data rather than
/// a decode failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoyaltyEvent {
    /// Points awarded by a sale.
    Earn,
    /// Points spent.
    Redeem,
    /// A manual correction by staff.
    Adjust,
}

impl LoyaltyEvent {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Earn => "earn",
            Self::Redeem => "redeem",
            Self::Adjust => "adjust",
        }
    }
}

pub struct AwardContext<'a> {
    pub customer_id: &'a str,
    pub branch_id: Option<&'a str>,
    pub device_id: Option<&'a str>,
    pub event: LoyaltyEvent,
    /// Signed. A redemption is negative.
    pub points_delta: i64,
    pub reference_type: Option<&'a str>,
    pub reference_id: Option<&'a str>,
    pub reason: Option<&'a str>,
    pub actor_user_id: Option<&'a str>,
}

/// The balance this terminal can prove from the events it holds.
///
/// Anchored on the earliest event's `points_after` and summed forward, so a
/// terminal that holds only part of the history still computes correctly —
/// provided the oldest event it has carries a truthful running total, which is
/// what `points_after` is for. Returns `None` when there are no events at all,
/// which means "nothing to say" rather than "zero".
pub async fn ledger_balance(pool: &SqlitePool, customer_id: &str) -> AppResult<Option<i64>> {
    let anchor: Option<(i64, String, i64)> = sqlx::query_as(
        "SELECT points_after, created_at, rowid
           FROM loyalty_events
          WHERE customer_id = ?
          ORDER BY datetime(created_at) ASC, rowid ASC
          LIMIT 1",
    )
    .bind(customer_id)
    .fetch_optional(pool)
    .await?;

    let Some((anchor_after, anchor_at, anchor_rowid)) = anchor else {
        return Ok(None);
    };

    let delta_sum: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(points_delta), 0)
           FROM loyalty_events
          WHERE customer_id = ?
            AND (datetime(created_at) > datetime(?)
                 OR (datetime(created_at) = datetime(?) AND rowid > ?))",
    )
    .bind(customer_id)
    .bind(&anchor_at)
    .bind(&anchor_at)
    .bind(anchor_rowid)
    .fetch_one(pool)
    .await?;

    // Never negative: a redemption that outran the balance is a bug upstream,
    // and showing a customer minus forty points helps nobody at the counter.
    Ok(Some((anchor_after + delta_sum).max(0)))
}

/// Refresh the cached figure on `customers` from the ledger.
///
/// The cache is what every existing read already uses, so it stays — but it is
/// now a consequence rather than the truth. Marked `pending` so the row still
/// travels for its *other* columns; `loyalty_points` itself is stripped on the
/// wire, so no terminal can overwrite another's derived total.
pub async fn recompute(pool: &SqlitePool, customer_id: &str) -> AppResult<i64> {
    let Some(balance) = ledger_balance(pool, customer_id).await? else {
        return Ok(sqlx::query_scalar(
            "SELECT loyalty_points FROM customers WHERE customer_id = ?",
        )
        .bind(customer_id)
        .fetch_optional(pool)
        .await?
        .unwrap_or(0));
    };
    sqlx::query(
        "UPDATE customers
            SET loyalty_points = ?, updated_at = ?, sync_status = 'pending'
          WHERE customer_id = ?",
    )
    .bind(balance)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(customer_id)
    .execute(pool)
    .await?;
    Ok(balance)
}

/// The same, on a connection the caller already holds open.
///
/// `create_refund` runs under its own `BEGIN IMMEDIATE` on one connection, so it
/// cannot hand over a `Transaction`. Reversing a sale's points has to happen
/// inside that same unit or a refund could succeed while the points it was
/// meant to claw back stayed on the customer.
pub async fn record_conn(
    conn: &mut sqlx::SqliteConnection,
    ctx: AwardContext<'_>,
) -> AppResult<i64> {
    record_on(conn, ctx).await
}

/// How many points a sale awarded, according to the ledger.
///
/// Read from `loyalty_events` rather than recomputed from the sale total: the
/// award rule can change, and a reversal has to give back what was actually
/// given, not what today's rule would have given.
pub async fn points_awarded_for_sale(
    conn: &mut sqlx::SqliteConnection,
    sale_id: &str,
) -> AppResult<i64> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(SUM(points_delta), 0) FROM loyalty_events
          WHERE reference_type = 'sale' AND reference_id = ? AND event_type = 'earn'",
    )
    .bind(sale_id)
    .fetch_one(conn)
    .await?)
}

/// Points already clawed back against a sale, so a partial refund cannot be
/// reversed twice and repeated partials cannot exceed what was awarded.
pub async fn points_reversed_for_sale(
    conn: &mut sqlx::SqliteConnection,
    sale_id: &str,
) -> AppResult<i64> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(-SUM(points_delta), 0) FROM loyalty_events
          WHERE reference_type = 'sale_reversal' AND reference_id = ?",
    )
    .bind(sale_id)
    .fetch_one(conn)
    .await?)
}

/// Append one event and return the new balance.
pub async fn record(pool: &SqlitePool, ctx: AwardContext<'_>) -> AppResult<i64> {
    let mut conn = pool.acquire().await?;
    record_on(&mut conn, ctx).await
}

/// The same, inside a caller's transaction.
///
/// A sale records its points in the transaction that records the sale. Points
/// that survive a crash the sale did not are worse than none: the customer
/// would be credited for a purchase that never completed.
pub async fn record_tx(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    ctx: AwardContext<'_>,
) -> AppResult<i64> {
    record_on(&mut *tx, ctx).await
}

/// Taken by `&mut` connection rather than a generic executor: the three
/// statements below have to run on the *same* connection to be one unit, and a
/// generic `Executor` is consumed by the first use.
async fn record_on(conn: &mut sqlx::SqliteConnection, ctx: AwardContext<'_>) -> AppResult<i64> {
    let previous: i64 = sqlx::query_scalar(
        "SELECT points_after FROM loyalty_events
          WHERE customer_id = ?
          ORDER BY datetime(created_at) DESC, rowid DESC
          LIMIT 1",
    )
    .bind(ctx.customer_id)
    .fetch_optional(&mut *conn)
    .await?
    .unwrap_or(0);

    let after = (previous + ctx.points_delta).max(0);
    // Store the delta that was actually applied, not the one that was asked for.
    //
    // Every row has to satisfy `points_after = previous.points_after +
    // points_delta`, because that is the equation `ledger_balance` inverts: it
    // anchors on the oldest surviving event's running total and adds every later
    // delta. Storing a raw −50 next to a clamped `after` of 0 breaks it, and the
    // damage surfaces later, somewhere else: redeem 50 against a balance of 5
    // (stored: delta −50, after 0), earn 20 (after 20, correct), then let any
    // sync-triggered `recompute` run — 5 + (−50) + 20 = −25, clamped to 0, and
    // twenty legitimately earned points are gone with nothing to explain it.
    //
    // Clamping is still right at the point of writing; what was wrong was
    // recording an intent the row did not carry out.
    let applied_delta = after - previous;
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO loyalty_events
            (loyalty_event_id, customer_id, branch_id, device_id, origin_device_id,
             event_type, points_delta, points_after, reference_type, reference_id,
             reason, created_by_user_id, created_at, updated_at, sync_status)
         VALUES (?, ?, ?, ?, COALESCE(?, ''), ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending')",
    )
    .bind(Ulid::new().to_string())
    .bind(ctx.customer_id)
    .bind(ctx.branch_id)
    .bind(ctx.device_id)
    .bind(ctx.device_id)
    .bind(ctx.event.as_str())
    .bind(applied_delta)
    .bind(after)
    .bind(ctx.reference_type)
    .bind(ctx.reference_id)
    .bind(ctx.reason)
    .bind(ctx.actor_user_id)
    .bind(&now)
    .bind(&now)
    .execute(&mut *conn)
    .await?;

    sqlx::query(
        "UPDATE customers
            SET loyalty_points = ?, updated_at = ?, sync_status = 'pending'
          WHERE customer_id = ?",
    )
    .bind(after)
    .bind(&now)
    .bind(ctx.customer_id)
    .execute(&mut *conn)
    .await?;

    Ok(after)
}

#[cfg(test)]
mod tests;
