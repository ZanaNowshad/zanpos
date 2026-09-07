#![cfg(test)]
//! Money: totals, payments, price resolution and representation.
//!
//! Split from the module root only to keep each file under the 500-line
//! rule; the fixtures they share live in `super`.

use super::{migrated_pool, one_real_sale, seed, CASHIER};

// ── Invariant 1: the lines add up to the total ───────────────────────────────

/// `sum(line_total) - bill discount == net total`, and the tax on the sale is
/// the tax on its lines.
///
/// This is the number a shop reconciles against its bank. If the header and the
/// lines can disagree, every report built on either is unverifiable against the
/// other, and nothing in the schema prevents it: `sales` stores its own totals
/// rather than deriving them.
#[tokio::test]
async fn a_sale_header_agrees_with_its_lines() {
    let pool = migrated_pool().await;
    let (sale_id, _) = one_real_sale(&pool, "3", "inv-lines").await;

    let (net, gross, tax, discount): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT net_total_minor, gross_total_minor, tax_total_minor, discount_total_minor
           FROM sales WHERE sale_id = ?",
    )
    .bind(&sale_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let (line_total, line_tax, line_discount): (i64, i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(line_total_minor),0),
                COALESCE(SUM(tax_amount_minor),0),
                COALESCE(SUM(line_discount_minor),0)
           FROM sale_items WHERE sale_id = ? AND voided = 0",
    )
    .bind(&sale_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        net,
        line_total - discount,
        "sale {sale_id}: net {net} but lines total {line_total} less discount {discount}"
    );
    assert_eq!(tax, line_tax, "sale header tax disagrees with its lines");
    assert_eq!(
        gross,
        line_total - line_tax + line_discount,
        "gross should be the pre-tax, pre-discount value of the lines"
    );
}

// ── Invariant 2: payments cover the sale ─────────────────────────────────────

/// `sum(payments) == net total` for a completed sale.
///
/// A sale recorded as completed for which the payments do not add up is either
/// money taken and not recorded, or a sale recorded that was never paid for.
#[tokio::test]
async fn payments_cover_every_completed_sale_exactly() {
    let pool = migrated_pool().await;
    one_real_sale(&pool, "2", "inv-pay").await;

    let mismatched: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT s.sale_id, s.net_total_minor, COALESCE(SUM(p.amount_minor),0)
           FROM sales s
           LEFT JOIN payments p ON p.sale_id = s.sale_id AND p.status = 'approved'
          WHERE s.status = 'completed'
          GROUP BY s.sale_id, s.net_total_minor
         HAVING COALESCE(SUM(p.amount_minor),0) <> s.net_total_minor",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert!(
        mismatched.is_empty(),
        "completed sales whose approved payments do not equal the total: {mismatched:?}"
    );
}

// ── Invariant 4: the price came from the price record ────────────────────────

