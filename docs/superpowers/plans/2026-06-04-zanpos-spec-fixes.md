# ZANPOS Spec Fixes — Full Implementation Plan
> **Status:** EXECUTED & SUPERSEDED (2026-06-05). All 74 deviations fixed. The outbox::enqueue_* calls referenced in this plan no longer exist — replaced by sync_status dirty-flag tracking in sync_v2.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix all 74 deviations found in the ZANPOS v1.0.24 spec audit, in dependency order, leaving cargo check + cargo test + npx tsc --noEmit green after every task group.

**Architecture:** Fixes are grouped by file/module to minimise context switching. Foundational data-integrity fixes (audit chain, outbox atomicity, retention guards) come first. POS/catalog/inventory mutations follow. UI and observability last.

**Tech Stack:** Tauri v2, Rust / sqlx / SQLite WAL, React 18 / TypeScript, rust_decimal for decimal arithmetic.

---

## File Map

| File | Tasks |
|---|---|
| `src-tauri/src/sync/worker.rs` | T1, T1 (retention) |
| `src-tauri/src/db/repositories/audit_hash.rs` | T2 |
| `src-tauri/src/sync/outbox.rs` | T3, T7 |
| `src-tauri/src/commands/inventory_commands.rs` | T4 |
| `src-tauri/src/commands/pos_commands.rs` | T5 |
| `src-tauri/src/commands/admin_commands.rs` | T6 |
| `src-tauri/src/commands/customer_commands.rs` | T8 |
| `src-tauri/src/db/repositories/delivery_repo.rs` | T9 |
| `src-tauri/src/commands/delivery_commands.rs` | T9 |
| `src-tauri/src/commands/phase10a_commands.rs` | T10 |
| `src-tauri/src/commands/report_commands.rs` | T10 |
| `src-tauri/src/lib.rs` | T10 (Z-report) |
| `src-tauri/src/commands/whatsapp_commands.rs` | T11 |
| `src-tauri/src/commands/setup_commands.rs` | T12 |
| `src-tauri/src/commands/sync_commands.rs` | T12 |
| `src-tauri/src/sync/central_schema.rs` | T13 |
| `src-tauri/src/secure_store.rs` | T13 |
| `src/components/SyncChip.tsx` | T14 |
| `src-tauri/src/commands/ai_admin_commands.rs` | T15 |
| `src-tauri/src/db/repositories/ai_admin_repo.rs` | T15 |
| `src-tauri/src/commands/migration_commands.rs` | T15 |
| `src-tauri/src/inventory/stock_repo.rs` | T16 |
| `src-tauri/src/commands/cash_commands.rs` | T16 |
| `src-tauri/src/db/repositories/shift_repo.rs` | T16 |
| `src-tauri/src/commands/override_token.rs` | T16 |
| `src-tauri/src/sync/supabase_client.rs` | T16 |
| `src-tauri/src/db/repositories/product_repo.rs` | T16 |

---

## Task 1 — Sync worker: constants + retention prune guards

**Fixes:** H-19, H-20, C-5, C-6

**Files:** `src-tauri/src/sync/worker.rs`

- [ ] **Step 1: Fix BATCH_SIZE and INTERVAL_SECS**

In `worker.rs`, lines 11–12, change:
```rust
const BATCH_SIZE: i64 = 250;
const INTERVAL_SECS: u64 = 20;
```
to:
```rust
const BATCH_SIZE: i64 = 50;
const INTERVAL_SECS: u64 = 30;
```

- [ ] **Step 2: Guard audit_log DELETE with sync_status check**

Find the line (around line 160):
```rust
let _ = sqlx::query("DELETE FROM audit_logs WHERE created_at < ?")
    .bind(&log_cutoff)
    .execute(&self.pool)
    .await;
```
Replace with:
```rust
let _ = sqlx::query(
    "DELETE FROM audit_logs WHERE created_at < ? AND sync_status = 'synced'",
)
.bind(&log_cutoff)
.execute(&self.pool)
.await;
```

- [ ] **Step 3: Guard stock_movements DELETE with sync_status check**

Find (around line 165):
```rust
let _ = sqlx::query("DELETE FROM stock_movements WHERE created_at < ?")
    .bind(&log_cutoff)
    .execute(&self.pool)
    .await;
```
Replace with:
```rust
let _ = sqlx::query(
    "DELETE FROM stock_movements WHERE created_at < ? AND sync_status = 'synced'",
)
.bind(&log_cutoff)
.execute(&self.pool)
.await;
```

