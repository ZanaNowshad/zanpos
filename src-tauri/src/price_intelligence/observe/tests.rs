//! The rule the whole feature rests on, tested where it is enforced.

use super::*;
use crate::price_intelligence::matching::MatchMethod;

async fn pool_with_product() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO categories (category_id, name, created_at, updated_at)
         VALUES ('cat_1','Grocery','2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO products (product_id, category_id, name, is_active, created_at, updated_at)
         VALUES ('prd_1','cat_1','Rainbow Evaporated Milk 160ml',1,
                 '2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

/// Insert a match in a given trust state and hang one price on it.
async fn seed_match(pool: &SqlitePool, id: &str, method: MatchMethod, status: &str, price: i64) {
    sqlx::query(
        "INSERT INTO product_matches
            (match_id, product_id, source_id, source_product_key, source_product_name,
             match_method, status, created_at, updated_at)
         VALUES (?, 'prd_1', ?, ?, 'Rainbow 160ml', ?, ?,
                 '2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
    )
    .bind(id)
    .bind(format!("src_{id}"))
    .bind(format!("key_{id}"))
    .bind(method.as_str())
    .bind(status)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO price_observations
            (observation_id, match_id, retailer_name, price_minor, in_stock, observed_at)
         VALUES (?, ?, ?, ?, 1, '2026-08-02T10:00:00Z')",
    )
    .bind(format!("obs_{id}"))
    .bind(id)
    .bind(format!("Retailer {id}"))
    .bind(price)
    .execute(pool)
    .await
    .unwrap();
}

/// The invariant, stated directly: a guess contributes nothing.
///
/// Offering a candidate that turns out wrong wastes ten seconds. Feeding that
/// guess into a median a manager then prices against moves real money, quietly,
/// every week until somebody notices the number was never about their product.
#[tokio::test]
async fn an_unconfirmed_match_feeds_no_figure() {
    let pool = pool_with_product().await;
    seed_match(&pool, "m_fuzzy", MatchMethod::FuzzyCandidate, "active", 999).await;

    let summary = market_summary(&pool, "prd_1").await.unwrap();
    assert_eq!(summary.retailer_count, 0);
    assert_eq!(summary.low_minor, None);
    assert_eq!(summary.median_minor, None);
    assert_eq!(summary.high_minor, None);
    assert!(latest_trusted_prices(&pool, "prd_1").await.unwrap().is_empty());
    assert!(history(&pool, "prd_1", 50).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_confirmed_match_does_feed_the_figures() {
    let pool = pool_with_product().await;
    seed_match(&pool, "m_ok", MatchMethod::OperatorConfirmed, "active", 250).await;

    let summary = market_summary(&pool, "prd_1").await.unwrap();
    assert_eq!(summary.retailer_count, 1);
    assert_eq!(summary.median_minor, Some(250));
}

/// A confirmation is a statement about a product, and pack size is part of what
/// makes it that product. Once suspended it stops being evidence.
#[tokio::test]
async fn a_match_suspended_for_a_pack_change_stops_counting() {
    let pool = pool_with_product().await;
    seed_match(&pool, "m_pack", MatchMethod::OperatorConfirmed, "pack_changed", 250).await;

    assert_eq!(market_summary(&pool, "prd_1").await.unwrap().retailer_count, 0);
}

#[tokio::test]
async fn a_rejected_match_stops_counting() {
    let pool = pool_with_product().await;
    seed_match(&pool, "m_no", MatchMethod::OperatorConfirmed, "rejected", 250).await;

    assert_eq!(market_summary(&pool, "prd_1").await.unwrap().retailer_count, 0);
}

/// Mixed evidence is the realistic case: the guess must not move the median.
#[tokio::test]
async fn a_guess_alongside_real_prices_does_not_shift_the_median() {
    let pool = pool_with_product().await;
    seed_match(&pool, "m_a", MatchMethod::OperatorConfirmed, "active", 200).await;
    seed_match(&pool, "m_b", MatchMethod::BarcodeExact, "active", 300).await;
    // A wildly wrong guess that would drag the median if it counted.
    seed_match(&pool, "m_junk", MatchMethod::FuzzyCandidate, "active", 9_000).await;

    let summary = market_summary(&pool, "prd_1").await.unwrap();
    assert_eq!(summary.retailer_count, 2);
    assert_eq!(summary.low_minor, Some(200));
    assert_eq!(summary.high_minor, Some(300));
    // 250 — `money::median_minor` averages the two middles and rounds down, so
    // a suggested price is never above something actually seen on a shelf. That
    // decision lives in one place with its own test; this asserts the shared
    // function is the one being used, not a second median with its own opinion.
    assert_eq!(summary.median_minor, Some(250));
}

/// One retailer refreshed daily for a month must not become thirty votes.
#[tokio::test]
async fn a_retailer_counts_once_however_often_it_was_read() {
    let pool = pool_with_product().await;
    seed_match(&pool, "m_r", MatchMethod::OperatorConfirmed, "active", 250).await;
    for (n, day) in ["03", "04", "05"].iter().enumerate() {
        sqlx::query(
            "INSERT INTO price_observations
                (observation_id, match_id, retailer_name, price_minor, in_stock, observed_at)
             VALUES (?, 'm_r', 'Retailer m_r', ?, 1, ?)",
        )
        .bind(format!("obs_extra_{n}"))
        .bind(260 + n as i64)
        .bind(format!("2026-08-{day}T10:00:00Z"))
        .execute(&pool)
        .await
        .unwrap();
    }

    let summary = market_summary(&pool, "prd_1").await.unwrap();
    assert_eq!(summary.retailer_count, 1, "one shop voted more than once");
    // And it is the latest reading, not the first.
    assert_eq!(summary.median_minor, Some(262));
}

/// An out-of-stock listing keeps its last price on the page long after the
/// shelf changed. It is not a price anyone can go and pay today.
#[tokio::test]
async fn an_out_of_stock_price_is_not_quoted() {
    let pool = pool_with_product().await;
    seed_match(&pool, "m_s", MatchMethod::OperatorConfirmed, "active", 250).await;
    sqlx::query("UPDATE price_observations SET in_stock = 0 WHERE match_id = 'm_s'")
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(market_summary(&pool, "prd_1").await.unwrap().retailer_count, 0);
}

#[tokio::test]
async fn a_product_nobody_has_priced_reports_nothing_rather_than_zero() {
    let pool = pool_with_product().await;
    let summary = market_summary(&pool, "prd_1").await.unwrap();

    assert_eq!(summary.retailer_count, 0);
    assert_eq!(summary.low_minor, None, "absent must not read as free");
}