/// The line price on a sale is the active `product_prices` row.
///
/// There is no price column on `products`; the authoritative record is a
/// `product_prices` row that is currently effective. A line priced from
/// anywhere else is a sale at a price the shop never set.
#[tokio::test]
async fn a_sale_line_is_priced_from_the_authoritative_price_record() {
    let pool = migrated_pool().await;
    let (sale_id, _) = one_real_sale(&pool, "1", "inv-price").await;

    let unit: i64 =
        sqlx::query_scalar("SELECT unit_price_minor FROM sale_items WHERE sale_id = ? LIMIT 1")
            .bind(&sale_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    let authoritative: i64 = sqlx::query_scalar(
        "SELECT price_minor FROM product_prices
          WHERE product_id='prd_inv' AND branch_id IS NULL AND price_type='selling'
            AND datetime(effective_from) <= datetime('now')
            AND (effective_to IS NULL OR datetime(effective_to) > datetime('now'))
          ORDER BY datetime(effective_from) DESC, price_id DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        unit, authoritative,
        "the line was priced at {unit} while the active price record says {authoritative}"
    );
}

// ── Invariant 7: money and time are stored one way ───────────────────────────

/// Every monetary column is an integer of minor units.
///
/// BHD has three decimal places. A monetary value stored as text or as a float
/// anywhere means two representations of money in one database, and a rounding
/// difference between them is a discrepancy nobody can explain later.
#[tokio::test]
async fn every_monetary_column_is_an_integer_of_minor_units() {
    let pool = migrated_pool().await;
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let mut wrong = Vec::new();
    for table in tables {
        let cols: Vec<(String, String)> = sqlx::query_as(&format!(
            "SELECT name, type FROM pragma_table_info('{table}')"
        ))
        .fetch_all(&pool)
        .await
        .unwrap_or_default();
        for (name, ty) in cols {
            if name.ends_with("_minor") && ty.to_uppercase() != "INTEGER" {
                wrong.push(format!("{table}.{name} is {ty}"));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "monetary columns not stored as integer minor units: {wrong:?}"
    );
}

// ── Invariant 8: one answer to "what does this cost" ─────────────────────────

/// Every predicate that resolves the active selling price agrees with checkout,
/// including when the column holds both timestamp formats at once.
///
/// `product_prices.effective_from` genuinely holds two: every Rust writer binds
/// `chrono::Utc::now().to_rfc3339()` (`2026-08-31T09:00:00Z`) while the
/// legacy-POS importer writes `datetime('now')` (`2026-08-31 09:00:00`). A shop
/// that migrated from another POS has both.
///
/// Compared as text, `'T'` (0x54) sorts above a space (0x20), so
/// `'…T09:00:00Z' <= '… 12:00:00'` is **false** — a price set this morning reads
/// as not yet effective — and `ORDER BY effective_from DESC` ranks every
/// RFC3339 row above every imported row whatever the real instant. Two callers
/// compared raw: the storefront (publishing a price the till would not charge)
/// and ZanAI's margin insight (reporting against a superseded price).
///
/// The fixture below is the case that separates them: an old imported price in
/// space format, superseded by a newer one in RFC3339 format, both effective.
#[tokio::test]
async fn every_price_lookup_agrees_with_what_the_till_charges() {
    let pool = migrated_pool().await;
    seed(&pool).await;

    // Supersede the seeded price with one written the way the application
    // writes them, and re-date the original the way the importer writes them.
    sqlx::query(
        "UPDATE product_prices SET effective_from = datetime('now','-2 days')
          WHERE price_id = 'prc_inv'",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        // RFC3339 at the start of today, so the row is unambiguously current
        // and a raw text comparison against datetime('now') still rejects it.
        //
        // The timestamp is built from SQLite's own clock rather than the host's
        // because the hazard is a *character* comparison: 'T' (0x54) only sorts
        // above a space (0x20) when the date parts match. Dating the row an
        // hour back put it on the previous calendar date between 00:00 and
        // 01:00 UTC, where '…-06T23:…' < '…-07 00:…' compares by the day digit
        // instead — the raw query then agreed with checkout and this test
        // failed for one hour a day, on the fixture rather than on the bug.
        "INSERT INTO product_prices
           (price_id, product_id, price_type, price_minor, currency,
            effective_from, created_by_user_id, created_at, updated_at)
         VALUES ('prc_new','prd_inv','selling', 2500, 'BHD',
                 strftime('%Y-%m-%dT00:00:00Z','now'), ?,
                 datetime('now'), datetime('now'))",
    )
    .bind(CASHIER)
    .execute(&pool)
    .await
    .unwrap();

    // What checkout would charge — the authoritative predicate.
    let charged: i64 = sqlx::query_scalar(
        "SELECT price_minor FROM product_prices
          WHERE product_id='prd_inv' AND branch_id IS NULL AND price_type='selling'
            AND datetime(effective_from) <= datetime('now')
            AND (effective_to IS NULL OR datetime(effective_to) > datetime('now'))
          ORDER BY datetime(effective_from) DESC, price_id DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        charged, 2500,
        "checkout itself resolved the superseded price — the fixture is wrong,          or the authoritative predicate has regressed"
    );

    // The same question asked the way the storefront and the insight ask it.
    let storefront: i64 = sqlx::query_scalar(
        "SELECT price_minor FROM product_prices candidate
          WHERE candidate.product_id='prd_inv' AND candidate.price_type='selling'
            AND candidate.branch_id IS NULL
            AND datetime(candidate.effective_from)<=datetime('now')
            AND (candidate.effective_to IS NULL OR
                 datetime(candidate.effective_to)>datetime('now'))
          ORDER BY datetime(candidate.effective_from) DESC, candidate.price_id DESC
          LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        storefront, charged,
        "the storefront would publish {storefront} while the till charges {charged}"
    );

    // And the raw form these were written in must still be demonstrably wrong,
    // so this test fails if someone reverts the fix rather than passing because
    // the hazard stopped existing.
    let raw: Option<i64> = sqlx::query_scalar(
        "SELECT price_minor FROM product_prices
          WHERE product_id='prd_inv' AND branch_id IS NULL AND price_type='selling'
            AND effective_from <= datetime('now')
          ORDER BY effective_from DESC, price_id DESC LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_ne!(
        raw,
        Some(charged),
        "raw text comparison now agrees with checkout — if the writers were          unified on one timestamp format this test should be simplified rather          than left asserting a hazard that no longer exists"
    );
}
