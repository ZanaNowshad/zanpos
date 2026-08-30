//! One description of every synced table, and what may be assumed about it.
//!
//! `SYNC_TABLES` and `CONSISTENCY_TABLES` were separately maintained lists, and
//! they had drifted: 28 tables synced, 18 were parity-checked. The ten that fell
//! through were `sales`, `sale_items`, `payments`, `refunds`, `refund_items`,
//! `shifts`, `cash_events`, `delivery_orders`, `audit_logs` and
//! `product_cost_history` — which is to say a terminal could be missing an
//! entire day of takings and `hub_truth_compare` would answer "100%".
//!
//! That failure class is structural, not a slip. Two hand-kept lists of the same
//! thing will always diverge eventually, so there is now one list and the others
//! are derived from it. [`tests`] asserts that every synced table is either
//! parity-checked or carries a written exemption, so the next table added is
//! covered by construction or fails the build.
//!
//! The registry also records what each table means by "deleted", because that
//! turned out not to be uniform either — see [`Deletion`].

/// How a table takes part in parity checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parity {
    /// Every row is compared. Correct for anything a shop reads decisions from.
    Full,
    /// Deliberately not compared, for a reason that is written down and tested.
    Exempt(&'static str),
}

/// What "deleted" means for this table, which is not the same everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deletion {
    /// Rows are never removed. Append-only ledgers: sales, payments, audit.
    Never,
    /// Retired by clearing `is_active`, with no `deleted_at` at all. The flag
    /// syncs like any other column, so retirement reaches every terminal.
    DeactivateOnly,
    /// `deleted_at` is set *and* `is_active` is cleared. Belt and braces: even
    /// before the tombstone synced, these rows disappeared elsewhere because
    /// every read filters on both columns.
    SoftDeleteWithActiveFlag,
    /// `deleted_at` is the only marker — there is no `is_active` to fall back
    /// on, so the tombstone has to travel or the deletion does not exist
    /// anywhere else. It did not, until `deleted_at` was taken out of
    /// `should_skip_column`: a customer deleted at the back office stayed live
    /// on every till, and the parity checksum used the same skip list, so
    /// nothing could see it. Covered now by `sync_v2::tombstone_tests`.
    SoftDeleteOnly,
}

/// How a table's outbound work is tracked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Queue {
    /// Per-row `sync_status`/`sync_attempts`: the row itself records that it is
    /// waiting to go up. Everything a shop writes works this way.
    PerRow,
    /// No per-row bookkeeping. `app_config` is the only one — it is pushed
    /// wholesale through an allowlist of keys, so there is nothing to count and
    /// `SELECT ... WHERE sync_status='pending'` on it is an error, not a zero.
    ///
    /// That distinction is why the "pending" lists are not simply the table
    /// list: a query against a table with no such column returns an error which
    /// `unwrap_or(0)` then hides. Naming it here is what stops the next person
    /// "fixing" the difference by adding the table back.
    Wholesale,
}

#[derive(Debug, Clone, Copy)]
pub struct SyncTable {
    pub name: &'static str,
    pub primary_key: &'static str,
    pub parity: Parity,
    pub deletion: Deletion,
    pub queue: Queue,
}