- [ ] **Step 4: Verify**
```bash
cd zanpos/src-tauri && cargo check 2>&1 | tail -5
```
Expected: no errors.

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/sync/worker.rs
git commit -m "fix: sync batch=50, interval=30s, guard prune on sync_status"
```

---

## Task 2 — Audit hash chain: add before_json, reason, actor_type

**Fixes:** C-4, H-24, M-19

**Files:** `src-tauri/src/db/repositories/audit_hash.rs`

> NOTE: Adding `before_json` and `reason` to the hash formula changes how new hashes are computed. Existing rows in audit_logs that used the old formula will be detected by `verify_chain` as `broken_hash`. This is the correct behaviour — they are pre-upgrade rows and are treated as a legacy genesis block. Only rows written after this upgrade will form a fully verifiable chain including `before_json` and `reason`.

- [ ] **Step 1: Extend AuditHashInput struct**

Replace the existing `AuditHashInput` struct definition:
```rust
pub struct AuditHashInput<'a> {
    pub audit_log_id: &'a str,
    pub event_type: &'a str,
    pub entity_type: &'a str,
    pub entity_id: &'a str,
    pub actor_user_id: &'a str,
    pub actor_type: &'a str,   // NEW — "user" or "ai"
    pub created_at: &'a str,
    pub before_json: Option<&'a str>, // NEW — pre-mutation snapshot
    pub after_json: Option<&'a str>,
    pub reason: Option<&'a str>,      // NEW — human-readable reason
    pub previous_hash: &'a str,
}
```

- [ ] **Step 2: Update compute_audit_hash to include new fields**

Replace the `compute_audit_hash` function body:
```rust
pub fn compute_audit_hash(i: &AuditHashInput<'_>) -> String {
    let mut h = Sha256::new();
    h.update(i.audit_log_id.as_bytes());   h.update(b"\x00");
    h.update(i.event_type.as_bytes());      h.update(b"\x00");
    h.update(i.entity_type.as_bytes());     h.update(b"\x00");
    h.update(i.entity_id.as_bytes());       h.update(b"\x00");
    h.update(i.actor_user_id.as_bytes());   h.update(b"\x00");
    h.update(i.actor_type.as_bytes());      h.update(b"\x00");
    h.update(i.created_at.as_bytes());      h.update(b"\x00");
    h.update(i.before_json.unwrap_or("").as_bytes()); h.update(b"\x00");
    h.update(i.after_json.unwrap_or("").as_bytes());  h.update(b"\x00");
    h.update(i.reason.unwrap_or("").as_bytes());      h.update(b"\x00");
    h.update(i.previous_hash.as_bytes());
    hex::encode(h.finalize())
}
```

- [ ] **Step 3: Update insert_audit_entry signature and INSERT**

Replace `insert_audit_entry` and `insert_audit_entry_override` with a single comprehensive function:

```rust
/// Write one audit_log row with a proper SHA-256 hash chain.
///
/// `before_json` — pre-mutation snapshot (None for create events)
/// `after_json`  — post-mutation snapshot (None for delete events)
/// `reason`      — human-readable reason for the change (None if not applicable)
/// `actor_type`  — "user" or "ai"
/// `override_used` — true when a manager-override token was consumed
#[allow(clippy::too_many_arguments)]
pub async fn insert_audit_entry(
    pool: &SqlitePool,
    event_type: &str,
    entity_type: &str,
    entity_id: &str,
    actor_user_id: &str,
    actor_type: &str,
    device_id: &str,
    branch_id: &str,
    before_json: Option<&str>,
    after_json: Option<&str>,
    reason: Option<&str>,
) -> AppResult<()> {
    insert_audit_entry_override(
        pool, event_type, entity_type, entity_id,
        actor_user_id, actor_type, device_id, branch_id,
        before_json, after_json, reason, false,
    ).await
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_audit_entry_override(
    pool: &SqlitePool,
    event_type: &str,
    entity_type: &str,
    entity_id: &str,
    actor_user_id: &str,
    actor_type: &str,
    device_id: &str,
    branch_id: &str,
    before_json: Option<&str>,
    after_json: Option<&str>,
    reason: Option<&str>,
    override_used: bool,
) -> AppResult<()> {
    let audit_log_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let prev_hash = fetch_last_hash(pool, device_id).await.unwrap_or_default();
    let hash = compute_audit_hash(&AuditHashInput {
        audit_log_id: &audit_log_id,
        event_type,
        entity_type,
        entity_id,
        actor_user_id,
        actor_type,
        created_at: &now,
        before_json,
        after_json,
        reason,
        previous_hash: &prev_hash,
    });

    sqlx::query(
        "INSERT INTO audit_logs
           (audit_log_id, event_type, entity_type, entity_id,
            actor_user_id, actor_type, device_id, origin_device_id, branch_id,
            before_json, after_json, reason,
            created_at, hash, previous_hash, override_used)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&audit_log_id)
    .bind(event_type)
    .bind(entity_type)
    .bind(entity_id)
    .bind(actor_user_id)
    .bind(actor_type)
    .bind(device_id)
    .bind(device_id) // origin_device_id = device_id
    .bind(branch_id)
    .bind(before_json)
    .bind(after_json)
    .bind(reason)
    .bind(&now)
    .bind(&hash)
    .bind(if prev_hash.is_empty() { None } else { Some(prev_hash.clone()) })
    .bind(override_used as i64)
    .execute(pool)
    .await?;

    Ok(())
}
```

- [ ] **Step 4: Add migration for before_json and reason columns**

Check if `audit_logs` already has `before_json` and `reason` columns. If not, create a new migration file. Find the highest-numbered migration in `src-tauri/src/db/` (look at `mod.rs` for the migration list) and add the next one:

```sql
-- migrations/XXXX_audit_before_json_reason.sql
ALTER TABLE audit_logs ADD COLUMN IF NOT EXISTS before_json TEXT;
ALTER TABLE audit_logs ADD COLUMN IF NOT EXISTS reason TEXT;
```

Register it in `src-tauri/src/db/mod.rs` in the migrations array.

- [ ] **Step 5: Update verify_chain to read and hash before_json + reason**

In the `AuditRow` struct inside `verify_chain`, add the new fields:
```rust
struct AuditRow {
    audit_log_id: String,
    event_type: String,
    entity_type: String,
    entity_id: String,
    actor_user_id: String,
    actor_type: String,   // NEW
    created_at: String,
    before_json: Option<String>, // NEW
    after_json: Option<String>,
    reason: Option<String>,      // NEW
    hash: String,
    previous_hash: Option<String>,
}
```

Update the SELECT query in `verify_chain`:
```rust
let raw = sqlx::query(
    "SELECT audit_log_id, event_type, entity_type, entity_id,
            COALESCE(actor_user_id, '') AS actor_user_id,
            COALESCE(actor_type, 'user') AS actor_type,
            created_at, before_json, after_json, reason, hash, previous_hash
     FROM audit_logs
     WHERE device_id = ?
     ORDER BY created_at ASC, audit_log_id ASC",
)
```

Update the row mapping and the `compute_audit_hash` call inside verify_chain to pass the new fields:
```rust
let expected = compute_audit_hash(&AuditHashInput {
    audit_log_id: &row.audit_log_id,
    event_type: &row.event_type,
    entity_type: &row.entity_type,
    entity_id: &row.entity_id,
    actor_user_id: &row.actor_user_id,
    actor_type: &row.actor_type,
    created_at: &row.created_at,
    before_json: row.before_json.as_deref(),
    after_json: row.after_json.as_deref(),
    reason: row.reason.as_deref(),
    previous_hash: stored_prev,
});
```

- [ ] **Step 6: Update ALL callers of insert_audit_entry throughout the codebase**

Search for all calls with:
```bash
grep -rn "insert_audit_entry\|insert_audit_entry_override" src-tauri/src/
```

For each call, add the new required parameters in this order:
- `actor_type`: use `"user"` for all existing human-triggered mutations (AI callers will use `"ai"`)
- `before_json`: use `None` for create events; for update events, serialize the pre-mutation row as JSON
- `after_json`: keep as-is (was previously the last parameter)
- `reason`: use `None` unless the mutation has a specific reason string

Example — existing call in `customer_commands.rs`:
```rust
// OLD:
let _ = audit_hash::insert_audit_entry(
    &state.db, "CUSTOMER_CREATED", "customer", &customer_id,
    &input.actor_user_id, &device_id, &branch_id, Some(&after),
).await;

// NEW:
let _ = audit_hash::insert_audit_entry(
    &state.db, "CUSTOMER_CREATED", "customer", &customer_id,
    &input.actor_user_id, "user", &device_id, &branch_id,
    None,          // before_json (create — no prior state)
    Some(&after),  // after_json
    None,          // reason
).await;
```

For update events that have a before state (e.g. product update), fetch the row before mutation and serialize:
```rust
// Fetch before state
let before_row = sqlx::query("SELECT * FROM products WHERE product_id = ?")
    .bind(&input.product_id)
    .fetch_optional(&state.db).await?;
let before_json = before_row.as_ref().map(|r| {
    serde_json::json!({
        "product_id": r.get::<String,_>("product_id"),
        "name": r.get::<String,_>("name"),
        // ... key fields
    }).to_string()
});
```

- [ ] **Step 7: Update test helpers in the #[cfg(test)] block**

Update the `inp` helper function in the tests to include the new fields:
```rust
fn inp<'a>(
    id: &'a str,
    et: &'a str,
    eid: &'a str,
    bj: Option<&'a str>,
    aj: Option<&'a str>,
    r: Option<&'a str>,
    ph: &'a str,
) -> AuditHashInput<'a> {
    AuditHashInput {
        audit_log_id: id,
        event_type: et,
        entity_type: "sale",
        entity_id: eid,
        actor_user_id: "U1",
        actor_type: "user",
        created_at: "t",
        before_json: bj,
        after_json: aj,
        reason: r,
        previous_hash: ph,
    }
}
```

Update the existing test calls to pass the new parameters.

- [ ] **Step 8: Verify**
```bash
cd zanpos/src-tauri && cargo check 2>&1 | tail -20
```
Expected: all audit_hash callers compile.

- [ ] **Step 9: Commit**
```bash
git add src-tauri/src/db/repositories/audit_hash.rs
git commit -m "fix: audit chain includes before_json, reason, actor_type in hash"
```

---

## Task 3 — Outbox: add transaction-aware enqueue variants

**Fixes:** C-3

**Files:** `src-tauri/src/sync/outbox.rs`

- [ ] **Step 1: Add enqueue_raw_in_tx**

After the closing brace of `enqueue_raw`, add:

```rust
/// Transaction-aware variant of enqueue_raw.
/// Call this when you need the sync_queue INSERT to be in the SAME
/// BEGIN/COMMIT as the parent mutation. The caller owns the transaction
/// and is responsible for commit/rollback.
#[allow(clippy::too_many_arguments)]
pub async fn enqueue_raw_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    device_id: &str,
    branch_id: &str,
    entity_type: &str,
    entity_id: &str,
    operation: &str,
    payload: Value,
    idem_suffix: &str,
) -> AppResult<i64> {
    let sync_event_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let seq: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(local_sequence), 0) + 1 FROM sync_queue WHERE device_id = ?",
    )
    .bind(device_id)
    .fetch_one(&mut **tx)
    .await?;

    let payload_str = payload.to_string();
    let payload_hash = sha256_hex(&payload_str);
    let idempotency_key = format!("{}-{}-{}", entity_type, entity_id, idem_suffix);

    sqlx::query(
        "INSERT OR IGNORE INTO sync_queue
         (sync_event_id, device_id, branch_id, entity_type, entity_id, operation,
          payload_json, payload_hash, idempotency_key, local_sequence, created_at, status)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,'pending')",
    )
    .bind(&sync_event_id)
    .bind(device_id)
    .bind(branch_id)
    .bind(entity_type)
    .bind(entity_id)
    .bind(operation)
    .bind(&payload_str)
    .bind(&payload_hash)
    .bind(&idempotency_key)
    .bind(seq)
    .bind(&now)
    .execute(&mut **tx)
    .await?;

    Ok(seq)
}
```

- [ ] **Step 2: Add _in_tx wrapper for every high-level enqueue helper used in mutation paths**

For each existing `enqueue_X(pool: &SqlitePool, ...)` helper in outbox.rs, add a matching `enqueue_X_in_tx(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, ...)` that calls `enqueue_raw_in_tx`. The payload construction is identical — just the low-level call and executor differ.

Add these wrappers for at minimum:
- `enqueue_sale_in_tx`
- `enqueue_sale_item_in_tx`
- `enqueue_payment_in_tx`
- `enqueue_product_in_tx`
- `enqueue_category_in_tx`
- `enqueue_customer_in_tx`
- `enqueue_stock_movement_in_tx`
- `enqueue_stock_level_in_tx`
- `enqueue_delivery_order_in_tx`
- `enqueue_shift_in_tx`
- `enqueue_refund_in_tx`
- `enqueue_refund_item_in_tx`
- `enqueue_audit_log_in_tx`

Pattern for each (example — enqueue_customer_in_tx):
```rust
#[allow(clippy::too_many_arguments)]
pub async fn enqueue_customer_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    device_id: &str,
    branch_id: &str,
    customer_id: &str,
    name: &str,
    phone: Option<&str>,
    email: Option<&str>,
    loyalty_points: i64,
    notes: Option<&str>,
    updated_at: &str,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "customer_id": customer_id,
        "branch_id": branch_id,
        "name": name,
        "phone": phone,
        "email": email,
        "loyalty_points": loyalty_points,
        "notes": notes,
        "updated_at": updated_at,
    });
    enqueue_raw_in_tx(tx, device_id, branch_id, "customer", customer_id, "upsert", payload, updated_at).await?;
    Ok(())
}
```

Repeat for each entity type, mirroring the payload JSON from the existing pool-based version.

- [ ] **Step 3: Update pos_finalize_sale to use enqueue_in_tx**

In `src-tauri/src/db/repositories/sale_repo.rs`, find where `pos_finalize_sale` calls `outbox::enqueue_sale`, `outbox::enqueue_sale_item`, `outbox::enqueue_payment` — these are called after `tx.commit()`. Move them BEFORE `tx.commit()` and change them to use the `_in_tx` variants, passing `&mut tx`:

```rust
// BEFORE tx.commit(): enqueue all sale entities
outbox::enqueue_sale_in_tx(&mut tx, ...).await?;
for item in &sale_items {
    outbox::enqueue_sale_item_in_tx(&mut tx, ...).await?;
}
for payment in &payments {
    outbox::enqueue_payment_in_tx(&mut tx, ...).await?;
}

