#![cfg(test)]
//! Prices, markdowns, and tax inside the label.
//!
//! What the till is allowed to charge. A manager may move a price away
//! from the catalogue, for a stated quantity; nothing else may.
//!
//! Part of the till lifecycle suite; the shared fixtures live in
//! [`super`](super).

use super::super::{migrated_pool, CASHIER, TAX_VAT};
use super::{assert_pre_sale_state, cash, due, line, till};
use crate::db::repositories::sale_repo;
use sqlx::SqlitePool;

// ── Manager price overrides ──────────────────────────────────────────────────

/// Record the approval `pos_set_line_price` writes when a manager marks a line
/// down, including how many units they approved it for.
async fn approve_price(
    pool: &SqlitePool,
    cart_id: &str,
    cart_line_id: &str,
    product_id: &str,
    price_minor: i64,
    quantity: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO pos_price_overrides
           (cart_line_id, cart_id, product_id, price_minor, approved_quantity,
            authorized_by_user_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, datetime('now'))",
    )
    .bind(cart_line_id)
    .bind(cart_id)
    .bind(product_id)
    .bind(price_minor)
    .bind(quantity)
    .bind(CASHIER)
    .execute(pool)
    .await
    .expect("record the price approval");
}

/// A manager's markdown sells at the marked-down price.
///
/// The negative case is covered elsewhere; this pins the positive one, because a
/// guard that refuses everything is not a working till.
#[tokio::test]
async fn an_approved_markdown_sells_at_the_approved_price() {
    let pool = migrated_pool().await;
    let (_shift, mut cart) = till(&pool).await;
    let mut marked = line("prd_inv", "Cola 330ml", "1", 1000);
    marked.unit_price_minor = 400; // catalogue is 1000
    marked.recalculate();
    let line_id = marked.cart_line_id.clone();
    let total = marked.line_total_minor;
    cart.lines.push(marked);
    approve_price(&pool, &cart.cart_id, &line_id, "prd_inv", 400, Some("1")).await;

    let result = sale_repo::finalize_sale(
        &pool,
        &cart,
        cash(total),
        "life-markdown-ok",
        None,
        false,
        None,
        false,
    )
    .await
    .expect("an approved markdown must sell");

    let charged: i64 =
        sqlx::query_scalar("SELECT unit_price_minor FROM sale_items WHERE sale_id = ?")
            .bind(&result.sale_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(charged, 400, "the till charged the price the manager set");
}

/// A markdown approved for one unit does not carry to a line that grew.
///
/// This is what a re-scan does: `pos_add_item_by_barcode` merges the second beep
/// into the existing line and raises its quantity, and `pos_update_quantity`
/// lets a cashier type a new one with no manager at all. Neither touches the
/// override, so a markdown on one damaged item covered however many the cashier
/// added to that line.
#[tokio::test]
async fn a_markdown_approved_for_one_does_not_cover_ten() {
    let pool = migrated_pool().await;
    let (_shift, mut cart) = till(&pool).await;
    let mut marked = line("prd_inv", "Cola 330ml", "10", 1000);
    marked.unit_price_minor = 400;
    marked.recalculate();
    let line_id = marked.cart_line_id.clone();
    let total = marked.line_total_minor;
    cart.lines.push(marked);
    // The manager approved 0.400 — for one.
    approve_price(&pool, &cart.cart_id, &line_id, "prd_inv", 400, Some("1")).await;

    let err = sale_repo::finalize_sale(
        &pool,
        &cart,
        cash(total),
        "life-markdown-grown",
        None,
        false,
        None,
        false,
    )
    .await
    .expect_err("ten units at a price approved for one must not sell");

    assert!(
        err.to_string().contains("Cola 330ml"),
        "the cashier needs to know which line, got: {err}"
    );
    assert_pre_sale_state(&pool, "a markdown stretched past its approved quantity").await;
}

/// Selling fewer than were approved is fine — the customer changed their mind.
#[tokio::test]
async fn a_markdown_still_covers_a_smaller_quantity() {
    let pool = migrated_pool().await;
    let (_shift, mut cart) = till(&pool).await;
    let mut marked = line("prd_inv", "Cola 330ml", "2", 1000);
    marked.unit_price_minor = 400;
    marked.recalculate();
    let line_id = marked.cart_line_id.clone();
    let total = marked.line_total_minor;
    cart.lines.push(marked);
    approve_price(&pool, &cart.cart_id, &line_id, "prd_inv", 400, Some("5")).await;

    sale_repo::finalize_sale(
        &pool,
        &cart,
        cash(total),
        "life-markdown-fewer",
        None,
        false,
        None,
        false,
    )
    .await
    .expect("two of five approved units must still sell");
}

// ── Tax charged inside the shelf price ───────────────────────────────────────

/// A tax-inclusive line does not add tax on top of the shelf price.
///
/// The customer pays what the label says; the tax is carved out of it for the
/// return. Adding it on top instead overcharges every customer and reports a
/// figure the takings cannot support.
#[tokio::test]
async fn an_inclusive_price_carves_the_tax_out_rather_than_adding_it() {
    let pool = migrated_pool().await;
    let (_shift, mut cart) = till(&pool).await;
    sqlx::query("UPDATE tax_rules SET inclusive = 1 WHERE tax_rule_id = ?")
        .bind(TAX_VAT)
        .execute(&pool)
        .await
        .unwrap();

    let mut inclusive = line("prd_inv", "Cola 330ml", "1", 1000);
    inclusive.tax_inclusive = true;
    inclusive.recalculate();
    cart.lines.push(inclusive);

    let result = sale_repo::finalize_sale(
        &pool,
        &cart,
        cash(1000),
        "life-inclusive",
        None,
        false,
        None,
        false,
    )
    .await
    .expect("checkout");

    let (net, tax, gross): (i64, i64, i64) = sqlx::query_as(
        "SELECT net_total_minor, tax_total_minor, gross_total_minor
           FROM sales WHERE sale_id = ?",
    )
    .bind(&result.sale_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(net, 1000, "the customer pays the shelf price, not 1.100");
    assert_eq!(gross, 1000);
    assert_eq!(
        tax, 91,
        "10% carved out of 1.000 is 0.091, not the 0.100 that adding it on top would give"
    );
    assert_eq!(
        net - tax,
        909,
        "what is left after the tax is the taxable value"
    );
}

// ── Price integrity ──────────────────────────────────────────────────────────

/// A cart priced below the catalogue is refused, however the payload got that
/// way. The till is not the authority on price; `product_prices` is.
#[tokio::test]
async fn a_cart_priced_under_the_catalogue_is_refused() {
    let pool = migrated_pool().await;
    let (_shift, mut cart) = till(&pool).await;
    // The catalogue says 1.000; this claims 0.100.
    cart.lines.push(line("prd_inv", "Cola 330ml", "1", 100));
    let total = due(&cart);

    let outcome = sale_repo::finalize_sale(
        &pool,
        &cart,
        cash(total),
        "life-underprice",
        None,
        false,
        None,
        false,
    )
    .await;
    assert!(outcome.is_err(), "a hand-lowered price was accepted");
    assert_pre_sale_state(&pool, "an under-priced cart").await;
}

/// A product nobody has priced cannot be sold for nothing.
///
/// The catalogue query reads `COALESCE(pp.price_minor, 0)`, so an item with no
/// row in `product_prices` — a new line the back office half-created, an import
/// that dropped the price — scans at zero and shows 0.000 on the screen. The
/// price check further down only compares against a catalogue price that exists,
/// so nothing there catches it either. Refusing a zero price outright is what
/// stops the shop giving the item away and never knowing.
#[tokio::test]
async fn a_product_with_no_price_is_refused_rather_than_sold_for_nothing() {
    let pool = migrated_pool().await;
    let (_shift, mut cart) = till(&pool).await;
    sqlx::query(
        "INSERT INTO products
           (product_id, category_id, name, track_inventory, is_active, tax_rule_id,
            created_at, updated_at)
         VALUES ('prd_unpriced','cat_inv','Unpriced Thing', 0, 1, ?,
                 datetime('now'), datetime('now'))",
    )
    .bind(TAX_VAT)
    .execute(&pool)
    .await
    .unwrap();

    let mut free = line("prd_unpriced", "Unpriced Thing", "1", 0);
    free.unit_price_minor = 0;
    free.recalculate();
    cart.lines.push(free);

    let err = sale_repo::finalize_sale(
        &pool,
        &cart,
        cash(0),
        "life-no-price",
        None,
        false,
        None,
        false,
    )
    .await
    .expect_err("an unpriced product must not sell");

    assert!(
        err.to_string().contains("Unpriced Thing"),
        "the cashier needs to know which item to deal with, got: {err}"
    );
    assert_pre_sale_state(&pool, "a product with no price").await;
}

/// Adding to a marked-down line is refused however the item gets there.
///
/// The scan path and the product-tile path had grown separate copies of the
/// merge, and only the scan one checked the override. The tile path was the
/// easier bypass of the two, because it takes a quantity outright rather than
/// adding one at a time — tap the tile with 50 on a line a manager marked down
/// to 0.100 and the shop sells fifty at the markdown. Both paths now go through
/// the same check, and this is the finalize-side backstop for it.
#[tokio::test]
async fn a_line_that_grew_past_its_markdown_is_refused_whichever_path_grew_it() {
    let pool = migrated_pool().await;
    let (_shift, mut cart) = till(&pool).await;
    let mut marked = line("prd_inv", "Cola 330ml", "1", 1000);
    marked.unit_price_minor = 100;
    marked.recalculate();
    let line_id = marked.cart_line_id.clone();
    cart.lines.push(marked);
    approve_price(&pool, &cart.cart_id, &line_id, "prd_inv", 100, Some("1")).await;

    // The tile path merges an arbitrary quantity into the existing line.
    cart.lines[0].quantity = "50".into();
    cart.lines[0].recalculate();

    sale_repo::finalize_sale(
        &pool,
        &cart,
        cash(cart.lines[0].line_total_minor),
        "life-tile-bypass",
        None,
        false,
        None,
        false,
    )
    .await
    .expect_err("fifty units at a price approved for one must not sell");

    assert_pre_sale_state(&pool, "a markdown grown by the tile path").await;
}
