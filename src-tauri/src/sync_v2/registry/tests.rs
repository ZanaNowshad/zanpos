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
            TABLES
                .iter()
                .find(|t| t.name == "stock_movements")
                .unwrap()
                .parity,
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
    assert_eq!(
        registry, synced,
        "SYNC_TABLES has drifted from the registry"
    );

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

/// These three have no `is_active` to fall back on, so their deletions live or
/// die by the tombstone. That is why `deleted_at` must stay out of both skip
/// lists: dropped from the wire the deletion never travels, and dropped from the
/// fingerprint nothing can see that the terminals disagree.
///
/// `product_barcodes` joined the list when 0058 gave it a `deleted_at`. It had
/// been `Deletion::Never` while three code paths hard-`DELETE`d from it, which
/// is the combination that let a removed barcode come straight back from the
/// hub — no marker to push, so nothing to distinguish "deleted here" from
/// "never seen here".
#[test]
fn the_tables_that_depend_entirely_on_the_tombstone_are_known() {
    assert_eq!(
        tombstone_only(),
        vec!["product_barcodes", "customers", "shifts"]
    );

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

/// Every table that carries per-row sync bookkeeping must actually have the
/// column the bookkeeping is read from.
///
/// `count_pending` and friends run `SELECT COUNT(*) ... WHERE sync_status =
/// 'pending'` and swallow the result with `unwrap_or(0)`. A table wrongly
/// classified `PerRow` therefore reports zero pending forever instead of
/// erroring — silently under-reporting exactly what an operator consults that
/// number to find out.
#[tokio::test]
async fn every_row_queued_table_really_has_sync_status() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    for table in row_queued() {
        let has: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = 'sync_status'"
        ))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(has, 1, "{table} is queued per row but has no sync_status");
    }

    // And the wholesale one genuinely lacks it, so the exception is real rather
    // than someone's guess.
    for table in TABLES.iter().filter(|t| t.queue == Queue::Wholesale) {
        let has: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name = 'sync_status'",
            table.name
        ))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            has, 0,
            "{} is exempt from per-row queueing but has sync_status",
            table.name
        );
    }
}

/// The lists that were kept by hand now derive from here, so this pins that
/// they stayed derived. `sync_repo` was missing `product_barcodes`, which meant
/// unsent barcode changes were absent from the sync status an operator reads.
#[test]
fn the_pending_count_lists_come_from_this_registry() {
    let expected = row_queued();
    assert_eq!(*crate::commands::sync_commands::SYNC_TABLES, expected);
    assert!(expected.contains(&"product_barcodes"));
    assert!(!expected.contains(&"app_config"));
}

/// `sync_commands::table_pk` is a hand-kept copy of `pk_for_table`. It once
/// lacked a `loyalty_events` arm, which sent that table's queue listing,
/// retry, dismiss and conflict-retry down a nonexistent `id` column — and the
/// SQL error was swallowed, so pending loyalty rows were silently invisible.
#[test]
fn the_sync_command_keys_cover_every_queued_table() {
    for table in row_queued() {
        let key = crate::commands::sync_commands::table_pk(table);
        assert_ne!(
            key, "id",
            "{table} fell through table_pk's default — its queue tools would address a column that does not exist"
        );
        assert_eq!(
            key,
            pk_for_table(table),
            "{table} has two different primary keys"
        );
    }
}

/// "Replace this terminal's data with the hub's" deletes children before
/// parents, but it must delete everything — a table left out survives the wipe
/// with rows that are marked synced, so they are neither re-pushed nor
/// re-pulled and can never converge.
#[test]
fn the_join_wipe_covers_every_queued_table() {
    let wipe: std::collections::BTreeSet<&str> =
        crate::commands::sync_commands::JOIN_WIPE_CHILD_FIRST
            .iter()
            .copied()
            .collect();
    let queued: std::collections::BTreeSet<&str> = row_queued().into_iter().collect();
    assert_eq!(
        wipe, queued,
        "the join wipe and the per-row queue list describe different sets of tables"
    );
}

/// Two schema facts applied by hand in `apply.rs`. The pull filter uses
/// `has_origin_device_id` to skip a device's own rows echoing back at it, and
/// the LWW tie-break uses `has_version_column` to pick the more-edited copy of
/// a same-millisecond tie — so a table gaining or losing either column must
/// fail here, at the schema, rather than silently change sync behaviour.
#[tokio::test]
async fn the_origin_and_version_column_lists_match_the_live_schema() {
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

        let has_origin = columns.iter().any(|c| c == "origin_device_id");
        assert_eq!(
            crate::sync_v2::apply::has_origin_device_id(table.name),
            has_origin,
            "{} disagrees with the hand-kept origin_device_id list",
            table.name
        );

        let has_version = columns.iter().any(|c| c == "version");
        assert_eq!(
            crate::sync_v2::apply::has_version_column(table.name),
            has_version,
            "{} disagrees with the hand-kept version list — the LWW tie-break would miscompile SQL",
            table.name
        );
    }
}

