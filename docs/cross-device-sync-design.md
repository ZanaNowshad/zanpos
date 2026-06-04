# ZANPOS — Cross-Device Sync Design (Ship 2 & 3)

**Status:** Implementation plan. Supersedes the "device owns; NOT imported" model in `sync-conflict-resolution.md` §2 Class B.

**Scope:** Single Supabase project serving multiple POS devices in one branch. NOT multi-branch, NOT multi-tenant.

---

## 0. Problem Statement

The current sync model (`sync-conflict-resolution.md`) treats each device as the sole writer of its own transactional tables. This means:

1. Manager on Device B cannot see Device A's sales in reports.
2. Manager on Device B cannot refund a sale made on Device A (no data).
3. Stock levels diverge silently because `stock_movements` are pulled but `stock_levels` is not updated.
4. Operator has no honest view of sync health.

The goal is to make **all transactional events visible on all devices** (read), while preserving **device-local authority** for writes.

---

## 1. New Architecture

```
POS Device (SQLite)         POS Device (SQLite)         POS Device (SQLite)
   ▲                              ▲                              ▲
   │ pull (apply)                 │ pull (apply)                 │ pull (apply)
   │                              │                              │
   └──────►  Supabase Central DB  ◄──────┘  ◄──────────────────────┘
                  │  ▲
                  │  │ push (origin_device_id stamped)
                  ▼  │
             sync_queue (per device, FIFO)
```

**Read model:** Every device receives a copy of every sale/payment/refund/shift from every other device. Cross-device reports are local queries; no Supabase round-trip needed.

**Write model:** Each device's writes are stamped with `origin_device_id`. Local CRUD remains device-authoritative — a sale is created on the device that rang it up. Remote events are applied read-only.

**Stock model:** `stock_movements` is the source of truth. `stock_levels.quantity_on_hand` is a derived cache, recomputable from the ledger. Drift detection fires when local sum diverges from cached value.

---

## 2. Entity Classes (Revised)

### Class A — Mutable Catalog (LWW by `updated_at`)

**Unchanged.** Products, categories, tax_rules, users. Pull-only from central.

### Class B — Transactional (Device-Local Write, Cross-Device Read)

| Entity | Local write | Remote apply | Conflict |
|---|---|---|---|
| Sales | Local | INSERT (idempotent on sale_id) | None (ULID unique) |
| Sale items | Local | INSERT (idempotent) | None |
| Payments | Local | INSERT | None |
| Refunds | Local | INSERT | None |
| Refund items | Local | INSERT | None |
| Shifts | Local | INSERT/UPDATE | None |
| Audit logs | Local | INSERT | None |

**Receipt number scoping:** `{branch_code}-{device_code}-{NNNNNNNN}`. Each device owns its own receipt number sequence. Cross-device queries don't need a global sequence — the device_code is the natural partition.

**Read cross-device:** Reports query local DB; remote events are now applied locally so all data is present.

**Authorization for cross-device writes (refunds):** A cashier on Device B cannot refund a sale from Device A without manager override. See §5.

### Class C — Stock (Ledger-Authoritative, Cache-Derived)

| Table | Source of truth | Local cache | Conflict |
|---|---|---|---|
| `stock_movements` | Append-only ledger | — | None (PK dedup) |
| `stock_levels` | — | `SUM(stock_movements.qty_delta)` per (product, branch) | Drift → reconcile |

**Reconciliation:** A drift detector runs after every pull batch. If `local_cached.quantity_on_hand` differs from `SUM(stock_movements.qty_delta) WHERE product_id=? AND branch_id=?` by more than `tolerance` (default 0.001 units), the device emits a `stock_drift_detected` event and surfaces it on the dashboard. Manager chooses: accept ledger sum (default) or local cached value (with reason).

### Class D — Append-Only Catalog (Deduplication by PK)

**Unchanged.** Product prices.

---

## 3. Origin Device Tracking (Phase B)

**Goal:** Every event written by a device carries `origin_device_id`, never null, never empty.

