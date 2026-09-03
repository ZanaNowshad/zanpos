//! Why the shelf count is what it is, and whether anything disagrees.
//!
//! Stock is two things that must always say the same number. `stock_movements`
//! is the ledger — an append-only record of every event that moved a quantity,
//! each one carrying what it moved, what the total became, and what caused it.
//! `stock_levels` is a cache of the running total, kept because every product
//! grid and every scan reads it and replaying a ledger per keystroke is not
//! affordable.
//!
//! A cache can drift from its ledger. When it does, the shelf count is a number
//! nobody can explain: the movements add up to one figure and the cache reports
//! another, and there is no way to tell which is right without counting the
//! shelf. This module is what turns that from a mystery into a report.
//!
//! The equation, stated once:
//!
//! ```text
//! quantity = quantity_after(earliest movement) + sum(quantity_delta of every later movement)
//! ```
//!
//! Anchoring on the earliest surviving movement's post-state rather than summing
//! from zero is deliberate: retention pruning deletes old movements, so the sum
//! of what remains is not the whole history. The earliest surviving row records
//! what the total already was, which is exactly the opening balance for
//! everything that follows.

use crate::errors::AppResult;
use serde::Serialize;
use sqlx::{Row, SqlitePool};

/// The quantity the ledger says a product should have.
///
/// `None` means this terminal holds no movements for the product at all, which
/// is different from a balance of zero: it is "the ledger has nothing to say",
/// the condition a freshly-onboarded till is in before movement replay reaches
/// it. Callers have to tell those apart — treating silence as zero is how a new
/// terminal wipes a shelf count it simply had not been told about yet.
pub async fn ledger_quantity(
    pool: &SqlitePool,
    product_id: &str,
    branch_id: &str,
) -> AppResult<Option<f64>> {
    let anchor: Option<(f64, String, i64)> = sqlx::query_as(
        "SELECT CAST(quantity_after AS REAL), created_at, rowid
           FROM stock_movements
          WHERE product_id = ? AND branch_id = ?
          ORDER BY datetime(created_at) ASC, rowid ASC
          LIMIT 1",
    )
    .bind(product_id)
    .bind(branch_id)
    .fetch_optional(pool)
    .await?;

    let Some((anchor_after, anchor_at, anchor_rowid)) = anchor else {
        return Ok(None);
    };

    // `rowid` breaks the tie within a second. Movements written by one
    // transaction share a timestamp, and ordering them by time alone would make
    // the boundary between "the anchor" and "everything after it" arbitrary.
    let delta_sum: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(CAST(quantity_delta AS REAL)), 0.0)
           FROM stock_movements
          WHERE product_id = ? AND branch_id = ?
            AND (datetime(created_at) > datetime(?)
                 OR (datetime(created_at) = datetime(?) AND rowid > ?))",
    )
    .bind(product_id)
    .bind(branch_id)
    .bind(&anchor_at)
    .bind(&anchor_at)
    .bind(anchor_rowid)
    .fetch_one(pool)
    .await?;

    Ok(Some(anchor_after + delta_sum))
}

/// A product whose cached quantity does not match its ledger.
#[derive(Debug, Clone, Serialize)]
pub struct StockDiscrepancy {
    pub product_id: String,
    pub product_name: String,
    pub branch_id: String,
    /// What `stock_levels` reports — the number every screen shows.
    pub cached_quantity: f64,
    /// What the movements add up to.
    pub ledger_quantity: f64,
    /// Cached minus ledger. Positive means the shelf count claims more than the
    /// movements can account for.
    pub difference: f64,
    /// How many movements the ledger holds for this product.
    pub movement_count: i64,
}

/// Every product whose cache and ledger disagree.
///
/// Products with no movements are skipped rather than reported as a discrepancy
/// against zero: an opening balance seeded before any movement exists is a
/// legitimate state, not a fault. What this finds is a cache that *has* a ledger
/// and has drifted from it.
///
/// `tolerance` absorbs the last bits of floating-point noise. Quantities are
/// stored as text and parsed as `REAL`, so a shelf that has been through
/// thousands of fractional movements can land a hair off an exact comparison
/// while being entirely correct. It defaults to well below the smallest quantity
/// any till can ring up.
pub async fn discrepancies(
    pool: &SqlitePool,
    branch_id: Option<&str>,
    tolerance: f64,
) -> AppResult<Vec<StockDiscrepancy>> {
    let rows = sqlx::query(
        "SELECT sl.product_id, sl.branch_id,
                COALESCE(p.name, '(deleted product)') AS product_name,
                CAST(sl.quantity_on_hand AS REAL) AS cached
           FROM stock_levels sl
           LEFT JOIN products p ON p.product_id = sl.product_id
          WHERE (? IS NULL OR sl.branch_id = ?)
          ORDER BY p.name",
    )
    .bind(branch_id)
    .bind(branch_id)
    .fetch_all(pool)
    .await?;

    let mut out = Vec::new();
    for row in &rows {
        let product_id: String = row.get("product_id");
        let branch: String = row.get("branch_id");
        let cached: f64 = row.get("cached");

        let Some(derived) = ledger_quantity(pool, &product_id, &branch).await? else {
            continue;
        };
        if (cached - derived).abs() <= tolerance {
            continue;
        }

        let movement_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM stock_movements WHERE product_id = ? AND branch_id = ?",
        )
        .bind(&product_id)
        .bind(&branch)
        .fetch_one(pool)
        .await?;

        out.push(StockDiscrepancy {
            product_id,
            product_name: row.get("product_name"),
            branch_id: branch,
            cached_quantity: cached,
            ledger_quantity: derived,
            difference: cached - derived,
            movement_count,
        });
    }
    Ok(out)
}

/// One line of the account of how a product reached its current quantity.
#[derive(Debug, Clone, Serialize)]
pub struct MovementExplanation {
    pub movement_id: String,
    pub movement_type: String,
    pub quantity_delta: String,
    pub quantity_after: String,
    /// What caused it — `sale`, `refund`, `po_receipt` and so on.
    pub reference_type: Option<String>,
    /// The specific document: which sale, which refund, which stock take.
    pub reference_id: Option<String>,
    pub created_by_user_id: Option<String>,
    pub device_id: Option<String>,
    pub created_at: String,
}

/// The full account, oldest first: every movement that made the number.
///
/// This is the answer to "why does the system think we have eleven of these".
/// Each line names what moved, what the total became, who did it, on which
/// terminal, and which document it belongs to — so any figure can be walked back
/// to the events that produced it.
pub async fn explain(
    pool: &SqlitePool,
    product_id: &str,
    branch_id: &str,
) -> AppResult<Vec<MovementExplanation>> {
    let rows = sqlx::query(
        "SELECT movement_id, movement_type, quantity_delta, quantity_after,
                reference_type, reference_id, created_by_user_id, device_id, created_at
           FROM stock_movements
          WHERE product_id = ? AND branch_id = ?
          ORDER BY datetime(created_at) ASC, rowid ASC",
    )
    .bind(product_id)
    .bind(branch_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| MovementExplanation {
            movement_id: r.get("movement_id"),
            movement_type: r.get("movement_type"),
            quantity_delta: r.get("quantity_delta"),
            quantity_after: r.get("quantity_after"),
            reference_type: r.get("reference_type"),
            reference_id: r.get("reference_id"),
            created_by_user_id: r.get("created_by_user_id"),
            device_id: r.get("device_id"),
            created_at: r.get("created_at"),
        })
        .collect())
}
