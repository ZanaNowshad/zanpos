#![cfg(test)]
//! What a sale charged does not move.
//!
//! The line snapshots the name, price and tax rule as they stood. That is
//! what makes a receipt reprintable and a VAT return defensible — a
//! catalogue edit today must not restate what a customer paid last week.
//!
//! Part of the product-master suite; the shared fixtures live in
//! [`super`](super).

use super::super::{migrated_pool, one_real_sale, TAX_VAT};
use super::{price_row, what_the_till_charges};

// ── History does not move ────────────────────────────────────────────────────

/// Repricing a product today does not rewrite what it sold for last week.
///
/// `sale_items` snapshots the name, SKU, barcode, unit price, tax rule and tax
/// amount at the moment of sale. That is what makes a receipt reprintable and a
/// VAT return defensible: the line says what the customer was actually charged,
/// not what the product costs now. A sale that read today's price through a join
/// would restate every historical margin every time the shop changed a price.
#[tokio::test]
async fn changing_todays_price_does_not_move_yesterdays_sale() {
    let pool = migrated_pool().await;
    let (sale, _shift) = one_real_sale(&pool, "2", "cat-history").await;

    let before: (i64, i64, i64, String, String) = sqlx::query_as(
        "SELECT unit_price_minor, line_total_minor, tax_amount_minor,
                product_name_snapshot, tax_rule_snapshot
           FROM sale_items WHERE sale_id = ?",
    )
    .bind(&sale)
    .fetch_one(&pool)
    .await
    .unwrap();
    let header_before: (i64, i64) =
        sqlx::query_as("SELECT net_total_minor, tax_total_minor FROM sales WHERE sale_id = ?")
            .bind(&sale)
            .fetch_one(&pool)
            .await
            .unwrap();

    // The back office triples the price, renames the product and moves it to a
    // different tax rule — everything a catalogue edit can touch.
    sqlx::query(
        "UPDATE product_prices SET effective_to = datetime('now') WHERE price_id = 'prc_inv'",
    )
    .execute(&pool)
    .await
    .unwrap();
    price_row(&pool, "prc_tripled", 3000, "", None).await;
    sqlx::query(
        "UPDATE product_prices SET effective_from = datetime('now')
          WHERE price_id = 'prc_tripled'",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO tax_rules (tax_rule_id, name, rate_basis_points, inclusive, is_active,
                                effective_from, created_at, updated_at)
         VALUES ('tax_new','VAT 20%', 2000, 0, 1,
                 datetime('now'), datetime('now'), datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE products SET name = 'Cola 330ml (renamed)', tax_rule_id = 'tax_new'
          WHERE product_id = 'prd_inv'",
    )
    .execute(&pool)
    .await
    .unwrap();

    let after: (i64, i64, i64, String, String) = sqlx::query_as(
        "SELECT unit_price_minor, line_total_minor, tax_amount_minor,
                product_name_snapshot, tax_rule_snapshot
           FROM sale_items WHERE sale_id = ?",
    )
    .bind(&sale)
    .fetch_one(&pool)
    .await
    .unwrap();
    let header_after: (i64, i64) =
        sqlx::query_as("SELECT net_total_minor, tax_total_minor FROM sales WHERE sale_id = ?")
            .bind(&sale)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(
        before, after,
        "a catalogue edit rewrote what a completed sale had charged"
    );
    assert_eq!(
        header_before, header_after,
        "a catalogue edit moved the totals on a completed sale"
    );
    assert_eq!(
        before.3, "Cola 330ml",
        "the line must keep the name it was sold under, not pick up the new one"
    );

    // And the till has genuinely moved on, so the fixture is not simply inert.
    assert_eq!(
        what_the_till_charges(&pool).await,
        3000,
        "the new price is not in force — this test would pass on a broken fixture"
    );
}

/// The tax on a historical line is the tax that was charged, not today's rate.
///
/// `tax_rule_snapshot` holds the rule as it stood, so a rate change next quarter
/// cannot restate last quarter's VAT. Recomputing from `products.tax_rule_id`
/// instead would make every filed return disagree with the books.
#[tokio::test]
async fn changing_a_tax_rate_does_not_restate_what_was_already_collected() {
    let pool = migrated_pool().await;
    let (sale, _shift) = one_real_sale(&pool, "1", "cat-tax").await;

    let (tax_before, snapshot): (i64, String) = sqlx::query_as(
        "SELECT tax_amount_minor, tax_rule_snapshot FROM sale_items WHERE sale_id = ?",
    )
    .bind(&sale)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(tax_before, 100, "10% of 1.000");
    assert!(
        snapshot.contains("1000"),
        "the line must carry the rate it was taxed at, got {snapshot}"
    );

    // The rate doubles.
    sqlx::query("UPDATE tax_rules SET rate_basis_points = 2000 WHERE tax_rule_id = ?")
        .bind(TAX_VAT)
        .execute(&pool)
        .await
        .unwrap();

    let (tax_after, snapshot_after): (i64, String) = sqlx::query_as(
        "SELECT tax_amount_minor, tax_rule_snapshot FROM sale_items WHERE sale_id = ?",
    )
    .bind(&sale)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (tax_after, snapshot_after),
        (tax_before, snapshot),
        "a tax-rate change restated tax that had already been collected and filed"
    );
}

/// Cost is recorded on the line but never sets the price.
///
/// `products.cost_minor` is what the shop paid; `product_prices` is what the
/// customer pays. Checkout reads cost once, to snapshot it onto the line so the
/// margin on that sale stays computable after the cost changes — and never as an
/// input to what is charged. A path that priced from cost would sell at cost the
/// moment a price row was missing.
#[tokio::test]
async fn cost_is_snapshotted_onto_the_line_and_never_priced_from() {
    let pool = migrated_pool().await;
    sqlx::query("UPDATE products SET cost_minor = 600 WHERE product_id = 'prd_inv'")
        .execute(&pool)
        .await
        .ok();
    let (sale, _shift) = one_real_sale(&pool, "1", "cat-cost").await;
    sqlx::query("UPDATE products SET cost_minor = 600 WHERE product_id = 'prd_inv'")
        .execute(&pool)
        .await
        .unwrap();

    let (charged, cost): (i64, Option<i64>) = sqlx::query_as(
        "SELECT unit_price_minor, cost_minor_snapshot FROM sale_items WHERE sale_id = ?",
    )
    .bind(&sale)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        charged, 1000,
        "the customer pays the selling price, not the cost"
    );
    assert_ne!(
        Some(charged),
        cost,
        "the line was priced from cost — the shop would sell at what it paid"
    );

    // The cost moves; the sale does not.
    sqlx::query("UPDATE products SET cost_minor = 950 WHERE product_id = 'prd_inv'")
        .execute(&pool)
        .await
        .unwrap();
    let after: Option<i64> =
        sqlx::query_scalar("SELECT cost_minor_snapshot FROM sale_items WHERE sale_id = ?")
            .bind(&sale)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        after, cost,
        "a cost change restated the margin on a sale already made"
    );
}
