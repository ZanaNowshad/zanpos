//! The report the panel draws, and what it refuses to put in it.

use super::*;

async fn pool_with_catalogue() -> SqlitePool {
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

/// Opening a product form must not wait on somebody else's server.
#[tokio::test]
async fn the_cached_report_touches_no_network() {
    let pool = pool_with_catalogue().await;
    let report = cached_report(&pool, "prd_1").await.expect("cached report");

    assert_eq!(report.product_name, "Rainbow Evaporated Milk 160ml");
    assert_eq!(report.summary.retailer_count, 0);
    assert!(report.candidates.is_empty());
}

#[tokio::test]
async fn an_unknown_product_is_not_found_rather_than_empty() {
    let pool = pool_with_catalogue().await;
    let result = cached_report(&pool, "nope").await;
    assert!(matches!(result, Err(AppError::NotFound(_))), "{result:?}");
}

/// Sources that cannot be reached are reported, not omitted. A thin result must
/// read as "we could not look" rather than "nobody else sells this" — the two
/// look identical on screen and lead to opposite decisions.
#[tokio::test]
async fn a_source_we_cannot_use_is_named_with_its_reason() {
    let pool = pool_with_catalogue().await;
    let statuses = source_statuses(&pool).await.expect("statuses");

    let lulu = statuses.iter().find(|s| s.source_id == "lulu_bh");
    assert!(
        lulu.is_some(),
        "the unsupported source vanished from the list"
    );
    let lulu = lulu.unwrap();
    assert_ne!(lulu.status, "ok");
    assert!(
        lulu.reason.is_some(),
        "no reason given for an unusable source"
    );
    assert_eq!(
        lulu.fallback_source_id.as_deref(),
        Some("akelny"),
        "no fallback offered for a source we cannot read directly"
    );
}

/// Trusted prices and unconfirmed candidates are separate fields, not one list
/// with a flag. A single list is one careless map away from being summed.
#[tokio::test]
async fn trusted_prices_and_candidates_never_share_a_list() {
    let pool = pool_with_catalogue().await;
    let report = cached_report(&pool, "prd_1").await.unwrap();

    // Shapes differ, so they cannot be concatenated by accident.
    let _: &Vec<crate::price_intelligence::observe::Observation> = &report.trusted;
    let _: &Vec<crate::price_intelligence::matching::Candidate> = &report.candidates;
    assert_eq!(report.summary.retailer_count, report.trusted.len());
}
