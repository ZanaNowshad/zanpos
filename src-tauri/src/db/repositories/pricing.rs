//! The one definition of "what this product sells for right now".
//!
//! Selling prices live in `product_prices` and nowhere else — there is no price
//! column on `products`. A product can hold many rows: the price it used to have,
//! the price it has, and a price scheduled to begin next week. Picking the right
//! one takes four conditions, and every screen that shows a price and every path
//! that charges one has to apply all four the same way.
//!
//! They did not. Checkout resolved the price with a tie-break; the query behind
//! the scan door, the product search and the POS grid applied the same window
//! filter and no tie-break at all, so with two open rows it returned whichever
//! the join happened to reach first. The cashier saw one price, checkout insisted
//! on another, and because checkout refuses a cart priced away from the catalogue
//! the sale did not go through at the wrong price — it did not go through at all,
//! and the operator was told to re-scan an item that would scan at the same wrong
//! price again.
//!
//! So the predicate is written once, here, and composed into both. A price
//! question answered any other way is a second authority, and two authorities on
//! one number is the shape of the bug above.

/// Restricts `product_prices` rows aliased as `pp` to the single row in force.
///
/// Composed into a `WHERE` or a `JOIN … ON`. The subquery is correlated on
/// `pp.product_id`, so it is per-product and works for one row or a batch.
///
/// The four conditions, and why each is load-bearing:
///
/// - `branch_id IS NULL` — the shop-wide price. Branch overrides are a separate
///   concept and are not what the till charges today.
/// - `price_type = 'selling'` — `product_prices` also carries cost and other
///   types; charging a cost price would sell at cost.
/// - `effective_from <= now` — a price scheduled for next week is not today's
///   price. Omitting this starts the new price the moment it is entered.
/// - `effective_to` open — a price whose window has closed is history.
///
/// And then the tie-break, which is the part that was missing: when two rows are
/// genuinely in force — an import that did not close the old row, a sync from the
/// back office, a second row typed by hand — *something* has to choose, and every
/// reader has to choose the same one. Newest `effective_from` wins; `price_id`
/// breaks an exact tie so the answer does not depend on the query plan.
///
/// `datetime()` on both sides is not decoration: the column holds two formats.
/// Rust writes RFC3339 (`2026-08-31T09:00:00Z`), the importer writes
/// `datetime('now')` (space-separated), and comparing them as text ranks `T`
/// above a space — so a raw `<=` gets the answer wrong whenever the two formats
/// meet. `datetime()` normalises both.
pub const PRICE_IN_FORCE: &str = "pp.price_id = (
    SELECT candidate.price_id FROM product_prices candidate
    WHERE candidate.product_id = pp.product_id
      AND candidate.branch_id IS NULL
      AND candidate.price_type = 'selling'
      AND datetime(candidate.effective_from) <= datetime('now')
      AND (candidate.effective_to IS NULL
           OR datetime(candidate.effective_to) > datetime('now'))
    ORDER BY datetime(candidate.effective_from) DESC, candidate.price_id DESC
    LIMIT 1
)";