tx.commit().await?;
// Remove the post-commit enqueue calls that were here
```

- [ ] **Step 4: Update admin_create_product / admin_update_product**

In `admin_commands.rs`, wrap each admin mutation that currently has separate enqueue calls:

```rust
// admin_create_product — wrap in transaction
let mut tx = state.db.begin().await?;
// ... INSERT INTO products ...  (bind to &mut *tx)
outbox::enqueue_product_in_tx(&mut tx, ...).await?;
tx.commit().await?;
```

Apply same pattern to `admin_update_product`, `admin_save_category`, `admin_save_tax_rule`.

- [ ] **Step 5: Verify**
```bash
cd zanpos/src-tauri && cargo check 2>&1 | tail -20
```
Expected: no errors.

- [ ] **Step 6: Commit**
```bash
git add src-tauri/src/sync/outbox.rs src-tauri/src/db/repositories/sale_repo.rs src-tauri/src/commands/admin_commands.rs
git commit -m "fix: enqueue sync_queue in same transaction as parent mutation"
```

---

## Task 4 — Inventory commands: transactions + decimal arithmetic + types + audit

**Fixes:** C-2, H-9, H-10, H-29, M-6, M-7, M-8

**Files:** `src-tauri/src/commands/inventory_commands.rs`

> NOTE: `rust_decimal` is used for string-based decimal arithmetic. Check `Cargo.toml` — if not present, add `rust_decimal = "1"` to `[dependencies]`.

- [ ] **Step 1: Add rust_decimal import and helper**

At the top of `inventory_commands.rs`, add:
```rust
use rust_decimal::Decimal;
use std::str::FromStr;
```

Add a helper at the top of the file:
```rust
/// Parse a decimal string for stock quantity. Returns an error AppResult if invalid.
fn parse_qty(s: &str) -> AppResult<Decimal> {
    Decimal::from_str(s.trim())
        .map_err(|_| AppError::Validation(format!("Invalid quantity: {}", s)))
}
```

- [ ] **Step 2: Rewrite inventory_receive_stock with transaction + Decimal**

Replace the entire `inventory_receive_stock` function body:

```rust
#[tauri::command]
pub async fn inventory_receive_stock(
    input: ReceiveStockInput,
    state: State<'_, AppState>,
) -> Result<StockLevel, AppError> {
    rbac::manager_or_owner(&state.db, &input.received_by_user_id).await?;
    let qty = parse_qty(&input.quantity)?;
    if qty <= Decimal::ZERO {
        return Err(AppError::Validation("Quantity must be positive".into()));
    }

    let branch_id = active_branch_id(&state).await?;
    let device_id = active_device_id(&state).await?;
    let now = chrono::Utc::now().to_rfc3339();

    let mut tx = state.db.begin().await?;

    // Read current stock (locked in this transaction)
    let old_qty_str: Option<String> = sqlx::query_scalar(
        "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?",
    )
    .bind(&input.product_id)
    .bind(&branch_id)
    .fetch_optional(&mut *tx)
    .await?;

    let old_qty = old_qty_str
        .as_deref()
        .map(|s| Decimal::from_str(s).unwrap_or(Decimal::ZERO))
        .unwrap_or(Decimal::ZERO);
    let new_qty = old_qty + qty;
    let new_qty_str = new_qty.normalize().to_string();

    let stock_level_id = format!("SL-{}-{}", input.product_id, branch_id);

    // Upsert stock_levels using pure Rust arithmetic (no CAST AS REAL)
    sqlx::query(
        "INSERT INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand, updated_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(product_id, branch_id) DO UPDATE SET
           quantity_on_hand = excluded.quantity_on_hand,
           updated_at = excluded.updated_at",
    )
    .bind(&stock_level_id)
    .bind(&input.product_id)
    .bind(&branch_id)
    .bind(&new_qty_str)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    let movement_id = Ulid::new().to_string();
    sqlx::query(
        "INSERT INTO stock_movements
           (movement_id, product_id, branch_id, device_id, origin_device_id, movement_type,
            quantity_delta, quantity_after, reference_type, notes, created_by_user_id,
            created_at, sync_status)
         VALUES (?, ?, ?, ?, ?, 'receive', ?, ?, 'receive', ?, ?, ?, 'pending')",
    )
    .bind(&movement_id)
    .bind(&input.product_id)
    .bind(&branch_id)
    .bind(&device_id)
    .bind(&device_id) // origin_device_id
    .bind(&input.quantity) // quantity_delta (positive)
    .bind(&new_qty_str)
    .bind(&input.notes)
    .bind(&input.received_by_user_id)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    // Enqueue inside transaction
    outbox::enqueue_stock_movement_in_tx(
        &mut tx, &device_id, &branch_id, &movement_id, &input.product_id,
        "receive", &input.quantity, &new_qty_str, "receive", "",
        input.notes.as_deref(), Some(&input.received_by_user_id), &now,
    ).await?;
    outbox::enqueue_stock_level_in_tx(
        &mut tx, &device_id, &branch_id, &input.product_id, &new_qty_str, &now, &now,
    ).await?;

    tx.commit().await?;

    // Audit log (outside tx is acceptable for audit — it is append-only)
    let after = serde_json::json!({
        "product_id": input.product_id,
        "quantity_delta": input.quantity,
        "quantity_after": new_qty_str,
    }).to_string();
    let _ = audit_hash::insert_audit_entry(
        &state.db, "STOCK_RECEIVED", "stock_level", &input.product_id,
        &input.received_by_user_id, "user", &device_id, &branch_id,
        None, Some(&after), input.notes.as_deref(),
    ).await;

    let levels = stock_repo::get_all_levels(&state.db, &branch_id).await?;
    levels
        .into_iter()
        .find(|l| l.product_id == input.product_id)
        .ok_or_else(|| AppError::NotFound(format!("Product {} not found", input.product_id)))
}
```

- [ ] **Step 3: Rewrite inventory_adjust_stock with transaction + Decimal**

Replace `inventory_adjust_stock` function body:

```rust
#[tauri::command]
pub async fn inventory_adjust_stock(
    input: AdjustStockInput,
    state: State<'_, AppState>,
) -> Result<StockLevel, AppError> {
    rbac::manager_or_owner(&state.db, &input.adjusted_by_user_id).await?;
    let new_qty = parse_qty(&input.new_quantity)?;
    if new_qty < Decimal::ZERO {
        return Err(AppError::Validation("Quantity cannot be negative".into()));
    }

    let branch_id = active_branch_id(&state).await?;
    let device_id = active_device_id(&state).await?;
    let now = chrono::Utc::now().to_rfc3339();

    let mut tx = state.db.begin().await?;

    let old_qty_str: Option<String> = sqlx::query_scalar(
        "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?",
    )
    .bind(&input.product_id)
    .bind(&branch_id)
    .fetch_optional(&mut *tx)
    .await?;

    let old_qty = old_qty_str
        .as_deref()
        .map(|s| Decimal::from_str(s).unwrap_or(Decimal::ZERO))
        .unwrap_or(Decimal::ZERO);
    let delta = new_qty - old_qty;
    let delta_str = delta.normalize().to_string();
    let new_qty_str = new_qty.normalize().to_string();
    let stock_level_id = format!("SL-{}-{}", input.product_id, branch_id);

    sqlx::query(
        "INSERT INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand, updated_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(product_id, branch_id) DO UPDATE SET
           quantity_on_hand = excluded.quantity_on_hand,
           updated_at = excluded.updated_at",
    )
    .bind(&stock_level_id)
    .bind(&input.product_id)
    .bind(&branch_id)
    .bind(&new_qty_str)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    let movement_id = Ulid::new().to_string();
    sqlx::query(
        "INSERT INTO stock_movements
           (movement_id, product_id, branch_id, device_id, origin_device_id, movement_type,
            quantity_delta, quantity_after, reference_type, notes, created_by_user_id,
            created_at, sync_status)
         VALUES (?, ?, ?, ?, ?, 'manual_adjust', ?, ?, 'manual_adjust', ?, ?, ?, 'pending')",
    )
    .bind(&movement_id)
    .bind(&input.product_id)
    .bind(&branch_id)
    .bind(&device_id)
    .bind(&device_id)
    .bind(&delta_str)
    .bind(&new_qty_str)
    .bind(&input.notes)
    .bind(&input.adjusted_by_user_id)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    outbox::enqueue_stock_movement_in_tx(
        &mut tx, &device_id, &branch_id, &movement_id, &input.product_id,
        "manual_adjust", &delta_str, &new_qty_str, "manual_adjust", "",
        input.notes.as_deref(), Some(&input.adjusted_by_user_id), &now,
    ).await?;
    outbox::enqueue_stock_level_in_tx(
        &mut tx, &device_id, &branch_id, &input.product_id, &new_qty_str, &now, &now,
    ).await?;

    tx.commit().await?;

    let after = serde_json::json!({
        "product_id": input.product_id,
        "old_quantity": old_qty_str.unwrap_or_else(|| "0".to_string()),
        "new_quantity": new_qty_str,
        "delta": delta_str,
    }).to_string();
    let _ = audit_hash::insert_audit_entry(
        &state.db, "STOCK_ADJUSTED", "stock_level", &input.product_id,
        &input.adjusted_by_user_id, "user", &device_id, &branch_id,
        None, Some(&after), input.notes.as_deref(),
    ).await;

    let levels = stock_repo::get_all_levels(&state.db, &branch_id).await?;
    levels
        .into_iter()
        .find(|l| l.product_id == input.product_id)
        .ok_or_else(|| AppError::NotFound(format!("Product {} not found", input.product_id)))
}
```

- [ ] **Step 4: Update BulkStockTakeEntry to use String for new_quantity**

Change:
```rust
pub struct BulkStockTakeEntry {
    pub product_id: String,
    pub new_quantity: f64,  // OLD
    pub notes: Option<String>,
}
```
to:
```rust
pub struct BulkStockTakeEntry {
    pub product_id: String,
    pub new_quantity: String, // decimal string — avoids float representation errors
    pub notes: Option<String>,
}
```

- [ ] **Step 5: Rewrite inventory_bulk_stock_take loop with Decimal + per-entry transaction**

Replace the `for entry in &entries` loop:

```rust
for entry in &entries {
    let new_qty = match parse_qty(&entry.new_quantity) {
        Ok(q) if q >= Decimal::ZERO => q,
        Ok(_) => {
            errors.push(format!("Product {}: quantity cannot be negative", entry.product_id));
            continue;
        }
        Err(_) => {
            errors.push(format!("Product {}: invalid quantity '{}'", entry.product_id, entry.new_quantity));
            continue;
        }
    };

    let new_qty_str = new_qty.normalize().to_string();

    let result: AppResult<()> = async {
        let mut tx = state.db.begin().await?;

        let old_qty_str: Option<String> = sqlx::query_scalar(
            "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?",
        )
        .bind(&entry.product_id)
        .bind(&branch_id)
        .fetch_optional(&mut *tx)
        .await?;

        let old_qty = old_qty_str
            .as_deref()
            .map(|s| Decimal::from_str(s).unwrap_or(Decimal::ZERO))
            .unwrap_or(Decimal::ZERO);
        let delta = new_qty - old_qty;
        let delta_str = delta.normalize().to_string();
        let stock_level_id = format!("SL-{}-{}", entry.product_id, branch_id);

        sqlx::query(
            "INSERT INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand, updated_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(product_id, branch_id) DO UPDATE SET
               quantity_on_hand = excluded.quantity_on_hand,
               updated_at = excluded.updated_at",
        )
        .bind(&stock_level_id)
        .bind(&entry.product_id)
        .bind(&branch_id)
        .bind(&new_qty_str)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        let movement_id = Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO stock_movements
               (movement_id, product_id, branch_id, device_id, origin_device_id, movement_type,
                quantity_delta, quantity_after, reference_type, notes, created_by_user_id,
                created_at, sync_status)
             VALUES (?, ?, ?, ?, ?, 'stock_take', ?, ?, 'stock_take', ?, ?, ?, 'pending')",
        )
        .bind(&movement_id)
        .bind(&entry.product_id)
        .bind(&branch_id)
        .bind(&device_id)
        .bind(&device_id)
        .bind(&delta_str)
        .bind(&new_qty_str)
        .bind(&entry.notes)
        .bind(&actor_user_id)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        outbox::enqueue_stock_movement_in_tx(
            &mut tx, &device_id, &branch_id, &movement_id, &entry.product_id,
            "stock_take", &delta_str, &new_qty_str, "stock_take", "",
            entry.notes.as_deref(), Some(&actor_user_id), &now,
        ).await?;
        outbox::enqueue_stock_level_in_tx(
            &mut tx, &device_id, &branch_id, &entry.product_id, &new_qty_str, &now, &now,
        ).await?;

        tx.commit().await?;
        Ok(())
    }.await;

    match result {
        Ok(()) => {
            updated += 1;
            let after = serde_json::json!({
                "product_id": entry.product_id,
                "new_quantity": new_qty_str,
            }).to_string();
            let _ = audit_hash::insert_audit_entry(
                &state.db, "STOCK_TAKE", "stock_level", &entry.product_id,
                &actor_user_id, "user", &device_id, &branch_id,
                None, Some(&after), entry.notes.as_deref(),
            ).await;
        }
        Err(e) => errors.push(format!("Product {}: {}", entry.product_id, e)),
    }
}
```

- [ ] **Step 6: Add required import for audit_hash**

At the top of `inventory_commands.rs`:
```rust
use crate::db::repositories::audit_hash;
```

- [ ] **Step 7: Verify**
```bash
cd zanpos/src-tauri && cargo check 2>&1 | tail -20
```

- [ ] **Step 8: Commit**
```bash
git add src-tauri/src/commands/inventory_commands.rs src-tauri/Cargo.toml
git commit -m "fix: inventory mutations use transactions, Decimal arithmetic, correct movement types, audit logs"
```

---

## Task 5 — POS commands: RBAC + ghost barcode + decimal guard + auto-print

**Fixes:** C-1, H-1, H-2, H-3, M-2

**Files:** `src-tauri/src/commands/pos_commands.rs`

- [ ] **Step 1: Add RBAC gate to pos_set_line_price**

Find the `SetLinePriceInput` struct. Add `authorized_by_user_id`:
```rust
#[derive(serde::Deserialize)]
pub struct SetLinePriceInput {
    pub cart_id: String,
    pub line_id: String,
    pub new_price_minor: i64,
    pub authorized_by_user_id: String, // NEW — manager/owner required
}
```

At the top of `pos_set_line_price`, before any other logic:
```rust
rbac::manager_or_owner(&state.db, &input.authorized_by_user_id).await?;
```

- [ ] **Step 2: Create ghost record on unknown barcode scan**

In `pos_add_item_by_barcode`, find the `None` branch where `get_product_by_barcode` returns nothing. Replace the simple `AppError::NotFound` return with:

```rust
None => {
    // Record unknown barcode for ghost resolution workflow
    let _ = crate::commands::ghost_barcode_commands::ghost_record(
        barcode.clone(),
        state.clone(),
    ).await;
    return Err(AppError::GhostBarcode(barcode));
}
```

If `AppError::GhostBarcode` doesn't exist, add it to `errors.rs`:
```rust
GhostBarcode(String),
```
And its Display/serialisation (follow the pattern of other variants in errors.rs).

- [ ] **Step 3: Gate decimal quantity in pos_add_item**

After fetching the product row inside `pos_add_item`, add:
```rust
let qty_str = input.quantity.as_deref().unwrap_or("1");
let qty_f: f64 = qty_str.parse().unwrap_or(1.0);
if qty_f.fract() != 0.0 && !product.allow_decimal_quantity {
    return Err(AppError::Validation(
        "This product does not allow decimal quantities".into()
    ));
}
```

- [ ] **Step 4: Gate decimal quantity in pos_update_quantity**

After parsing `qty` from the input string inside `pos_update_quantity`, add:
```rust
// Fetch allow_decimal_quantity for this cart line's product
let allow_decimal: bool = sqlx::query_scalar(
    "SELECT p.allow_decimal_quantity
     FROM cart_lines cl
     JOIN products p ON p.product_id = cl.product_id
     WHERE cl.line_id = ?",
)
.bind(&input.line_id)
.fetch_optional(&state.db)
.await?
.flatten()
.unwrap_or(false);

