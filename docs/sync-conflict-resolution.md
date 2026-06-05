# ZANPOS — Sync Conflict Resolution

**Status:** SUPERSEDED by sync_v2 (2026-06-05). This document describes the v1 event-sourcing conflict resolution (apply_sync_event RPC, inbox.rs LWW dispatch). The active implementation handles conflicts via direct row-level LWW (`WHERE updated_at < excluded.updated_at`) and `INSERT OR IGNORE` in `src-tauri/src/sync_v2/worker.rs`.

---

## 1. Architecture Summary

ZANPOS uses a **device-authoritative, append-only outbox** sync model:

```
POS Device (SQLite)
  │
  │  sync_queue (pending → synced)
  │  30-second push cycle
  ▼
Supabase Central DB  ←→  apply_sync_event() RPC
  │
  │  pull_events (watermark-based, 30-second cycle)
  ▼
Other POS Devices (inbox.apply_event)
```

Each device writes to its **own** transactional tables (sales, payments, shifts, refunds). The central Supabase DB is the **archive and catalog distribution point**. Remote events are imported locally via inbox apply (INSERT OR IGNORE), making all data available for cross-device reports and refunds.

---

## 2. Entity Classes and Their Conflict Strategies

### Class A — Mutable Catalog (Last-Write-Wins by `updated_at`)

| Entity | Table | Strategy |
|--------|-------|----------|
| Products | `products` | LWW on `updated_at` |
| Categories | `categories` | LWW on `updated_at` |
| Tax rules | `tax_rules` | LWW on `version` (no date guard) |
| Users | `users` | LWW on `updated_at` |

**Implementation (inbox.rs):**
```sql
ON CONFLICT(product_id) DO UPDATE SET
  name = excluded.name, ...
WHERE products.updated_at < excluded.updated_at
```

The `WHERE` clause is the conflict guard: a remote update is only applied if its `updated_at` is strictly newer than the local record's.

**Risk: Clock skew.** If Device A's clock is ahead of the central server by more than the round-trip latency, a stale update from Device A could incorrectly "win" over a newer central update. **Mitigation:** Ensure all POS devices use NTP time synchronization (Windows default).

---

### Class B — Append-Only Transactional (Device-Local Write, Cross-Device Read)

| Entity | Table | Strategy |
|--------|-------|----------|
| Sales | `sales` | INSERT OR IGNORE on `sale_id` |
| Sale items | `sale_items` | INSERT OR IGNORE on `sale_item_id` |
| Payments | `payments` | INSERT OR IGNORE on `payment_id` |
| Shifts | `shifts` | LWW UPSERT (UPDATE if local `opened_at` is older) |
| Refunds | `refunds` | INSERT OR IGNORE on `refund_id` |
| Refund items | `refund_items` | INSERT OR IGNORE on `refund_item_id` |
| Audit logs | `audit_logs` | INSERT OR IGNORE on `audit_log_id` |

**Receipt number scoping:** `{branch_code}-{device_code}-{NNNNNNNN}`. Each device owns its own receipt number sequence — no cross-device collision guard needed.

**Cross-device refunds (Ship 3):** A cashier on Device B can refund a sale from Device A only with a manager PIN override. The override is recorded as `override_used=true` in the audit log. See §6.

**Implementation (inbox.rs):** Remote events are applied as `INSERT OR IGNORE` into the local device's tables. On UNIQUE violation, the event is silently skipped (already applied). Shifts use an LWW UPSERT: local record is overwritten only if the remote shift has a more recent timestamp.

---

### Class C — Stock (Ledger-Authoritative, Cache-Derived)

| Entity | Table | Strategy |
|--------|-------|----------|
| Stock movements | `stock_movements` | Append-only (`INSERT OR IGNORE`) |
| Stock levels | `stock_levels` | Derived cache — recomputed from `SUM(stock_movements.qty_delta)` on every remote stock_movement apply |

**Drift detector:** After each pull batch, the local `stock_levels.quantity_on_hand` is compared to `SUM(stock_movements.qty_delta)` per (product, branch). If the absolute difference exceeds 0.001 units, a `stock_drift_detected` event is emitted. The ledger is always the source of truth.