/// Every table the sync protocol may read or write. The hub rejects any other
/// name, and parity coverage is derived from this list rather than repeated.
pub const TABLES: &[SyncTable] = &[
    // ── Reference data: what the shop is configured to be ────────────────────
    t(
        "branches",
        "branch_id",
        Parity::Full,
        Deletion::SoftDeleteWithActiveFlag,
    ),
    t("roles", "role_id", Parity::Full, Deletion::Never),
    t(
        "users",
        "user_id",
        Parity::Full,
        Deletion::SoftDeleteWithActiveFlag,
    ),
    t(
        "devices",
        "device_id",
        Parity::Full,
        Deletion::SoftDeleteWithActiveFlag,
    ),
    t(
        "categories",
        "category_id",
        Parity::Full,
        Deletion::SoftDeleteWithActiveFlag,
    ),
    t(
        "tax_rules",
        "tax_rule_id",
        Parity::Full,
        Deletion::SoftDeleteWithActiveFlag,
    ),
    t("app_config", "key", Parity::Full, Deletion::Never),
    // ── Catalogue: what the tills sell ───────────────────────────────────────
    t(
        "products",
        "product_id",
        Parity::Full,
        Deletion::SoftDeleteWithActiveFlag,
    ),
    // Soft-delete-only, and it was Deletion::Never while three code paths
    // hard-deleted from it. With no is_active to fall back on, a removed
    // barcode had no marker at all and the hub handed its copy back.
    t(
        "product_barcodes",
        "barcode",
        Parity::Full,
        Deletion::SoftDeleteOnly,
    ),
    t("product_prices", "price_id", Parity::Full, Deletion::Never),
    t(
        "product_cost_history",
        "cost_history_id",
        Parity::Full,
        Deletion::Never,
    ),
    // ── Stock ────────────────────────────────────────────────────────────────
    // `stock_levels` is a cache, not a fact: every terminal derives it from the
    // movement ledger. Comparing derived caches reports divergence whenever a
    // movement is in flight, which is noise rather than signal — and it would
    // invite someone to "repair" the cache instead of the ledger underneath it.
    // The ledger is what determines stock, so the ledger is what gets checked.
    t(
        "stock_levels",
        "stock_level_id",
        Parity::Exempt("derived from the stock_movements ledger, which is parity-checked instead"),
        Deletion::Never,
    ),
    t(
        "stock_movements",
        "movement_id",
        Parity::Full,
        Deletion::Never,
    ),
    // ── People and partners ──────────────────────────────────────────────────
    // Soft-delete-only: a removed customer stays visible on other terminals.
    t(
        "customers",
        "customer_id",
        Parity::Full,
        Deletion::SoftDeleteOnly,
    ),
    // The loyalty ledger. Points are derived from this the way stock is derived
    // from movements, so this is the table that has to agree between terminals —
    // `customers.loyalty_points` is a cache and no longer travels.
    t(
        "loyalty_events",
        "loyalty_event_id",
        Parity::Full,
        Deletion::Never,
    ),
    t(
        "suppliers",
        "supplier_id",
        Parity::Full,
        Deletion::DeactivateOnly,
    ),
    t(
        "riders",
        "rider_id",
        Parity::Full,
        Deletion::SoftDeleteWithActiveFlag,
    ),
    // ── Purchasing ───────────────────────────────────────────────────────────
    t("purchase_orders", "po_id", Parity::Full, Deletion::Never),
    t(
        "purchase_order_lines",
        "po_line_id",
        Parity::Full,
        Deletion::Never,
    ),
    t("po_receipts", "receipt_id", Parity::Full, Deletion::Never),
    // ── Money. None of these was parity-checked before ───────────────────────
    // A till missing a day of sales must never be able to report 100%, which is
    // exactly what the old hand-kept list allowed.
    t("sales", "sale_id", Parity::Full, Deletion::Never),
    t("sale_items", "sale_item_id", Parity::Full, Deletion::Never),
    t("payments", "payment_id", Parity::Full, Deletion::Never),
    t("refunds", "refund_id", Parity::Full, Deletion::Never),
    t(
        "refund_items",
        "refund_item_id",
        Parity::Full,
        Deletion::Never,
    ),
    t(
        "cash_events",
        "cash_event_id",
        Parity::Full,
        Deletion::Never,
    ),
    t("shifts", "shift_id", Parity::Full, Deletion::SoftDeleteOnly),
    // ── Operations ───────────────────────────────────────────────────────────
    t(
        "delivery_orders",
        "delivery_id",
        Parity::Full,
        Deletion::Never,
    ),
    // ── The one genuine exemption ────────────────────────────────────────────
    // Each device writes its own hash-linked audit chain and only ever appends
    // to it, so two terminals hold different, equally correct chains and a
    // whole-table checksum would report permanent divergence. Chain integrity is
    // verified on its own terms by `get_audit_chain_status`, which is the check
    // that actually means something here.
    t(
        "audit_logs",
        "audit_log_id",
        Parity::Exempt("per-device hash chain; verified by get_audit_chain_status instead"),
        Deletion::Never,
    ),
];

const fn t(
    name: &'static str,
    primary_key: &'static str,
    parity: Parity,
    deletion: Deletion,
) -> SyncTable {
    SyncTable {
        name,
        primary_key,
        parity,
        deletion,
        // Every shop-written table carries per-row bookkeeping. The one that
        // does not is spelled out below rather than inferred.
        queue: if matches!(name.as_bytes(), b"app_config") {
            Queue::Wholesale
        } else {
            Queue::PerRow
        },
    }
}

pub fn get(name: &str) -> Option<&'static SyncTable> {
    TABLES.iter().find(|table| table.name == name)
}

/// Tables that take part in parity checking.
pub fn parity_checked() -> Vec<&'static str> {
    TABLES
        .iter()
        .filter(|table| table.parity == Parity::Full)
        .map(|table| table.name)
        .collect()
}

/// Tables that record outbound work per row, so "how much is waiting to go up"
/// can be counted from them.
///
/// This is the list the sync status an operator reads is built from. It used to
/// be kept by hand in two more places and had already drifted: `sync_repo` was
/// missing `product_barcodes`, so a terminal with unsent barcode changes
/// under-reported what it was holding — on the very screen someone consults
/// when they suspect it is holding something.
pub fn row_queued() -> Vec<&'static str> {
    TABLES
        .iter()
        .filter(|table| table.queue == Queue::PerRow)
        .map(|table| table.name)
        .collect()
}

/// Tables whose deletion rests entirely on the `deleted_at` tombstone.
///
/// These have no `is_active` to fall back on, so they are the tables where the
/// tombstone failing to sync means the deletion simply does not exist on any
/// other terminal. Named so reconciliation can treat them with the care that
/// history earns rather than as ordinary content drift.
pub fn tombstone_only() -> Vec<&'static str> {
    TABLES
        .iter()
        .filter(|table| table.deletion == Deletion::SoftDeleteOnly)
        .map(|table| table.name)
        .collect()
}

#[cfg(test)]
mod tests;
