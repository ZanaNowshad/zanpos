#![cfg(test)]
//! Which price is in force.
//!
//! A product can hold the price it used to have, the price it has, and one
//! scheduled for next week. Every screen and every till has to pick the
//! same row out of those, including when two are open at once.
//!
//! Part of the product-master suite; the shared fixtures live in
//! [`super`](super).

use super::super::{migrated_pool, seed, CASHIER, TAX_VAT};
use super::{price_row, what_the_screen_shows, what_the_till_charges};
use crate::db::repositories::{product_repo, sale_repo};

/// The cashier then sees one price and the till charges another. Because
/// checkout refuses a cart priced away from the catalogue, the sale does not go
/// through at the wrong price — it does not go through at all, and the operator
/// is told to re-scan an item that will scan at the same wrong price again.
/// Two open price rows must not give two different answers.
///
/// A shop that schedules a new price without closing the old one — an import, a
/// sync from the back office, a second row entered by hand — has two `selling`
/// rows with open windows. Checkout resolves that with an explicit tie-break
/// (newest `effective_from`, then `price_id`). The product query the scan door,
/// the search and the POS grid all share had the same window filter and no
/// tie-break at all, so it returned whichever row the join reached first.
///
#[tokio::test]
async fn the_screen_and_the_till_pick_the_same_price_when_two_are_open() {
    let pool = migrated_pool().await;
    seed(&pool).await;

    // The seeded price, backdated, plus a newer one that nobody closed.
    sqlx::query(
        "UPDATE product_prices SET effective_from = datetime('now','-10 days')
          WHERE price_id = 'prc_inv'",
    )
    .execute(&pool)
    .await
    .unwrap();
    price_row(&pool, "prc_newer", 2500, "datetime('now','-1 day')", None).await;
    sqlx::query(
        "UPDATE product_prices SET effective_from = datetime('now','-1 day')
          WHERE price_id = 'prc_newer'",
    )
    .execute(&pool)
    .await
    .unwrap();

    let till = what_the_till_charges(&pool).await;
    let screen = what_the_screen_shows(&pool).await;

    assert_eq!(
        till, 2500,
        "checkout did not take the newer of two open prices"
    );
    assert_eq!(
        screen, till,
        "the price on screen is not the price the till will charge: \
         the cashier scans {screen} and checkout insists on {till}"
    );
}

/// A price that starts tomorrow is not today's price.
///
/// Scheduling a price change is the whole point of `effective_from`. A lookup
/// that only checks `effective_to IS NULL` treats a future row as current, so
/// the shop starts charging the new price the moment it is entered rather than
/// on the day it was meant to begin.
#[tokio::test]
async fn a_price_that_starts_tomorrow_is_not_charged_today() {
    let pool = migrated_pool().await;
    seed(&pool).await;

    sqlx::query(
        "INSERT INTO product_prices
           (price_id, product_id, price_type, price_minor, currency,
            effective_from, created_by_user_id, created_at, updated_at)
         VALUES ('prc_future','prd_inv','selling', 9999, 'BHD',
                 datetime('now','+1 day'), ?, datetime('now'), datetime('now'))",
    )
    .bind(CASHIER)
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(
        what_the_till_charges(&pool).await,
        1000,
        "checkout charged a price that does not start until tomorrow"
    );
    assert_eq!(
        what_the_screen_shows(&pool).await,
        1000,
        "the screen showed a price that does not start until tomorrow"
    );
}

