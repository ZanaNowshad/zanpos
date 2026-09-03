#![cfg(test)]
//! The arithmetic the books have to satisfy, checked against rows the real
//! checkout wrote.
//!
//! Every other test in this crate asserts that a function returned what it was
//! asked for. These assert something different and harder to fake: that after
//! the application has done its ordinary work, the database is internally
//! consistent. A sale whose lines do not add up to its total is not a failing
//! function — every function involved returned `Ok` — it is a shop whose till
//! and whose reports disagree, discovered weeks later.
//!
//! They run against a freshly migrated database and drive `finalize_sale`, the
//! same path a cashier's Enter key takes, rather than inserting rows by hand.
//! Hand-written fixtures can satisfy an invariant that the production writer
//! violates, which is the failure mode this file exists to avoid.

use crate::db::repositories::{sale_repo, shift_repo};
use crate::domain::cart::{Cart, CartLine};
use crate::domain::sale::PaymentInput;
use sqlx::SqlitePool;

const BRANCH_FALLBACK: &str = "01JBRANCH00000000000MAIN1";
const DEVICE: &str = "01JDEVICE0000000000000001";
const CASHIER: &str = "01JUSER000000000000ADMIN1";
const TAX_VAT: &str = "01JTAX000000000000VAT001";

/// A migrated database, built once for the whole test binary and copied.
///
/// Every test here wants its own database, and running all 59 migrations for
/// each of them is most of the wall time — thirteen tests doing it at once
/// turned two seconds apiece into three and a half minutes of disk contention.
/// The migrations are deterministic, so the result is a file, and a file can be
/// copied. Each test still gets a private database; it just does not rebuild the
/// schema to get one.
static TEMPLATE: tokio::sync::OnceCell<std::path::PathBuf> = tokio::sync::OnceCell::const_new();

async fn migrated_template() -> &'static std::path::PathBuf {
    TEMPLATE
        .get_or_init(|| async {
            let path = std::env::temp_dir().join(format!("zanpos_tmpl_{}.db", ulid::Ulid::new()));
            let _ = std::fs::remove_file(&path);
            let pool = SqlitePool::connect(&format!("sqlite:{}?mode=rwc", path.display()))
                .await
                .expect("open template database");
            sqlx::migrate!("./migrations")
                .run(&pool)
                .await
                .expect("migrations apply to an empty database");
            // Fold the WAL back into the main file so a plain copy is complete.
            let _ = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
                .execute(&pool)
                .await;
            pool.close().await;
            path
        })
        .await
}

async fn migrated_pool() -> SqlitePool {
    let template = migrated_template().await;
    let path = std::env::temp_dir().join(format!("zanpos_inv_{}.db", ulid::Ulid::new()));
    std::fs::copy(template, &path).expect("copy the migrated template");
    SqlitePool::connect(&format!("sqlite:{}?mode=rwc", path.display()))
        .await
        .expect("open database")
}

/// A branch, a tax rule, a category, one product with a price and stock.
///
/// Written with plain SQL because these are the shop's *configuration*, not its
/// takings — the point of the fixture is that the takings below are produced by
/// the real checkout, not that every row in the file is.
async fn seed(pool: &SqlitePool) -> String {
    let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(pool)
        .await
        .unwrap_or_else(|_| BRANCH_FALLBACK.to_string());

    sqlx::query(
        "INSERT OR IGNORE INTO tax_rules
           (tax_rule_id, name, rate_basis_points, inclusive, is_active,
            effective_from, created_at, updated_at)
         VALUES (?, 'VAT 10%', 1000, 0, 1, datetime('now'), datetime('now'), datetime('now'))",
    )
    .bind(TAX_VAT)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT OR IGNORE INTO categories (category_id, name, created_at, updated_at)
         VALUES ('cat_inv','Grocery', datetime('now'), datetime('now'))",
    )
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT OR IGNORE INTO products
           (product_id, category_id, name, track_inventory, is_active, tax_rule_id,
            created_at, updated_at)
         VALUES ('prd_inv','cat_inv','Cola 330ml', 1, 1, ?, datetime('now'), datetime('now'))",
    )
    .bind(TAX_VAT)
    .execute(pool)
    .await
    .unwrap();

    // The authoritative selling price. Checkout reads this record, not any
    // column on `products` — there is no price column on `products`.
    sqlx::query(
        "INSERT OR IGNORE INTO product_prices
           (price_id, product_id, price_type, price_minor, currency,
            effective_from, created_by_user_id, created_at, updated_at)
         VALUES ('prc_inv','prd_inv','selling', 1000, 'BHD',
                 datetime('now','-1 day'), ?, datetime('now'), datetime('now'))",
    )
    .bind(CASHIER)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT OR IGNORE INTO stock_levels
           (stock_level_id, product_id, branch_id, quantity_on_hand, created_at, updated_at)
         VALUES ('SL-prd_inv', 'prd_inv', ?, '100', datetime('now'), datetime('now'))",
    )
    .bind(&branch)
    .execute(pool)
    .await
    .unwrap();

    branch
}

fn cola(qty: &str) -> CartLine {
    CartLine::new(
        Some("prd_inv".into()),
        "Cola 330ml".into(),
        None,
        None,
        qty,
        1000,
        TAX_VAT.into(),
        1000,
        false,
    )
}

/// Open a shift and ring up one sale through `finalize_sale`.
async fn one_real_sale(pool: &SqlitePool, qty: &str, idem: &str) -> (String, String) {
    let branch = seed(pool).await;
    let shift = shift_repo::open_shift(pool, &branch, DEVICE, CASHIER, 10_000)
        .await
        .expect("open shift");

    let mut cart = Cart::new(
        branch.clone(),
        DEVICE.into(),
        shift.shift_id.clone(),
        CASHIER.into(),
    );
    cart.lines.push(cola(qty));
    let due: i64 = cart.lines.iter().map(|l| l.line_total_minor).sum();

    let result = sale_repo::finalize_sale(
        pool,
        &cart,
        vec![PaymentInput {
            method: "cash".into(),
            amount_minor: due,
            tendered_minor: Some(due),
            external_reference: None,
        }],
        idem,
        None,
        false,
        None,
        false,
    )
    .await
    .expect("checkout must complete");

    (result.sale_id, shift.shift_id)
}

mod backup;
mod catalog;
mod drawer;
mod integrity;
mod lifecycle;
mod loyalty;
mod money;
mod perf;
mod purchasing;
mod reversal;
mod shift;
mod stock;
mod tender;

/// Record the approval a manager gives at the till before a discount is allowed.
///
/// `finalize_sale` will not accept a discount that has no row here: the cart
/// arrives over IPC, so the discount fields on it are a claim, not an
/// authorisation. `pos_apply_bill_discount` and `pos_apply_line_discount` write
/// this row after checking the manager's permission; tests that need a
/// discounted basket stand in for that step.
pub async fn approve_discount(pool: &SqlitePool, cart_id: &str, cart_line_id: &str, minor: i64) {
    sqlx::query(
        "INSERT INTO pos_discount_authorizations
           (cart_id, cart_line_id, discount_minor, reason, authorized_by_user_id, created_at)
         VALUES (?, ?, ?, 'approved in test', ?, datetime('now'))",
    )
    .bind(cart_id)
    .bind(cart_line_id)
    .bind(minor)
    .bind(CASHIER)
    .execute(pool)
    .await
    .expect("record the discount approval");
}