**Schema changes** (`central_schema.rs` and the central Postgres migrations):

```sql
-- sync_events
ALTER TABLE sync_events ADD COLUMN origin_device_id UUID NOT NULL
  CHECK (origin_device_id IS NOT NULL);
ALTER TABLE sync_events ADD CONSTRAINT sync_events_origin_fk
  FOREIGN KEY (origin_device_id) REFERENCES devices(device_id);

-- sales
ALTER TABLE sales ADD COLUMN origin_device_id UUID NOT NULL
  REFERENCES devices(device_id);
-- (same for payments, refunds, refund_items, shifts, audit_logs)
```

**Outbox change** (`outbox.rs`): Every `enqueue_event` reads the device id from a process-level singleton (already set at startup from `app_config.device_id`) and injects it into the payload. The CHECK constraint is the safety net: any code path that forgets gets a hard error at the database layer, not a silent empty string.

**Local SQLite equivalent** (`migrations/*.sql`):

```sql
ALTER TABLE sales ADD COLUMN origin_device_id TEXT NOT NULL DEFAULT '';
-- backfill from app_config.device_id
UPDATE sales SET origin_device_id = (SELECT value FROM app_config WHERE key='device_id');
-- then
ALTER TABLE sales ADD CONSTRAINT sales_origin_nn CHECK (origin_device_id != '');
```

The two-step migration (DEFAULT '' → UPDATE → DROP DEFAULT → CHECK) avoids breaking existing rows.

---

## 4. Inbox Applying Remote Events (Phase C)

**Current state** (`inbox.rs:441`): The `apply_event` match arm silently drops `sale | sale_item | payment | refund | refund_item | shift | audit_log`. These are the events we now want to import.

**New behaviour:** Add a remote-apply branch that:

1. Opens a transaction.
2. Decodes the payload to the entity struct.
3. INSERTs (not UPSERTs) into the local table.
4. On `UNIQUE` violation → ack and skip (already applied).
5. Commits the transaction.

**Order of apply:** Events must be applied in `global_sequence ASC` order. The pull query already orders this way; the worker just needs to process the batch in array order, not parallelize.

**Watermark fix** (`worker.rs:451-466`): The watermark must advance only after the local apply transaction commits. Currently it advances in the same SQL statement as the event apply, which means a network blip between apply commit and watermark write can cause re-pull and double-apply. The UNIQUE constraints absorb the double-apply, but it's wasted work. New order:

1. `BEGIN`
2. Apply event
3. `UPDATE sync_state SET last_pulled_central_sequence = ?`
4. `COMMIT`
5. (or batch: apply all → advance watermark once → commit)

The single-transaction-per-batch approach is preferred for atomicity.

**Receipt collision guard** (`sale_repo.rs:11-26`): When a device allocates a new local receipt number, it must check that the same number does not exist remotely. Since receipts are device-scoped (`{device_code}` prefix), this is already safe — the check is unnecessary. But the migration of historical data must preserve receipt numbers exactly, including any legacy single-prefix receipts (back-fill with empty `device_code` allowed; only NEW inserts get the device prefix).

**Refund cross-device authorization** (Phase F, see §5) is the policy layer above this.

---

## 5. Stock Drift Detection (Phase D-Prime)

**Current state** (`inbox.rs:226-261`): The `apply_stock_movement` arm pulls remote movements but does NOT update `stock_levels.quantity_on_hand`. The cache stays stale.

**Fix:**

```rust
"stock_movement" => {
    // Existing: INSERT OR IGNORE into stock_movements (ledger)
    // NEW: recompute cached level
    let sum: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(SUM(quantity_delta), 0) FROM stock_movements
         WHERE product_id = ? AND branch_id = ?"
    ).bind(product_id).bind(branch_id).fetch_one(...).await?;
    let stored: Decimal = sqlx::query_scalar(
        "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?"
    ).bind(product_id).bind(branch_id).fetch_one(...).await?;
    if (sum - stored).abs() > tolerance {
        // emit drift event
    }
    // accept ledger as truth
    sqlx::query(
        "UPDATE stock_levels SET quantity_on_hand = ? WHERE product_id = ? AND branch_id = ?"
    ).bind(sum).bind(product_id).bind(branch_id).execute(...).await?;
}
```

