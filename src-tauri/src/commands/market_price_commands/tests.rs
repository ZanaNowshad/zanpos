//! The upserts, run against the real indexes.
//!
//! Both `product_matches` and `price_watchlist` carry *partial* unique indexes
//! (`WHERE deleted_at IS NULL`). SQLite requires a conflict target to match the
//! index it names, WHERE clause included — an `ON CONFLICT` that omits the
//! predicate, or names the wrong columns, is not a compile error and not a
//! warning. It fails at runtime, the first time two rows collide, which in this
//! feature means the first time an operator confirms the same match twice.

use super::*;
use crate::price_intelligence::matching::{self, Candidate};
use crate::price_intelligence::SourceOffer;
use sqlx::SqlitePool;

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

fn candidate() -> Candidate {
    Candidate {
        source_id: "akelny".into(),
        source_product_key: "listing-1".into(),
        name: "Rainbow Evaporated Milk 160ml".into(),
        pack_text: Some("160ml".into()),
        url: "https://akelny.net/bh/products/rainbow".into(),
        confidence: 92,
        offers: vec![SourceOffer {
            retailer: "Al Osra".into(),
            price_minor: 250,
            in_stock: true,
            url: None,
        }],
    }
}

/// Confirming the same listing twice must update, not explode.
#[tokio::test]
async fn confirming_the_same_match_twice_is_an_update() {
    let pool = pool_with_product().await;

    matching::confirm(&pool, "prd_1", "br_1", "user_1", &candidate())
        .await
        .expect("first confirm");
    matching::confirm(&pool, "prd_1", "br_1", "user_1", &candidate())
        .await
        .expect("second confirm hit the wrong conflict target");

    let stored = matching::for_product(&pool, "prd_1").await.unwrap();
    assert_eq!(stored.len(), 1, "a duplicate pairing was created");
    assert!(stored[0].is_trusted());
}

/// Two different listings for the same product from the same source are two
/// pairings, not a collision — the unique index is on all three columns.
#[tokio::test]
async fn two_listings_from_one_source_are_two_matches() {
    let pool = pool_with_product().await;
    let mut other = candidate();
    other.source_product_key = "listing-2".into();

    matching::confirm(&pool, "prd_1", "br_1", "user_1", &candidate()).await.unwrap();
    matching::confirm(&pool, "prd_1", "br_1", "user_1", &other).await.unwrap();

    assert_eq!(matching::for_product(&pool, "prd_1").await.unwrap().len(), 2);
}

/// A rejected pairing keeps its row, so the same wrong candidate is not offered
/// again next week.
#[tokio::test]
async fn rejecting_keeps_the_row_and_stops_the_trust() {
    let pool = pool_with_product().await;
    let match_id = matching::confirm(&pool, "prd_1", "br_1", "user_1", &candidate())
        .await
        .unwrap();

    matching::reject(&pool, &match_id).await.expect("reject");

    let stored = matching::for_product(&pool, "prd_1").await.unwrap();
    assert_eq!(stored.len(), 1, "the row was deleted instead of marked");
    assert_eq!(stored[0].status, "rejected");
    assert!(!stored[0].is_trusted());
}

#[tokio::test]
async fn rejecting_something_that_is_not_there_says_so() {
    let pool = pool_with_product().await;
    let result = matching::reject(&pool, "no-such-match").await;
    assert!(matches!(result, Err(AppError::NotFound(_))), "{result:?}");
}

/// Same partial-index trap on the watchlist.
#[tokio::test]
async fn watchlisting_twice_is_an_update_and_untracking_removes_it() {
    let pool = pool_with_product().await;

    set_watchlist(&pool, "prd_1", "br_1", "user_1", true).await.expect("track");
    set_watchlist(&pool, "prd_1", "br_1", "user_1", true)
        .await
        .expect("re-track hit the wrong conflict target");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM price_watchlist WHERE product_id='prd_1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);

    set_watchlist(&pool, "prd_1", "br_1", "user_1", false).await.expect("untrack");
    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM price_watchlist WHERE product_id='prd_1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, 0);
}

/// Confirming records the offers already in hand, so the panel is useful
/// immediately rather than after the next refresh cycle — and those prices are
/// now trusted, because a person said so.
#[tokio::test]
async fn confirming_makes_the_price_quotable_at_once() {
    let pool = pool_with_product().await;

    let report = crate::price_intelligence::service::confirm_match(
        &pool, "prd_1", "br_1", "user_1", &candidate(),
    )
    .await
    .expect("confirm");

    assert_eq!(report.summary.retailer_count, 1);
    assert_eq!(report.summary.median_minor, Some(250));
    assert_eq!(report.trusted.len(), 1);
    assert_eq!(report.trusted[0].retailer_name, "Al Osra");
}