/// A price whose window has closed is history, not a price.
#[tokio::test]
async fn a_price_that_ended_yesterday_is_not_charged_today() {
    let pool = migrated_pool().await;
    seed(&pool).await;

    sqlx::query(
        "UPDATE product_prices
            SET effective_from = datetime('now','-10 days'),
                effective_to   = datetime('now','-1 day')
          WHERE price_id = 'prc_inv'",
    )
    .execute(&pool)
    .await
    .unwrap();
    price_row(&pool, "prc_current", 1500, "", None).await;
    sqlx::query(
        "UPDATE product_prices SET effective_from = datetime('now','-2 hours')
          WHERE price_id = 'prc_current'",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(what_the_till_charges(&pool).await, 1500);
    assert_eq!(what_the_screen_shows(&pool).await, 1500);
}

/// A product nobody has priced has no price, and says so rather than saying nothing.
///
/// `PRODUCT_QUERY` reads `COALESCE(pp.price_minor, 0)`, so an unpriced product
/// scans at zero rather than as "unpriced". Checkout refuses a zero price
/// outright — that guard is tested elsewhere — but the two disagree about what
/// the product *is*: one says it costs nothing, the other says it cannot be
/// sold. Pinned so the difference is deliberate rather than discovered.
#[tokio::test]
async fn an_unpriced_product_reads_as_zero_on_screen_and_as_absent_at_the_till() {
    let pool = migrated_pool().await;
    seed(&pool).await;
    sqlx::query(
        "INSERT INTO products
           (product_id, category_id, name, track_inventory, is_active, tax_rule_id,
            created_at, updated_at)
         VALUES ('prd_bare','cat_inv','Unpriced', 0, 1, ?, datetime('now'), datetime('now'))",
    )
    .bind(TAX_VAT)
    .execute(&pool)
    .await
    .unwrap();

    let on_screen = product_repo::get_product_by_id(&pool, "prd_bare")
        .await
        .unwrap()
        .expect("the product exists")
        .price_minor;
    assert_eq!(on_screen, 0, "the catalogue reads an absent price as zero");

    let at_the_till = sale_repo::current_selling_prices(&pool, &["prd_bare"])
        .await
        .unwrap();
    assert!(
        !at_the_till.contains_key("prd_bare"),
        "checkout must have no price for it at all, not a zero one"
    );
}

// ── Timestamps written two ways ──────────────────────────────────────────────

/// A price row the importer wrote is still in force when it says it is.
///
/// `effective_from`/`effective_to` hold two formats: Rust writes RFC3339
/// (`2026-09-01T18:00:00Z`), the importer writes `datetime('now')`
/// (`2026-09-01 18:00:00`). Compared as text, the space (0x20) ranks below `T`
/// (0x54), so an imported row ending later the same day sorts *below* an RFC3339
/// "now" and reads as already expired. The storefront joined prices that way and
/// with an inner JOIN, so the product did not merely lose its price — it
/// disappeared from the public catalogue.
#[tokio::test]
async fn a_price_written_in_the_importers_format_is_still_current() {
    let pool = migrated_pool().await;
    seed(&pool).await;

    // Written the way the importer writes: space-separated, and ending at the
    // last second of today.
    //
    // Same calendar date as `now` on purpose. That is the only shape where the
    // two formats actually diverge: once the dates differ, the year/month/day
    // decide the comparison before the separator is ever reached, and a raw text
    // compare gets the right answer by luck. Pinning `23:59:59` rather than a
    // relative offset keeps it on today's date whatever time the suite runs —
    // an earlier version used `+6 hours` and stopped reproducing the hazard
    // whenever the suite ran after six in the evening.
    sqlx::query(
        "UPDATE product_prices
            SET effective_from = date('now') || ' 00:00:00',
                effective_to   = date('now') || ' 23:59:59'
          WHERE price_id = 'prc_inv'",
    )
    .execute(&pool)
    .await
    .unwrap();

    // The comparison the bug made: raw text against an RFC3339 'now'.
    let raw: Option<i64> = sqlx::query_scalar(
        "SELECT price_minor FROM product_prices
          WHERE product_id = 'prd_inv'
            AND effective_from <= strftime('%Y-%m-%dT%H:%M:%fZ','now')
            AND (effective_to IS NULL
                 OR effective_to > strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(
        raw.is_none(),
        "the raw comparison found the row, so this test is no longer exercising \
         the two-format hazard. Only possible in the last second of a UTC day, \
         when 23:59:59 is not still ahead."
    );

    // Every production reader must still see it.
    assert_eq!(
        what_the_till_charges(&pool).await,
        1000,
        "checkout lost a price that is in force for another six hours"
    );
    assert_eq!(
        what_the_screen_shows(&pool).await,
        1000,
        "the catalogue lost a price that is in force for another six hours"
    );
}

/// The public storefront keeps the product too.
///
/// The storefront resolves prices with its own predicate — deliberately, because
/// it prefers a branch-specific price where the till only takes the shop-wide
/// one. That is a different question, not a second authority. What it must not
/// have is a different answer about *time*: it compared the effective window as
/// raw text against an RFC3339 `now`, so a row written in the importer's format
/// and ending later the same day read as expired. The join is an inner one, so
/// the product did not merely lose its price — it dropped out of the public
/// catalogue entirely.
#[tokio::test]
async fn an_importer_written_price_keeps_the_product_on_the_storefront() {
    let pool = migrated_pool().await;
    seed(&pool).await;

    sqlx::query(
        "UPDATE product_prices
            SET effective_from = date('now') || ' 00:00:00',
                effective_to   = date('now') || ' 23:59:59'
          WHERE price_id = 'prc_inv'",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO storefront_products (product_id, is_visible, updated_at)
         VALUES ('prd_inv', 1, datetime('now'))",
    )
    .execute(&pool)
    .await
    .expect("publish the product to the storefront");

    let snapshot = crate::storefront::catalog::build_catalog_snapshot(&pool)
        .await
        .expect("build the public catalogue");

    let listed = snapshot.products.iter().find(|p| p.id == "prd_inv").expect(
        "the product fell out of the public catalogue: its price is in force \
             until the end of today, but the window was compared as raw text",
    );
    assert_eq!(listed.price_minor, 1000);
}

/// An unpriced product is refused when it is scanned, not at the Charge button.
///
/// The catalogue reads `COALESCE(pp.price_minor, 0)`, so a product with no price
/// row shows 0.000 and looks free. Checkout refuses it — but only once the
/// basket is full and the customer is waiting. The back office still needs to see
/// unpriced products in order to price them, so the catalogue query keeps
/// returning them; what stops is putting one in a basket.
#[tokio::test]
async fn an_unpriced_product_is_kept_out_of_the_basket() {
    let pool = migrated_pool().await;
    seed(&pool).await;
    sqlx::query(
        "INSERT INTO products
           (product_id, category_id, name, track_inventory, is_active, tax_rule_id,
            created_at, updated_at)
         VALUES ('prd_nopr','cat_inv','Unpriced Thing', 0, 1, ?, datetime('now'), datetime('now'))",
    )
    .bind(TAX_VAT)
    .execute(&pool)
    .await
    .unwrap();

    let unpriced = product_repo::get_product_by_id(&pool, "prd_nopr")
        .await
        .unwrap()
        .expect("the back office can still see it");
    assert_eq!(unpriced.price_minor, 0);

    let mut cart =
        crate::domain::cart::Cart::new("b".into(), "d".into(), "s".into(), CASHIER.into());
    let outcome =
        crate::commands::pos_commands::add_or_merge_line(&pool, &mut cart, &unpriced, "1").await;

    let error = outcome.expect_err("an unpriced product must not reach the basket");
    assert!(
        error.to_string().contains("Unpriced Thing"),
        "the cashier needs to know which item, got: {error}"
    );
    assert!(cart.lines.is_empty(), "the line was added anyway");

    // A priced one still goes in, so the guard is not simply refusing everything.
    let priced = product_repo::get_product_by_id(&pool, "prd_inv")
        .await
        .unwrap()
        .unwrap();
    crate::commands::pos_commands::add_or_merge_line(&pool, &mut cart, &priced, "1")
        .await
        .expect("a priced product must still scan");
    assert_eq!(cart.lines.len(), 1);
}