**Drift event:** A row in `sync_queue` with `entity_type='stock_drift_detected'` carrying `{product_id, branch_id, local_cached, ledger_sum, diff}`. The dashboard surfaces these for manager review.

**Tolerance:** 0.001 units (sub-cent precision; safe for BHD's 3-decimal currency and for any product unit).

**Performance:** For a 1000-SKU store with 10 pulls/min, that's 10k recomputes/min. Each is a single `SUM()` indexed on (product_id, branch_id). Acceptable for a POS terminal. If load gets out of hand, batch the recompute nightly instead of per-event.

---

## 6. Reports Cross-Device Toggle (Phase E)

**Default:** `device_scope = 'origin'` (each device reports its own data only). Preserves existing single-device behaviour, no surprises.

**Toggle:** In Reports view, a radio button `[This device] [All devices]`. The query gets a `WHERE origin_device_id = ?` (origin) or no filter (all). Both queries run against the local SQLite; no Supabase round-trip.

**Migration:** Add `app_config.reports_device_scope TEXT NOT NULL DEFAULT 'origin' CHECK (reports_device_scope IN ('origin', 'all'))`.

---

## 7. Manager Cross-Device Refund Authorization (Phase F)

**Threat model:** A cashier on Device B has the ability to call `refundSale` on a sale made on Device A. The current RBAC only checks "can this user refund at all", not "can this user refund a sale that was not made on their device". This is an authorization gap.

**Policy:**
- Cashier can refund sales where `origin_device_id = current_device_id` without override.
- Cashier can refund sales where `origin_device_id != current_device_id` only with a **manager override** (manager PIN entry in the UI, audit log entry).
- Manager can refund any sale without override (subject to normal refund limits).

**UI flow:**
1. Cashier selects a sale from "All devices" view → refund button is shown but disabled.
2. Tooltip: "Cross-device refund requires manager override."
3. Click → PIN entry modal → manager enters PIN → backend validates → refund proceeds.
4. Audit log records: `actor_id` = manager, `target_sale_id`, `override_used = true`, `origin_device_id`.

**Backend check** (`refund_commands.rs`):
```rust
let sale = sale_repo::get_sale(sale_id)?;
let user = auth::current_user()?;
if sale.origin_device_id != current_device_id
   && !rbac::can_override_refund(user) {
    return Err(RefundError::ManagerOverrideRequired);
}
if sale.origin_device_id != current_device_id
   && rbac::can_override_refund(user) {
    audit::log_override(user, sale_id, "cross_device_refund");
}
```

**PIN verification:** Manager PIN is hashed (argon2id) and stored in `users.pin_hash`. The frontend never sees the manager's PIN; it submits it to `validate_manager_pin(pin)` which returns a short-lived (60s) override token, which the refund command then validates.

---

## 8. Outbox Idempotency & Replay Safety

**No change.** Every event has `idempotency_key`; the inbox apply uses `INSERT OR IGNORE` / `ON CONFLICT DO NOTHING`. A replayed event from a network blip is silently absorbed. This applies equally to Class B and Class C events.

---

## 9. Failure Modes & Mitigations

| Failure | Detection | Mitigation |
|---|---|---|
| Origin device id lost (bug) | `CHECK (origin_device_id != '')` fires | Hard error at enqueue; cannot ship without device id |
| Receipt number collision (multi-device) | Impossible (device_code prefix) | N/A |
| Stock drift > tolerance | Drift detector emits event | Manager accepts ledger sum; audit log records decision |
| Watermark regression on crash | `global_sequence` is monotonic; watermark can only go forward | On startup, watermark `GREATEST(stored, max(global_sequence_seen))` |
| `apply_sync_event` RPC not idempotent (Supabase side) | Document gap | Server-side review checklist (existing §3 action) |
| Cross-device refund by cashier | RBAC check | Manager override required; audit log |
| Pull from Supabase contains corrupt event | Payload CRC32 check | Log warning, skip, advance watermark (event lost — alert) |

---

## 10. Ship 3 — Cross-Device Refund Authorization (Phase F)

**Status:** Implemented. Closes the authorization gap for refunds across devices.

### 10.1 Threat Model

A refund is the financial inverse of a sale. Without cross-device authorization, a cashier on Device B who can see Device A's sales (from Ship 2's inbox apply) could refund them — creating a shadow payout with no physical cash return.