if qty.fract() != 0.0 && !allow_decimal {
    return Err(AppError::Validation(
        "This product does not allow decimal quantities".into()
    ));
}
```

- [ ] **Step 5: Trigger auto-print after finalize_sale**

After `sale_repo::finalize_sale` returns `Ok(result)` inside `pos_finalize_sale`, add:

```rust
// Auto-print if flag is set
let auto_print: Option<String> = sqlx::query_scalar(
    "SELECT value FROM app_config WHERE key = 'flag_auto_print_receipt'",
)
.fetch_optional(&state.db)
.await
.ok()
.flatten()
.flatten();

if auto_print.as_deref() == Some("1") {
    let _ = crate::commands::thermal_commands::print_receipt_raw_for_sale(
        &state,
        &result.sale_id,
    ).await;
}
```

If `print_receipt_raw_for_sale` doesn't exist as a callable sub-function, check `thermal_commands.rs` for the print function and call it appropriately, or emit a Tauri event to the frontend to trigger printing.

- [ ] **Step 6: Verify**
```bash
cd zanpos/src-tauri && cargo check 2>&1 | tail -20
```

- [ ] **Step 7: Commit**
```bash
git add src-tauri/src/commands/pos_commands.rs src-tauri/src/errors.rs
git commit -m "fix: RBAC on line price, ghost barcode on unknown scan, decimal quantity guard, auto-print"
```

---

## Task 6 — Admin commands: version increment + missing fields + tax append-only

**Fixes:** H-4, H-5, H-6, H-8, M-3, M-4, M-5, L-2, L-4, PR-02 (effective_to re-enqueue)

**Files:** `src-tauri/src/commands/admin_commands.rs`

- [ ] **Step 1: Add missing fields to CreateProductInput and UpdateProductInput**

Find `CreateProductInput` and `UpdateProductInput` structs. Add:
```rust
pub cost_minor: Option<i64>,
pub description: Option<String>,
pub default_supplier_id: Option<String>,
```

- [ ] **Step 2: Add fields to AdminProduct response struct**

Find `AdminProduct` struct. Add:
```rust
pub cost_minor: Option<i64>,
pub description: Option<String>,
pub default_supplier_id: Option<String>,
```

- [ ] **Step 3: Add fields to ADMIN_PRODUCT_QUERY**

Find the `ADMIN_PRODUCT_QUERY` constant. Add `p.cost_minor, p.description, p.default_supplier_id` to the SELECT list.

- [ ] **Step 4: Add fields to admin_create_product INSERT**

Find the INSERT in `admin_create_product`. Add the three columns and `.bind(input.cost_minor).bind(input.description.as_deref()).bind(input.default_supplier_id.as_deref())`.

- [ ] **Step 5: Increment version on product UPDATE**

Find the UPDATE statement in `admin_update_product`. Add `version = version + 1,` to the SET clause. After the UPDATE, fetch the new version:
```rust
let new_version: i64 = sqlx::query_scalar(
    "SELECT version FROM products WHERE product_id = ?",
)
.bind(&input.product_id)
.fetch_one(&state.db)
.await?;
```
Pass `new_version` to `enqueue_product(...)` instead of the hardcoded `1`.

Also add `description` and `default_supplier_id` to the UPDATE SET clause, and `cost_minor`.

- [ ] **Step 6: Increment version on category UPDATE**

Same pattern in `admin_save_category`:
```rust
// In UPDATE branch:
version = version + 1,
```
Fetch new version and pass to `enqueue_category(...)` instead of hardcoded `1`.

- [ ] **Step 7: Tax rule append-only — INSERT new row for rate changes**

In `admin_save_tax_rule`, when an existing `tax_rule_id` is provided and `rate_basis_points` or `inclusive` differs from the stored value:
```rust
// Check if rate is changing
let existing: Option<(i64, bool)> = sqlx::query_as(
    "SELECT rate_basis_points, inclusive FROM tax_rules WHERE tax_rule_id = ?",
)
.bind(&input.tax_rule_id)
.fetch_optional(&state.db)
.await?;