/// Push and pull order are hand-sequenced because foreign keys care about
/// order — but they must still cover everything, or a table silently never
/// moves in that direction.
#[test]
fn push_and_pull_cover_every_registered_table() {
    let all: Vec<&str> = TABLES.iter().map(|t| t.name).collect();
    for table in &all {
        // app_config is pushed wholesale by push_app_config, not in the row loop.
        if *table == "app_config" {
            continue;
        }
        assert!(
            crate::sync_v2::worker::PUSH_ORDER.contains(table),
            "{table} is never pushed"
        );
        assert!(
            crate::sync_v2::worker::PULL_ORDER.contains(table),
            "{table} is never pulled"
        );
    }
}

/// A timestamp the hub cannot compare is a row the hub cannot serve.
///
/// The hub selects rows to send with
/// `strftime('%Y-%m-%dT%H:%M:%f', updated_at) > strftime(..., :watermark)`.
/// `strftime` of an empty string is NULL, and `NULL > anything` is NULL, which
/// is not true. So a row whose `updated_at` is `''` is not served late — it is
/// never served, to any terminal, and no amount of resetting watermarks helps,
/// because resetting a watermark does not change a predicate that cannot match.
///
/// Two migrations added the column as `NOT NULL DEFAULT ''`: 0004 for `roles`
/// and 0028 for `product_barcodes`. Neither backfilled new writes, and no insert
/// path set the column, so `product_barcodes` accumulated tens of thousands of
/// rows that could not cross the wire while the table reported as diverging with
/// no explanation of why. 0058 rebuilt the table with a real default and 0059
/// backfilled both.
///
/// This checks the property rather than the two migrations: any synced table
/// that ships a row with a blank timestamp fails here, at the point the seed is
/// written, rather than after a shop spends a week wondering where its barcodes
/// went.
#[tokio::test]
async fn no_synced_row_carries_a_timestamp_the_hub_cannot_compare() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    let mut unservable = Vec::new();
    let mut blank_defaults = Vec::new();

    for table in TABLES {
        let has_updated_at: bool = sqlx::query_scalar(&format!(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('{}') WHERE name = 'updated_at')",
            table.name
        ))
        .fetch_one(&pool)
        .await
        .unwrap();
        if !has_updated_at {
            continue;
        }

        let blank_rows: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM {} WHERE TRIM(COALESCE(updated_at, '')) = ''",
            table.name
        ))
        .fetch_one(&pool)
        .await
        .unwrap();
        if blank_rows > 0 {
            unservable.push(format!("{} ({blank_rows} rows)", table.name));
        }

        let default: Option<String> = sqlx::query_scalar(&format!(
            "SELECT dflt_value FROM pragma_table_info('{}') WHERE name = 'updated_at'",
            table.name
        ))
        .fetch_one(&pool)
        .await
        .unwrap();
        if default.as_deref() == Some("''") {
            blank_defaults.push(table.name);
        }
    }

    assert!(
        unservable.is_empty(),
        "these rows ship with a timestamp the hub cannot compare, so they can never \
         be pulled by any terminal: {unservable:?}"
    );

    // `roles` keeps the empty default from 0004 because changing it needs a table
    // rebuild and `users.role_id` references it. That is safe only because the
    // three rows are seeded once in 0001 and nothing inserts a role at runtime —
    // 0059 gives those three deterministic timestamps, identical on every
    // terminal. Add a runtime insert path and the assertion above starts failing,
    // which is the intended order of events.
    assert_eq!(
        blank_defaults,
        vec!["roles"],
        "a table gained a blank updated_at default; a row written without setting \
         the column explicitly will never reach another terminal"
    );
}

#[test]
fn hub_owned_heartbeat_evidence_never_enters_generic_device_replication() {
    for column in [
        "last_heartbeat_at",
        "last_seen_at",
        "observed_ip",
        "app_version",
        "heartbeat_seq",
        "heartbeat_hub_id",
    ] {
        assert!(
            crate::sync_v2::apply::skip_on_wire("devices", column),
            "{column} could be forged or made stale by a generic device push"
        );
        assert!(
            crate::sync_v2::apply::skip_in_fingerprint("devices", column),
            "hub-local liveness evidence must not create permanent parity drift"
        );
    }
}
