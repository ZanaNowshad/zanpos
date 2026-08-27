//! Finding a customer by whichever name the shop knows them under.
//!
//! Split from `tests.rs` for the 500-line rule. These belong together: they all
//! exercise the shared search predicate, which gained a placeholder when
//! `whatsapp_name` was added — and that placeholder shares its statement with
//! the branch filter.

use crate::commands::customer_commands::customer_list_inner;
use super::tests::{make_pool, BRANCH_A, BRANCH_B, USER_A};
use sqlx::SqlitePool;


async fn seed_with_whatsapp_name(
    pool: &SqlitePool,
    id: &str,
    name: &str,
    whatsapp_name: Option<&str>,
    phone: &str,
) {
    sqlx::query(
        "INSERT INTO customers
           (customer_id, branch_id, name, whatsapp_name, phone, email, loyalty_points,
            notes, origin_device_id, created_at, updated_at, version, sync_status)
         VALUES (?, ?, ?, ?, ?, NULL, 0, NULL, 'dev',
                 datetime('now'), datetime('now'), 1, 'pending')",
    )
    .bind(id)
    .bind(BRANCH_A)
    .bind(name)
    .bind(whatsapp_name)
    .bind(phone)
    .execute(pool)
    .await
    .expect("seed customer with whatsapp name");
}

/// The reported gap. Contact import kept only the customer's own WhatsApp name,
/// so someone this shop saved as "Ali Baqala" could be found only by typing the
/// name they chose for themselves — which nobody behind the counter knows.
/// Both names now reach the database and both are searched.
#[tokio::test]
async fn a_customer_is_found_by_either_name() {
    let pool = make_pool().await;
    seed_with_whatsapp_name(&pool, "cus_ali", "Ali Baqala", Some("Ali ⚡"), "+97333050666").await;

    for query in ["Baqala", "Ali ⚡"] {
        let page = customer_list_inner(&pool, USER_A, query, None, None)
            .await
            .unwrap_or_else(|e| panic!("search '{query}' failed: {e}"));
        assert_eq!(page.items.len(), 1, "'{query}' found nothing");
        // The receipt still carries the shop's own spelling.
        assert_eq!(page.items[0].name, "Ali Baqala");
        assert_eq!(page.total, 1, "the count query disagreed with the page");
    }
}

/// Most customers are entered at the counter and have no WhatsApp name at all.
/// Comparing NULL yields NULL, so a missing value must not stop the row
/// matching on the name it does have.
#[tokio::test]
async fn a_customer_without_a_whatsapp_name_is_still_searchable() {
    let pool = make_pool().await;
    seed_with_whatsapp_name(&pool, "cus_walk", "Walk-in Fatima", None, "+97333050777").await;

    let page = customer_list_inner(&pool, USER_A, "Fatima", None, None)
        .await
        .expect("search failed");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].whatsapp_name, None);
}

/// The branch filter shares its statement with the name predicate, and a bind
/// miscount would push the branch id into a LIKE and quietly widen the search
/// to the whole company. Adding a placeholder is exactly when that breaks.
#[tokio::test]
async fn searching_by_whatsapp_name_stays_inside_the_branch() {
    let pool = make_pool().await;
    seed_with_whatsapp_name(&pool, "cus_here", "Ours", Some("Shared Name"), "+97333050888").await;
    sqlx::query(
        "INSERT INTO customers
           (customer_id, branch_id, name, whatsapp_name, phone, loyalty_points,
            origin_device_id, created_at, updated_at, version, sync_status)
         VALUES ('cus_other', ?, 'Theirs', 'Shared Name', '+97333050999', 0, 'dev',
                 datetime('now'), datetime('now'), 1, 'pending')",
    )
    .bind(BRANCH_B)
    .execute(&pool)
    .await
    .unwrap();

    let page = customer_list_inner(&pool, USER_A, "Shared Name", None, None)
        .await
        .expect("search failed");
    assert_eq!(page.items.len(), 1, "the branch filter was dropped");
    assert_eq!(page.items[0].name, "Ours");
    assert_eq!(page.total, 1);
}