if let Some((old_rate, old_inclusive)) = existing {
    let rate_changed = old_rate != input.rate_basis_points
        || old_inclusive != input.inclusive;
    if rate_changed {
        let now = chrono::Utc::now().to_rfc3339();
        // Close the old rule
        sqlx::query(
            "UPDATE tax_rules SET effective_to = ?, updated_at = ? WHERE tax_rule_id = ?",
        )
        .bind(&now)
        .bind(&now)
        .bind(&input.tax_rule_id)
        .execute(&state.db)
        .await?;
        // Insert new rule
        let new_id = Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO tax_rules (tax_rule_id, name, rate_basis_points, inclusive,
             is_active, effective_from, effective_to, updated_at, version)
             VALUES (?, ?, ?, ?, ?, ?, NULL, ?, 1)",
        )
        .bind(&new_id)
        .bind(&input.name)
        .bind(input.rate_basis_points)
        .bind(input.inclusive)
        .bind(input.is_active.unwrap_or(true))
        .bind(&now)
        .bind(&now)
        .execute(&state.db)
        .await?;
        // Enqueue the new rule
        outbox::enqueue_tax_rule(&state.db, &device_id, &branch_id, &new_id, ...).await?;
        return Ok(new_id);
    }
}
// If only name/is_active changed, do a normal UPDATE (excluding rate/inclusive/effective_from)
sqlx::query(
    "UPDATE tax_rules SET name = ?, is_active = ?, updated_at = ? WHERE tax_rule_id = ?",
)
.bind(&input.name)
.bind(input.is_active.unwrap_or(true))
.bind(&now)
.bind(&input.tax_rule_id)
.execute(&state.db)
.await?;
```

- [ ] **Step 8: Re-enqueue closed price row with effective_to**

In `admin_update_product`, after setting `effective_to` on the old price row:
```rust
// Re-enqueue the closed price row so remote terminals update it
let old_price_id: Option<String> = // fetch the price_id that was just closed
outbox::enqueue_product_price(
    &state.db, &device_id, &branch_id,
    &old_price_id.unwrap_or_default(),
    &input.product_id, old_price_minor, &now, // effective_to = now
).await?;
```

- [ ] **Step 9: Fix BulkProductRow to include reorder_point**

Find `BulkProductRow` struct in `admin_commands.rs`. Add `pub reorder_point: Option<i64>`. Parse it from CSV. Use it in both the INSERT and the `enqueue_product` call (instead of hardcoded `0`).

- [ ] **Step 10: Fix BulkCategoryRow to support parent_category_id (L-4)**

Find `BulkCategoryRow` struct in `admin_commands.rs`. Add `pub parent_category_id: Option<String>`. Parse it from the CSV column (e.g. `"parent_category_id"` or `"parent_id"` — check headers). Add the column to the INSERT statement. If the CSV uses category name as the parent reference, resolve the name to an ID before inserting (the same auto-create logic used for product categories already exists — apply it here).

- [ ] **Step 10: Verify**
```bash
cd zanpos/src-tauri && cargo check 2>&1 | tail -20
```

- [ ] **Step 11: Commit**
```bash
git add src-tauri/src/commands/admin_commands.rs
git commit -m "fix: version increment, cost/description/supplier fields, tax append-only, price re-enqueue"
```

---

## Task 7 — Outbox: enqueue_tax_rule effective_to payload

**Fixes:** H-7

**Files:** `src-tauri/src/sync/outbox.rs`

- [ ] **Step 1: Add effective_to to enqueue_tax_rule**

Find `enqueue_tax_rule` in outbox.rs. Add `effective_to: Option<&str>` parameter. Add `"effective_to": effective_to` to the `serde_json::json!({...})` payload. Update all call sites.

- [ ] **Step 2: Verify and commit**
```bash
cd zanpos/src-tauri && cargo check 2>&1 | tail -5
git add src-tauri/src/sync/outbox.rs
git commit -m "fix: include effective_to in tax_rule sync payload"
```

---

## Task 8 — Customer commands: updated_at + loyalty enqueue

**Fixes:** H-11, H-12, H-13, M-9

**Files:** `src-tauri/src/commands/customer_commands.rs`, `src-tauri/src/sync/outbox.rs`

- [ ] **Step 1: Add updated_at to customer_create INSERT**

In `customer_create`, the INSERT currently is:
```rust
"INSERT INTO customers
   (customer_id, branch_id, name, phone, email, loyalty_points, created_at, notes)
 VALUES (?,?,?,?,?,0,?,?)"
```
Change to:
```rust
"INSERT INTO customers
   (customer_id, branch_id, name, phone, email, loyalty_points, created_at, updated_at, notes)
 VALUES (?,?,?,?,?,0,?,?,?)"
```
Add `.bind(&now)` after the `created_at` bind.

Also update the `enqueue_customer` call to pass `&now` as `updated_at`:
```rust
let _ = outbox::enqueue_customer(
    &state.db, &device_id, &branch_id,
    &customer_id, input.name.trim(),
    input.phone.as_deref().filter(|s| !s.is_empty()),
    input.email.as_deref().filter(|s| !s.is_empty()),
    0,
    input.notes.as_deref().filter(|s| !s.is_empty()),
    &now, // updated_at — same timestamp written to DB
).await;
```

- [ ] **Step 2: Add updated_at to customer_update**

In `customer_update`, change the UPDATE to:
```rust
"UPDATE customers
 SET name=?, phone=?, email=?, notes=?, updated_at=?
 WHERE customer_id=?"
```
Add `.bind(&now)` before `.bind(&input.customer_id)`.

Also enqueue after the update — fetch the updated row and call `enqueue_customer`:
```rust
// Fetch device for enqueue
let device_id: String = sqlx::query_scalar(
    "SELECT device_id FROM devices WHERE is_active=1 ORDER BY device_code LIMIT 1",
).fetch_optional(&state.db).await?.flatten().unwrap_or_default();
let branch_id = active_branch_id(&state).await?;
let row = sqlx::query(
    "SELECT loyalty_points FROM customers WHERE customer_id = ?",
).bind(&input.customer_id).fetch_one(&state.db).await?;
let _ = outbox::enqueue_customer(
    &state.db, &device_id, &branch_id,
    &input.customer_id, input.name.trim(),
    input.phone.as_deref().filter(|s| !s.is_empty()),
    input.email.as_deref().filter(|s| !s.is_empty()),
    row.get::<i64, _>("loyalty_points"),
    input.notes.as_deref().filter(|s| !s.is_empty()),
    &now,
).await;
```

- [ ] **Step 3: Enqueue in customer_add_loyalty**

Find `customer_add_loyalty`. After the loyalty UPDATE completes, add:
```rust
// Enqueue sync event for the loyalty change
let device_id: String = sqlx::query_scalar(
    "SELECT device_id FROM devices WHERE is_active=1 ORDER BY device_code LIMIT 1",
).fetch_optional(&state.db).await?.flatten().unwrap_or_default();
let branch_id = active_branch_id(&state).await?;
let now = chrono::Utc::now().to_rfc3339();

let updated = sqlx::query(
    "SELECT name, phone, email, loyalty_points, notes FROM customers WHERE customer_id = ?",
).bind(&customer_id).fetch_one(&state.db).await?;

let _ = outbox::enqueue_customer(
    &state.db, &device_id, &branch_id,
    &customer_id,
    updated.get::<String,_>("name").as_str(),
    updated.get::<Option<String>,_>("phone").as_deref(),
    updated.get::<Option<String>,_>("email").as_deref(),
    updated.get::<i64,_>("loyalty_points"),
    updated.get::<Option<String>,_>("notes").as_deref(),
    &now,
).await;