**Risk: Concurrent stock deductions.** If two devices sell the same product simultaneously, each deducts from its local snapshot. The last device to sync overwrites the cache — but the drift detector catches this and reconciles from the ledger. No permanent data loss; only the derived cache is transiently incorrect.

---

### Class D — Append-Only Catalog (Deduplication by PK)

| Entity | Table | Strategy |
|--------|-------|----------|
| Product prices | `product_prices` | `INSERT OR IGNORE` on `price_id` |

Once a price is created, it is never updated — only superseded by a new price with a later `effective_from` date. No conflict is possible.

---

## 3. Outbox Idempotency

Every `sync_queue` entry has:
- A unique `idempotency_key`: `"{entity_type}-{entity_id}-{suffix}"`.
- `INSERT OR IGNORE` at enqueue time — duplicate enqueues are silently dropped.
- A SHA-256 `payload_hash`.

The Supabase `apply_sync_event` RPC is called with the `idempotency_key`. If the network times out after the server succeeds but before the client receives the 200, the client will retry. The RPC **must** be idempotent (upsert, not insert) to handle this.

---

## 4. Retry and Failure Behaviour

| Scenario | Behaviour |
|---|---|
| Network timeout during push | Event stays `status = 'failed'`, retried next 30-second cycle |
| HTTP 4xx (bad payload) | Event stays `failed`, `attempt_count` incremented; after 10 attempts, permanently failed |
| HTTP 5xx (server error) | Same as timeout — retry |
| Network reconnects mid-push | Push resumes from oldest `pending` event (ordered by `local_sequence`) |
| Supabase down for > 5 days | Events accumulate in `sync_queue`; auto-pruned only after 7 days of `synced` status. Pending events are never pruned. |

---

## 5. Pull and Watermark

- Pull uses a monotonically increasing `global_sequence` watermark stored per device in `sync_state.last_pulled_central_sequence`.
- Events from **this device** are excluded from the pull (`device_id=neq.{exclude_device}`).
- Pull batch size: 100 events per cycle.
- A bad event (missing required field) is logged as a warning and skipped; it does not block subsequent events.
- Watermark advances only after the local apply transaction commits (single-transaction-per-batch atomicity).

---

## 6. Cross-Device Refund Authorization (Ship 3)

| Caller Role | Same Device | Cross-Device |
|-------------|-------------|--------------|
| Owner | Allowed | Allowed (override_used logged) |
| Manager | Allowed | Allowed (override_used logged) |
| Cashier | Allowed | **Required**: Manager PIN override → 60s single-use token |

**Override mechanism:** The `auth_validate_manager_pin` endpoint validates a PIN against all active manager/owner accounts and returns a ULID-based token (TTL 60s, single-use). The token is consumed by `refund_create` and the audit log records `override_used=true`. Token store is in-memory only — app restart clears all tokens.

---

## 7. Origin Device Tracking

Every transactional event carries `origin_device_id` (never null, never empty, enforced via SQLite BEFORE INSERT trigger from migration 0032). This column is the foundation for:
- Cross-device report scoping (`origin` vs `all`; see `reports_device_scope` in `app_config`)
- Cross-device refund authorization (compare `sale.origin_device_id` to current device)
- Stock drift attribution (which device's movement caused the drift)

---

## 8. Known Gaps — Pre-Production Action Items

| Gap | Severity | Mitigation |
|-----|----------|------------|
| No `apply_sync_event` RPC idempotency verified | High | Review Supabase function; confirm `ON CONFLICT DO NOTHING` |
| Clock-skew can cause LWW to pick wrong winner | Medium | Enforce NTP on all POS devices |
| Concurrent stock deductions not reconciled automatically | Low | Drift detector emits event; ledger is source of truth |
| No three-way merge for concurrent catalog edits | Low | Catalog edits from Back Office only; document in ops runbook |
| `global_sequence` gaps possible | Low | Monitor; migrate to timestamp-based pull in Phase 2 |
| Permanently-failed events have no alerting | Medium | Add a daily check: `SELECT COUNT(*) FROM sync_queue WHERE status='failed' AND attempt_count >= 10` |
| Manager PIN override tokens in memory only | Low | Acceptable for POS; token TTL is 60s. Restart clears all tokens.
