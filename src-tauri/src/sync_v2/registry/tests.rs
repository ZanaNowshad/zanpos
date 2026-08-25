use super::*;
use crate::sync_v2::apply::{pk_for_table, SYNC_TABLES};
use crate::sync_v2::consistency::CONSISTENCY_TABLES;

/// The failure this registry exists to make impossible.
///
/// Two hand-kept lists of the same thing drifted to 28 against 18, and the ten
/// that fell through the gap included `sales`, `payments` and `refunds` — so a
/// terminal missing a day of takings reported "100% consistent". A new table is
/// now covered by construction or fails here.
#[test]
fn every_synced_table_is_parity_checked_or_carries_a_written_exemption() {
    let mut unexplained = Vec::new();
    for table in TABLES {
        match table.parity {
            Parity::Full => {}
            Parity::Exempt(reason) => {
                // An exemption with no reason is a hole with a label on it.
                assert!(
                    reason.len() > 20,
                    "{} is exempt without a real reason: {reason:?}",
                    table.name
                );
            }
        }
        if matches!(table.parity, Parity::Exempt(r) if r.is_empty()) {
            unexplained.push(table.name);
        }
    }
    assert!(unexplained.is_empty(), "{unexplained:?}");

    // Exactly two tables may be exempt, and both are exempt because comparing
    // them across terminals is meaningless rather than because checking them is
    // inconvenient. Anything else appearing here is a coverage regression, not a
    // design decision — which is what this assertion is for.
    //
    //   audit_logs   — each device appends to its own hash chain, so two
    //                  terminals legitimately hold different, equally correct
    //                  chains. Verified by get_audit_chain_status instead.
    //   stock_levels — a cache every terminal derives from the movement ledger.
    //                  It differs whenever a movement is in flight, and
    //                  "repairing" it would paper over the ledger underneath.
    //                  stock_movements stays Parity::Full, which is the thing
    //                  that actually determines stock.
    let exempt: Vec<&str> = TABLES
        .iter()
        .filter(|t| !matches!(t.parity, Parity::Full))
        .map(|t| t.name)
        .collect();
    assert_eq!(
        exempt,
        vec!["stock_levels", "audit_logs"],
        "unexpected parity exemption"
    );

    // The ledger behind the derived cache must never itself become exempt.
    assert!(
        matches!(
            TABLES.iter().find(|t| t.name == "stock_movements").unwrap().parity,
            Parity::Full
        ),
        "stock_movements is the source of truth for stock and must stay checked"
    );
}

/// The registry is the source; the two legacy constants are derived from it.
/// If they can drift apart again, nothing above helps.
#[test]
fn the_derived_lists_match_the_registry_exactly() {
    let registry: std::collections::BTreeSet<&str> = TABLES.iter().map(|t| t.name).collect();
    let synced: std::collections::BTreeSet<&str> = SYNC_TABLES.iter().copied().collect();
    assert_eq!(registry, synced, "SYNC_TABLES has drifted from the registry");

    let parity: std::collections::BTreeSet<&str> = parity_checked().into_iter().collect();
    let consistency: std::collections::BTreeSet<&str> =
        CONSISTENCY_TABLES.iter().copied().collect();
    assert_eq!(
        parity, consistency,
        "CONSISTENCY_TABLES has drifted from the registry"
    );
}

/// Money was the whole point. These were the tables a shop would notice missing
/// and the ones the old list omitted.
#[test]
fn the_financial_tables_are_parity_checked() {
    for table in [
        "sales",
        "sale_items",
        "payments",
        "refunds",
        "refund_items",
        "cash_events",
        "shifts",
        "product_cost_history",
        "delivery_orders",
    ] {
        let entry = get(table).unwrap_or_else(|| panic!("{table} missing from the registry"));
        assert_eq!(entry.parity, Parity::Full, "{table} is not parity-checked");
        assert!(
            CONSISTENCY_TABLES.contains(&table),
            "{table} did not reach CONSISTENCY_TABLES"
        );
    }
}

/// Parity compares rows keyed by primary key, so a wrong key silently compares
/// the wrong things — or, for a table whose key falls back to "id", nothing.
#[test]
fn every_primary_key_agrees_with_the_one_sync_uses() {
    for table in TABLES {
        assert_eq!(
            table.primary_key,
            pk_for_table(table.name),
            "{} has two different primary keys",
            table.name
        );
        assert_ne!(
            table.primary_key, "id",
            "{} fell through to the default key",
            table.name
        );
    }
}

/// These two have no `is_active` to fall back on, so their deletions live or
/// die by the tombstone. That is why `deleted_at` must stay out of both skip
/// lists: dropped from the wire the deletion never travels, and dropped from the
/// fingerprint nothing can see that the terminals disagree.
#[test]
fn the_tables_that_depend_entirely_on_the_tombstone_are_known() {
    assert_eq!(tombstone_only(), vec!["customers", "shifts"]);

    for table in tombstone_only() {
        assert!(
            !crate::sync_v2::apply::skip_on_wire(table, "deleted_at"),
            "{table} would lose its only deletion marker in transit"
        );
        assert!(
            !crate::sync_v2::apply::skip_in_fingerprint(table, "deleted_at"),
            "{table} could be deleted on one side with parity still reporting 100%"
        );
    }
}

/// A table claiming an `is_active` fallback must actually have the column, or
/// the reasoning behind calling its deletions safe does not hold.
#[tokio::test]
async fn tables_claiming_an_active_flag_really_have_one() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    for table in TABLES {
        let columns: Vec<String> = sqlx::query_scalar(&format!(
            "SELECT name FROM pragma_table_info('{}')",
            table.name
        ))
        .fetch_all(&pool)
        .await
        .unwrap();

        match table.deletion {
            Deletion::SoftDeleteWithActiveFlag => {
                assert!(
                    columns.iter().any(|c| c == "is_active"),
                    "{} claims an is_active fallback it does not have",
                    table.name
                );
                assert!(
                    columns.iter().any(|c| c == "deleted_at"),
                    "{} claims soft deletion without deleted_at",
                    table.name
                );
            }
            Deletion::SoftDeleteOnly => {
                assert!(
                    columns.iter().any(|c| c == "deleted_at"),
                    "{} claims soft deletion without deleted_at",
                    table.name
                );
                assert!(
                    !columns.iter().any(|c| c == "is_active"),
                    "{} has is_active and should be SoftDeleteWithActiveFlag",
                    table.name
                );
            }
            Deletion::DeactivateOnly => {
                assert!(
                    columns.iter().any(|c| c == "is_active"),
                    "{} is deactivate-only without an is_active column",
                    table.name
                );
                assert!(
                    !columns.iter().any(|c| c == "deleted_at"),
                    "{} has deleted_at and should be a soft-delete strategy",
                    table.name
                );
            }
            Deletion::Never => {
                assert!(
                    !columns.iter().any(|c| c == "deleted_at"),
                    "{} has deleted_at but is declared append-only",
                    table.name
                );
            }
        }
    }
}