// Also update updated_at
let _ = sqlx::query(
    "UPDATE customers SET updated_at = ? WHERE customer_id = ?",
).bind(&now).bind(&customer_id).execute(&state.db).await;
```

- [ ] **Step 4: Fix enqueue_customer timestamp**

In `outbox.rs`, find `enqueue_customer`. Add `updated_at: &str` parameter:
```rust
pub async fn enqueue_customer(
    pool: &SqlitePool,
    device_id: &str,
    branch_id: &str,
    customer_id: &str,
    name: &str,
    phone: Option<&str>,
    email: Option<&str>,
    loyalty_points: i64,
    notes: Option<&str>,
    updated_at: &str,  // NEW — use the same timestamp written to the DB row
) -> AppResult<()> {
    let payload = serde_json::json!({
        "customer_id": customer_id,
        "branch_id": branch_id,
        "name": name,
        "phone": phone,
        "email": email,
        "loyalty_points": loyalty_points,
        "notes": notes,
        "updated_at": updated_at,  // use caller-provided timestamp
    });
    enqueue_raw(pool, device_id, branch_id, "customer", customer_id, "upsert", payload, updated_at).await?;
    Ok(())
}
```

Update all call sites to pass `updated_at`.

- [ ] **Step 5: Verify and commit**
```bash
cd zanpos/src-tauri && cargo check 2>&1 | tail -20
git add src-tauri/src/commands/customer_commands.rs src-tauri/src/sync/outbox.rs
git commit -m "fix: customers get updated_at, loyalty changes enqueued for sync"
```

---

## Task 9 — Delivery fixes

**Fixes:** H-14, H-15, H-16, M-10, D-05

**Files:** `src-tauri/src/db/repositories/delivery_repo.rs`, `src-tauri/src/commands/delivery_commands.rs`

- [ ] **Step 1: Enforce customer_id in create_delivery_in_tx**

In `delivery_repo.rs`, inside `create_delivery_in_tx`, before the INSERT:
```rust
if input.customer_id.as_deref().map(str::trim).unwrap_or("").is_empty() {
    return Err(AppError::Validation("customer_id is required for delivery orders".into()));
}
```

- [ ] **Step 2: Rename status 'out_for_delivery' → 'dispatched'**

In `delivery_repo.rs`, find the allowed status list in `update_delivery_status`:
```rust
let allowed = ["pending", "dispatched", "delivered", "cancelled"];
```
(Change `"out_for_delivery"` to `"dispatched"`.)

Also update any existing INSERT or default status value references.

- [ ] **Step 3: Remove 'wallet' from expected_payment_method**

Find the validation in `create_delivery_in_tx`:
```rust
let allowed_pm = ["cash", "card"];  // remove "wallet"
```

- [ ] **Step 4: Add branch_id scoping to list_deliveries**

In `list_deliveries`, add a `branch_id: &str` parameter. Make the base query start with:
```rust
let mut q = "SELECT ... FROM delivery_orders d WHERE d.branch_id = ? ".to_string();
```
Pass `branch_id` as the first bind param. Update `delivery_list` command to pass `active_branch_id`.

- [ ] **Step 5: Trigger WhatsApp notifications on delivery transitions**

> **Dependency note:** This step calls `whatsapp_commands::whatsapp_send_delivery_impl` and similar `_impl` helpers. Complete **Task 11 Step 5** before implementing this step — those helpers must exist for this file to compile.

In `delivery_commands.rs`, inside `delivery_update_status`, after a successful status update, add:

```rust
let new_status = &input.new_status;
if new_status == "dispatched" || new_status == "out_for_delivery" {
    let contact: Option<String> = sqlx::query_scalar(
        "SELECT contact_number FROM delivery_orders WHERE delivery_order_id = ?",
    )
    .bind(&input.delivery_order_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
    .flatten();

    if let Some(phone) = contact {
        let _ = crate::commands::whatsapp_commands::whatsapp_send_delivery_impl(
            &state, &phone, &input.delivery_order_id,
        ).await;
    }
}
if new_status == "delivered" {
    let contact: Option<String> = sqlx::query_scalar(
        "SELECT contact_number FROM delivery_orders WHERE delivery_order_id = ?",
    )
    .bind(&input.delivery_order_id)
    .fetch_optional(&state.db)
    .await.ok().flatten().flatten();
    if let Some(phone) = contact {
        let _ = crate::commands::whatsapp_commands::whatsapp_notify_arrival_impl(
            &state, &phone, &input.delivery_order_id,
        ).await;
    }
}
```

In `delivery_confirm_payment`, after success:
```rust
let contact: Option<String> = sqlx::query_scalar(
    "SELECT contact_number FROM delivery_orders WHERE delivery_order_id = ?",
)
.bind(&input.delivery_order_id)
.fetch_optional(&state.db).await.ok().flatten().flatten();
if let Some(phone) = contact {
    let _ = crate::commands::whatsapp_commands::whatsapp_payment_reminder_impl(
        &state, &phone, &input.delivery_order_id,
    ).await;
}
```

This requires extracting the WA send logic into `_impl` helper functions in `whatsapp_commands.rs`. See Task 11.

- [ ] **Step 6: Verify and commit**
```bash
cd zanpos/src-tauri && cargo check 2>&1 | tail -20
git add src-tauri/src/db/repositories/delivery_repo.rs src-tauri/src/commands/delivery_commands.rs
git commit -m "fix: delivery customer_id guard, branch_id scope, dispatched status, WA triggers"
```

---

## Task 10 — Report device scoping + Z-report

**Fixes:** H-17, M-13, M-14

**Files:** `src-tauri/src/commands/phase10a_commands.rs`, `src-tauri/src/commands/report_commands.rs`, `src-tauri/src/lib.rs`

- [ ] **Step 1: Add origin_device_id filter to report_tax_by_day**

In `phase10a_commands.rs`, find `report_tax_by_day`. At the top, call the existing `report_scope` helper:
```rust
let (scope, origin_device_id) = crate::commands::report_commands::report_scope_values(&state.db).await;
```
(Check how `report_scope` works in `report_commands.rs` — it returns a `(String, String)` of `(scope_mode, device_id)`. If it's a private helper, make it `pub`.)

Add to the SQL WHERE clause:
```sql
AND (? = 'all' OR origin_device_id = ?)
```
And add two binds: `.bind(&scope).bind(&origin_device_id)`.

- [ ] **Step 2: Add origin_device_id filter to report_eod_cashup**

In `report_commands.rs`, find the shifts query inside `report_eod_cashup_inner`. Apply the same `report_scope` pattern — add `AND (? = 'all' OR s.origin_device_id = ?)` to the shifts SELECT WHERE clause.

- [ ] **Step 3: Add Z-report command alias**

In `report_commands.rs`, add:
```rust
/// Z-report: end-of-day cash-up summary with an audit log entry marking it as issued.
/// Delegates to report_eod_cashup_inner and records a Z_REPORT audit event.
#[tauri::command]
pub async fn report_z_report(
    date: String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<EodCashupReport, AppError> {
    let report = report_eod_cashup(date.clone(), actor_user_id.clone(), state.clone()).await?;
    // Record Z-report issuance in audit log
    let device_id: String = sqlx::query_scalar(
        "SELECT device_id FROM devices WHERE is_active=1 ORDER BY device_code LIMIT 1",
    )
    .fetch_optional(&state.db).await?.flatten().unwrap_or_default();
    let branch_id: String = sqlx::query_scalar(
        "SELECT branch_id FROM branches WHERE is_active=1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(&state.db).await?.flatten().unwrap_or_default();
    let _ = audit_hash::insert_audit_entry(
        &state.db, "Z_REPORT_ISSUED", "report", &date,
        &actor_user_id, "user", &device_id, &branch_id,
        None, None, None,
    ).await;
    Ok(report)
}
```

Register it in `lib.rs` `generate_handler!`:
```rust
report_commands::report_z_report,
```

- [ ] **Step 4: Verify and commit**
```bash
cd zanpos/src-tauri && cargo check 2>&1 | tail -20
git add src-tauri/src/commands/phase10a_commands.rs src-tauri/src/commands/report_commands.rs src-tauri/src/lib.rs
git commit -m "fix: tax and EOD reports scoped to device, add Z-report command"
```

---

## Task 11 — WhatsApp: offline guard + phone normalisation

**Fixes:** H-18, M-15

**Files:** `src-tauri/src/commands/whatsapp_commands.rs`

- [ ] **Step 1: Add normalize_phone helper**

Add this function to `whatsapp_commands.rs`:
```rust
/// Normalize a phone number: strip spaces/hyphens, ensure single '+' prefix,
/// remove any doubled country code (e.g. +973973... → +973...).
fn normalize_phone(raw: &str) -> String {
    let stripped: String = raw.chars().filter(|c| c.is_ascii_digit() || *c == '+').collect();
    // Remove leading duplicate country code (e.g. "973973" → "973" for Bahrain)
    let digits_only: String = stripped.trim_start_matches('+').to_string();
    // Heuristic: if number starts with known CC repeated twice, strip the duplicate
    for cc in &["973", "966", "971", "965", "968", "974", "967"] {
        let double = format!("{}{}", cc, cc);
        if digits_only.starts_with(&double) {
            return format!("+{}", &digits_only[cc.len()..]);
        }
    }
    format!("+{}", digits_only)
}
```

- [ ] **Step 2: Add is_network_available check**

Add a lightweight helper:
```rust
async fn is_network_available() -> bool {
    use tokio::net::TcpStream;
    use tokio::time::{timeout, Duration};
    timeout(Duration::from_secs(2), TcpStream::connect("8.8.8.8:53"))
        .await
        .is_ok()
}
```

- [ ] **Step 3: Guard all WA send commands with offline check**

At the top of `whatsapp_send_delivery`, `whatsapp_notify_arrival`, `whatsapp_payment_reminder`, add:
```rust
if !is_network_available().await {
    return Err(AppError::Internal("WhatsApp: device is offline".into()));
}
```

- [ ] **Step 4: Apply normalize_phone at all call sites**

Wherever a phone number is passed to `send_raw` (or equivalent), wrap it:
```rust
let phone = normalize_phone(&raw_phone);
```

- [ ] **Step 5: Extract _impl helpers for delivery-triggered calls**

Add pub(crate) impl wrappers used by delivery_commands.rs (Task 9):
```rust
pub(crate) async fn whatsapp_send_delivery_impl(
    state: &crate::AppState,
    phone: &str,
    delivery_order_id: &str,
) -> AppResult<()> {
    if !is_network_available().await {
        return Ok(()); // Best-effort — don't fail the delivery status update
    }
    let phone = normalize_phone(phone);
    // Read sidecar token and call sidecar endpoint
    // ... (same logic as whatsapp_send_delivery command, but with passed-in params)
    Ok(())
}

pub(crate) async fn whatsapp_notify_arrival_impl(
    state: &crate::AppState,
    phone: &str,
    delivery_order_id: &str,
) -> AppResult<()> {
    if !is_network_available().await { return Ok(()); }
    let phone = normalize_phone(phone);
    // ... call sidecar notify endpoint
    Ok(())
}

pub(crate) async fn whatsapp_payment_reminder_impl(
    state: &crate::AppState,
    phone: &str,
    delivery_order_id: &str,
) -> AppResult<()> {
    if !is_network_available().await { return Ok(()); }
    let phone = normalize_phone(phone);
    // ... call sidecar payment reminder endpoint
    Ok(())
}
```

- [ ] **Step 6: Verify and commit**
```bash
cd zanpos/src-tauri && cargo check 2>&1 | tail -20
git add src-tauri/src/commands/whatsapp_commands.rs
git commit -m "fix: WA offline guard, phone normalisation, impl helpers for delivery triggers"
```

---

## Task 12 — Setup: non-blocking run_once + seed device delete fix

**Fixes:** H-22, H-23, M-18

**Files:** `src-tauri/src/commands/setup_commands.rs`, `src-tauri/src/commands/sync_commands.rs`

- [ ] **Step 1: Make setup_wizard_complete non-blocking on network**

In `setup_commands.rs`, find `state.sync_worker.run_once().await` inside `setup_wizard_complete`. Replace with:
```rust
let worker_clone = state.sync_worker.clone();
tauri::async_runtime::spawn(async move {
    let _ = worker_clone.run_once().await;
});
```

- [ ] **Step 2: Make admin_setup_supabase non-blocking on network**

In `sync_commands.rs`, find `state.sync_worker.run_once().await` inside `admin_setup_supabase`. Same fix:
```rust
let worker_clone = state.sync_worker.clone();
tauri::async_runtime::spawn(async move {
    let _ = worker_clone.run_once().await;
});
```

- [ ] **Step 3: Fix seed device delete to use stable device_id**

In `setup_commands.rs`, find:
```rust
// Something like:
sqlx::query("DELETE FROM devices WHERE device_code = '01JDEVICE0000000000000001' OR device_code = 'POS01'")
```

Replace with a migration-coordinated approach. First, check what the seeded device_id is in the migration file (grep for the seed INSERT):
```bash
grep -r "01JDEVICE\|seed.*device\|INSERT.*devices" src-tauri/src/db/ | head -20
```

Then replace the hardcoded delete with:
```rust
// Delete devices that were created as seed/placeholder (not real terminals)
// Use is_seed flag or match the stable seed device_id from migrations
sqlx::query(
    "DELETE FROM devices WHERE device_id IN (
       SELECT device_id FROM devices
       WHERE is_active = 0 AND device_code IN ('POS01', '01JDEVICE0000000000000001')
     )"
)
.execute(&state.db)
.await?;
```

If there's a stable seed device_id constant in the codebase, use it directly.

- [ ] **Step 4: Verify and commit**
```bash
cd zanpos/src-tauri && cargo check 2>&1 | tail -20
git add src-tauri/src/commands/setup_commands.rs src-tauri/src/commands/sync_commands.rs
git commit -m "fix: setup run_once is non-blocking, seed device delete uses stable ID"
```

---

## Task 13 — Security: schema grants + keyring hardening

**Fixes:** H-25, M-20, L-10

**Files:** `src-tauri/src/sync/central_schema.rs`, `src-tauri/src/secure_store.rs`

- [ ] **Step 1: Replace blanket GRANT ALL with granular grants on audit_logs**

In `central_schema.rs`, find the `GRANT ALL ON ALL TABLES` statement. Replace (or supplement with an explicit override after it):
```sql
-- Revoke destructive permissions on append-only tables
REVOKE UPDATE, DELETE ON audit_logs FROM anon;
REVOKE UPDATE, DELETE ON audit_logs FROM authenticated;
GRANT SELECT, INSERT ON audit_logs TO anon;
GRANT SELECT, INSERT ON audit_logs TO authenticated;
GRANT ALL ON audit_logs TO service_role;
```

Alternatively, if the schema uses individual GRANT statements, ensure `audit_logs` only gets `SELECT, INSERT` for `anon` and `authenticated`.

- [ ] **Step 2: Add write-then-readback in set_secret**

In `secure_store.rs`, replace `set_secret`:
```rust
pub fn set_secret(key: &str, value: &str) -> bool {
    let entry = match Entry::new(SERVICE, key) {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("keyring: failed to create entry for '{}': {}", key, e);
            return false;
        }
    };
    if let Err(e) = entry.set_password(value) {
        tracing::error!("keyring: failed to write '{}': {}", key, e);
        return false;
    }
    // Read-back verification
    match entry.get_password() {
        Ok(stored) if stored == value => true,
        Ok(_) => {
            tracing::error!("keyring: readback mismatch for '{}' — stored value differs", key);
            false
        }
        Err(e) => {
            tracing::error!("keyring: readback failed for '{}': {}", key, e);
            false
        }
    }
}
```

- [ ] **Step 3: Log keyring read errors in get_secret**

Replace `get_secret`:
```rust
pub fn get_secret(key: &str) -> Option<String> {
    let entry = Entry::new(SERVICE, key).ok()?;
    match entry.get_password() {
        Ok(v) => Some(v),
        Err(keyring::Error::NoEntry) => None, // Not set — silent
        Err(e) => {
            tracing::warn!("keyring: read failed for '{}': {} — falling back to DB", key, e);
            None
        }
    }
}
```

- [ ] **Step 4: Verify and commit**
```bash
cd zanpos/src-tauri && cargo check 2>&1 | tail -20
git add src-tauri/src/sync/central_schema.rs src-tauri/src/secure_store.rs
git commit -m "fix: audit_logs append-only grants, keyring write-then-readback, log read errors"
```

---

## Task 14 — SyncChip: inline error text

**Fixes:** M-21, M-22

**Files:** `src/components/SyncChip.tsx`

- [ ] **Step 1: Surface stuck-push error inline (online-with-errors case)**

Find the `sync-has-error` case in `SyncChip.tsx` (around lines 35–41). Replace the span that only puts the error in `title`:
```tsx
{/* OLD — error only in title tooltip */}
<span className="sync-chip sync-has-error" title={errorTitle}>
  <span className="sync-chip-error-marker" aria-label="sync error">!</span>
  {' '}Online{pending ? ` — Syncing (${status.pending_events})` : " (with errors)"}
</span>
```
with:
```tsx
{/* NEW — error shown inline, truncated */}
<span className="sync-chip sync-has-error" title={errorTitle}>
  <span className="sync-chip-error-marker" aria-label="sync error">!</span>
  {' '}Online (with errors)
  {status.last_error && (
    <span className="sync-error-inline" style={{ fontSize: '0.75em', marginLeft: 4, opacity: 0.85 }}>
      — {status.last_error.slice(0, 60)}{status.last_error.length > 60 ? '…' : ''}
    </span>
  )}
</span>
```

- [ ] **Step 2: Surface offline error inline**

Find the offline state case (around lines 62–78). Find the `errorSuffix` variable and ensure it renders as inline text:
```tsx
{/* After "Offline" text, add: */}
{err && (
  <span className="sync-error-inline" style={{ fontSize: '0.75em', marginLeft: 4, opacity: 0.85 }}>
    — {errorSuffix}
  </span>
)}
```

- [ ] **Step 3: Verify TypeScript**
```bash
cd zanpos && npx tsc --noEmit 2>&1 | tail -10
```

- [ ] **Step 4: Commit**
```bash
git add src/components/SyncChip.tsx
git commit -m "fix: SyncChip shows stuck-push error inline, not only on hover"
```

---

## Task 15 — AI / Migration fixes

**Fixes:** M-23, M-24, M-25, H-26, H-27, H-28

**Files:** `src-tauri/src/commands/ai_admin_commands.rs`, `src-tauri/src/db/repositories/ai_admin_repo.rs`, `src-tauri/src/commands/migration_commands.rs`, `src-tauri/src/lib.rs`

- [ ] **Step 1: Add admin_validate_anthropic command**

In `ai_admin_commands.rs`, add:
```rust
#[derive(serde::Serialize)]
pub struct ValidateProviderResult {
    pub valid: bool,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn admin_validate_anthropic(
    api_key: String,
    _state: State<'_, AppState>,
) -> Result<ValidateProviderResult, AppError> {
    // Ping Anthropic's models list endpoint to verify key validity
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.anthropic.com/v1/models")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => Ok(ValidateProviderResult { valid: true, error: None }),
        Ok(r) => Ok(ValidateProviderResult {
            valid: false,
            error: Some(format!("HTTP {}", r.status())),
        }),
        Err(e) => Ok(ValidateProviderResult {
            valid: false,
            error: Some(e.to_string()),
        }),
    }
}
```

Register in `lib.rs`:
```rust
ai_admin_commands::admin_validate_anthropic,
```

- [ ] **Step 2: Add actor_type='ai' to ai_actions table and INSERT**

Check if `ai_actions` table has an `actor_type` column. If not, add a migration:
```sql
ALTER TABLE ai_actions ADD COLUMN IF NOT EXISTS actor_type TEXT NOT NULL DEFAULT 'ai';
```

In `ai_admin_repo.rs`, find `create_action`. Add `actor_type` to the INSERT column list with value `'ai'`:
```rust
sqlx::query(
    "INSERT INTO ai_actions
       (action_id, session_user_id, actor_type, tool_name, tool_input_json,
        tool_input_hash, preview_text, status, confirmation_token, prepared_at, expires_at)
     VALUES (?, ?, 'ai', ?, ?, ?, ?, ?, ?, ?, ?)",
)
```

- [ ] **Step 3: Add origin_device_id to migration insert_sale**

In `migration_commands.rs`, find `insert_sale` (around line 2406). Add `origin_device_id` to the INSERT:

```rust
// In the INSERT column list, add: origin_device_id
// In the VALUES, add: ?
// In the bind chain, add: .bind(device_id)
```

The `device_id` is already available from `ensure_import_anchors`. Thread it down to `insert_sale` as a parameter.

- [ ] **Step 4: Add origin_device_id to migration insert_sale_item**

Same fix in `insert_sale_item` (around line 2483). Add `origin_device_id` parameter and bind.

- [ ] **Step 5: Add origin_device_id to ensure_import_anchors shift INSERT**

In `ensure_import_anchors` (around line 2341), find the synthetic shift INSERT. Add `origin_device_id` to the column list and bind `device_id`.

- [ ] **Step 6: Add audit log entry to migration_execute**

At the end of `migration_execute`, after the migration completes successfully:
```rust
let device_id: String = // resolve from DB
let branch_id: String = // resolve from DB
let after = serde_json::json!({
    "migration_id": migration_id,
    "rows_imported": total_rows,
    "source_file": source_file_name,
}).to_string();
let _ = audit_hash::insert_audit_entry(
    &state.db, "MIGRATION_EXECUTED", "migration", &migration_id,
    &actor_user_id, "user", &device_id, &branch_id,
    None, Some(&after), None,
).await;
```

- [ ] **Step 7: Verify and commit**
```bash
cd zanpos/src-tauri && cargo check 2>&1 | tail -20
git add src-tauri/src/commands/ai_admin_commands.rs src-tauri/src/db/repositories/ai_admin_repo.rs src-tauri/src/commands/migration_commands.rs src-tauri/src/lib.rs
git commit -m "fix: admin_validate_anthropic, ai_actions actor_type, migration origin_device_id + audit"
```

---

## Task 16 — Remaining medium and low fixes

**Fixes:** M-1 (override token), M-16 (transient detection), M-17 (RPC body), L-6 (low stock filter), L-1 (cash formula), L-3 (product_repo query), M-12 (receipt reprint), L-9 (pull error tagging)

**Files:** Various

- [ ] **Step 1: [L-6] Fix low-stock false positives (reorder_point=0)**

In `src-tauri/src/inventory/stock_repo.rs`, find `get_low_stock`. Add `AND p.reorder_point > 0` to the WHERE clause.

- [ ] **Step 2: [L-3] Add default_supplier_id to PRODUCT_QUERY**

In `src-tauri/src/db/repositories/product_repo.rs`, find `PRODUCT_QUERY`. Add `p.default_supplier_id` to the SELECT list.

- [ ] **Step 3: [M-16] Replace transient tag substring matching with typed error variant**

In `src-tauri/src/sync/worker.rs`, find the `contains(TRANSIENT_TAG.trim())` check. The cleanest fix without a major refactor: ensure the TRANSIENT_TAG is always prepended reliably by checking the raw tag string instead of trimming:

```rust
// In supabase_client.rs, ensure TRANSIENT_TAG = "[TRANSIENT]" (no trailing space)
pub const TRANSIENT_TAG: &str = "[TRANSIENT]";

// In worker.rs, check:
if e.to_string().contains("[TRANSIENT]") {
    // transient — retry
} else {
    // permanent — mark failed
}
```

For a more robust fix, introduce a typed wrapper:
```rust
// In errors.rs:
pub enum SyncPushError {
    Transient(String),
    Permanent(String),
}
```
Then `push_event` returns `Result<(), SyncPushError>` and the worker matches on variant. (This is the full fix; the substring-check fix above is acceptable for now.)

- [ ] **Step 4: [M-17] Validate RPC response body**

In `src-tauri/src/sync/supabase_client.rs`, find the success branch of `push_event` where `status.is_success()`. Add response body validation:
```rust
let body = resp.text().await.unwrap_or_default();
if !body.contains("\"ok\"") && !body.trim_matches('"') == "ok" {
    tracing::warn!("sync push: unexpected RPC response body: {}", &body[..body.len().min(200)]);
    // Don't fail — log only, per spec intent (RPC should return "ok" but partial success is ok)
}
```

- [ ] **Step 5: [L-9] Tag pull network errors as TRANSIENT**

In `supabase_client.rs`, find `pull_events`. When a network/send error occurs, prepend the TRANSIENT_TAG:
```rust
Err(e) => Err(AppError::Internal(format!("{} Supabase pull error: {}", crate::sync::supabase_client::TRANSIENT_TAG, e)))
```

Also differentiate 4xx from 5xx in pull response handling.

- [ ] **Step 6: [M-1] Persist override tokens to DB**

In `src-tauri/src/commands/override_token.rs`, find the in-process `HashMap`. Add a DB-backed table fallback. The simplest fix: write the token to `app_config` with key `override_token_{token_id}` and expiry stored in value as JSON, then delete it on consume:

```rust
// On issue:
let token_json = serde_json::json!({
    "manager_user_id": manager_user_id,
    "expires_at": expires_at_rfc3339,
}).to_string();
sqlx::query(
    "INSERT INTO app_config(key, value) VALUES(?, ?)
     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
)
.bind(format!("override_token_{}", token))
.bind(&token_json)
.execute(pool)
.await?;

// On verify/consume:
let stored: Option<String> = sqlx::query_scalar(
    "SELECT value FROM app_config WHERE key = ?",
)
.bind(format!("override_token_{}", token))
.fetch_optional(pool)
.await?
.flatten();
// Parse and validate expiry, then delete
sqlx::query("DELETE FROM app_config WHERE key = ?")
    .bind(format!("override_token_{}", token))
    .execute(pool)
    .await?;
```

Keep the in-memory HashMap as a fast-path cache. The DB write is the durable record.

- [ ] **Step 7: [L-1] Align cash_drawer_summary and close_shift expected-cash formulas**

In `cash_commands.rs`, find `drawer_summary_inner` (the expected_minor computation). In `shift_repo.rs`, find `close_shift` expected computation. Extract the shared SQL logic into a helper or ensure both use the same query for cash_sales (either both include or both exclude pending deliveries). Add a comment documenting the decision.

- [ ] **Step 8: [L-7] Add origin_device_id to customers table**

Add a migration file (find the next migration number in `src-tauri/src/db/mod.rs`):
```sql
ALTER TABLE customers ADD COLUMN IF NOT EXISTS origin_device_id TEXT NOT NULL DEFAULT '';
```

In `customer_commands.rs`, `customer_create` INSERT, add `origin_device_id` to the column list and bind the `device_id` value.
In `customer_update` UPDATE, do NOT update `origin_device_id` (it is immutable after creation).

Register the migration in `db/mod.rs`.

- [ ] **Step 9: [M-12] Audit receipt_reprint for pruning safety**

In `refund_commands.rs`, find `receipt_reprint`. Check if it queries `FROM sales WHERE sale_id = ?`. If so, add a note: if the sale is older than 90 days and has been pruned, the reprint will fail. Acceptable solutions:
1. Return a clear error: "This receipt was pruned and cannot be reprinted."
2. Store receipt snapshots in a separate `receipt_archive` table populated at finalize_sale.

For now, add a clear error message in the NotFound case:
```rust
.ok_or_else(|| AppError::NotFound(
    "Receipt not found. Sales older than 90 days may have been pruned from this device.".into()
))?
```

- [ ] **Step 9: Final full build verification**
```bash
cd zanpos/src-tauri && cargo check 2>&1
cd zanpos/src-tauri && cargo test 2>&1
cd zanpos && npx tsc --noEmit 2>&1
```
Expected: all three pass with zero errors.

- [ ] **Step 10: Final commit**
```bash
git add -p  # stage each remaining changed file
git commit -m "fix: low-stock filter, product_repo query, transient detection, pull error tagging, override token persistence, cash formula alignment, reprint error message"
```

---

## Post-Implementation Checklist

After all tasks complete, verify:

- [ ] `cargo check` passes with zero errors
- [ ] `cargo test` passes (or pre-existing failures only)
- [ ] `npx tsc --noEmit` passes
- [ ] Run `audit_verify_chain` on a test device — new rows should have `before_json`/`reason` in hash
- [ ] Open shift → ring a sale → check sync_queue — enqueue should be in the same tx as sale INSERT (test by killing the process mid-finalize and verifying no orphan sale without queue entry)
- [ ] Inventory receive: verify `movement_type='receive'` and `reference_type='receive'` in stock_movements
- [ ] Tax report: verify result scopes to this device's origin_device_id
- [ ] WhatsApp send: verify offline returns error immediately
- [ ] Customer loyalty add: verify sync_queue has a customer upsert row after the update

---

## Notes on Deviations Not Fixed in This Plan

- **H-21 (Join store service_role key):** This is an architectural decision that may be intentional (no RLS on the deployment). Document in spec as "service_role key accepted at join" rather than anon key. No code change until RLS strategy is confirmed.
- **M-26 (~110 vs ~140 commands):** The count discrepancy needs manual reconciliation against the spec's command list. This is a discovery task, not a code fix — identify which commands are genuinely missing.
- **A-1 (20s interval — duplicate of H-20):** Fixed in Task 1.