**Threats:**

| # | Threat | Severity | Mitigation |
|---|--------|----------|------------|
| T1 | Cashier on Device B refunds sale from Device A without manager knowledge | High | Manager PIN override required for cross-device refunds by non-managers |
| T2 | Cashier caught short funds own device's refunds to cover drawer gap | Medium | Same-device refunds allowed; audit trail provides accountability |
| T3 | Manager PIN shoulder-surfed by cashier | Medium | Short-lived (60s) single-use override token; audit log records `override_used=true` |
| T4 | Override token replayed after expiry | Low | Token store prunes expired tokens on every consume; single-use |
| T5 | Manager refunds cross-device without audit trail | Low | All cross-device refunds set `override_used=true` in audit log |

### 10.2 Policy Matrix

| Caller Role | Same Device | Cross-Device |
|-------------|-------------|--------------|
| Owner | Allowed | Allowed (override_used logged) |
| Manager | Allowed | Allowed (override_used logged) |
| Cashier | Allowed | **Required**: Manager PIN override → 60s token → audit log with `override_used=true` |

### 10.3 Implementation Components

**Migration 0034:** `ALTER TABLE audit_logs ADD COLUMN override_used INTEGER NOT NULL DEFAULT 0` with SQLite not-null trigger.

**Override token store** (`commands/override_token.rs`): In-memory `Mutex<HashMap<String, OverrideToken>>`. Token = ULID, TTL = 60s, single-use (consumed on validation). Stored on `validate_manager_pin` success, consumed by `refund_create`.

**RBAC extension** (`commands/rbac.rs`): `can_override_refund(user_id)` — returns `true` for manager or owner.

**New command `auth_validate_manager_pin`:** Accepts a PIN, verifies it against all active manager/owner accounts, returns a single-use override token valid for 60s.

**`refund_get_sale` RBAC relaxation:** Changed from `manager_or_owner` to `require_any_role`. Cashiers can look up receipts (for same-device refunds). Returns `origin_device_id`.

**`refund_create` cross-device check:**
```
1. Resolve user role + current device id
2. Load sale → extract origin_device_id
3. If cross-device:
   a. Manager/owner → allow, log audit with override_used=true
   b. Cashier → validate override token (consumed), log audit with override_used=true
4. If same-device → allow for any active user, log normal audit
```

**Audit log:** `create_refund` takes `override_used: bool`. The inline audit insert and `insert_audit_entry` helper both set the column.

**Frontend:** `RefundModal` detects cross-device sales via `origin_device_id`, shows PIN entry modal for cashiers, passes `manager_override_token` to `refundCreate`.

### 10.4 Rollback Safety

- Override token store is in-memory only; app restart clears all tokens.
- `override_used` defaults to 0; existing audit rows unaffected.
- Cashiers blocked from cross-device by absence of valid token. No config or migration rollback needed to restore the gate.

## 11. Rollout

1. **Ship 1** (done): SyncChip error surfacing, KPI card 4-state, Change connection flow.
2. **Ship 2** (done): Origin columns → inbox apply → stock recompute → reports toggle.
3. **Ship 3** (this phase): Manager cross-device refund authorization. RBAC completes.

**Rollback plan:** Each phase is independently shippable. If Ship 3 breaks, revert `refund_create` to `manager_or_owner` guard and remove the new command registration — Ship 2 cross-device data flow continues unaffected.
