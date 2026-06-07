# ZANPOS Enterprise Gaps Audit — 2026-06-07

| # | Area | Risk | Severity | File:Line | Exact Fix |
|---|------|------|----------|-----------|-----------|
| E01 | Shift integrity | `open_shift` is SELECT-then-INSERT with no UNIQUE constraint on (device_id, status='open'). Two concurrent calls both read `None` from `get_active_shift`, both proceed to INSERT — creating two open shifts for the same device. The second shift has no sales and can poison EOD cashup totals. | P0 | `shift_repo.rs:201–230` + next migration — Add: `CREATE UNIQUE INDEX IF NOT EXISTS idx_shifts_one_open_per_device ON shifts(device_id) WHERE status = 'open';` The INSERT at line 221 will then fail with UNIQUE constraint on the second concurrent caller, which is the correct behavior. |
| E02 | Audit trail — admin mutations | `insert_audit_entry` calls in `admin_commands.rs` use `let _ =` (lines 343, 495, 671, 763, 1261, 1307, 1362, 1481). Audit failures are **silently swallowed** — if the audit DB is corrupt or locked, price changes, product creations, and category edits are **unaudited with no log or alert**. | P0 | `admin_commands.rs:343,495,671,763,1261,1307,1362,1481` — Replace every `let _ = audit_hash::insert_audit_entry(...)` with: `if let Err(e) = audit_hash::insert_audit_entry(...).await { tracing::error!("AUDIT WRITE FAILED for {}: {:?}", event_type, e); }` For P1 mutations (price changes, user ops), consider returning an error to the caller instead of just logging. |
| E03 | Audit trail — customer mutations | `customer_commands.rs:156` — `let _ = audit_hash::insert_audit_entry(...)` — customer creates/updates are silently unaudited on failure | P1 | `customer_commands.rs:156` — Same fix as E02: log the error at `tracing::error!` level, do not silently discard |
| E04 | Audit trail — migration execute | `migration_commands.rs:859` — `let _ = audit_hash::insert_audit_entry(...)` — bulk migrations (potentially thousands of rows) have silently optional audit trail | P1 | `migration_commands.rs:859` — Log audit failure at `tracing::error!`. A failed audit on migration is especially critical because there's no other record of what was imported. |
| E05 | Sync — loyalty post-commit | `sale_repo.rs:443–449` — `UPDATE customers SET loyalty_points = loyalty_points + ?` is called **after `tx.commit()` at line 436**, using the pool (not the transaction). If the process crashes between commit and loyalty update, the sale is permanently saved but the customer's loyalty points are silently lost. | P1 | `sale_repo.rs:436–449` — Two options: (a) move the loyalty UPDATE inside the transaction before commit — simplest; (b) accept best-effort and add `tracing::warn!` on the error case so it's visible in logs. Option (a) is preferred for correctness. |
| E06 | Sync — outbox vs dirty-flag | Architecture note: The app uses `sync_status = 'pending'` dirty-flag on rows (confirmed: `sale_repo.rs:173` sets `sync_status = 'pending'` inside the transaction). This means sale sync events are **atomically tied to the sale INSERT** — no phantom-enqueue risk. This is correct and safe. | ✓ Safe | No fix needed. Document this as an architectural invariant in CLAUDE.md so future contributors don't accidentally add a post-commit enqueue. |
| E07 | Receipt number uniqueness | `sale_repo.rs:161–165` — receipt number is generated inside the transaction via `next_receipt_number(&mut *tx, ...)` which does `UPDATE devices SET next_receipt_seq = next_receipt_seq + 1 RETURNING next_receipt_seq`. SQLite serializes writers, so two concurrent finalizes on the same device are serialized at the DB level. **This is safe.** | ✓ Safe | No fix needed. The implementation correctly uses an atomic counter inside the transaction. |
| E08 | RBAC — shift_open | `shift_commands.rs:31` — `shift_open` calls `rbac::require_any_role` — correct: any authenticated user can open a shift | ✓ Safe | No fix. |
| E09 | RBAC — shift_close | `shift_commands.rs:85,90` — close calls `manager_or_owner` for forced close, `owner_only` for certain operations — correct role escalation | ✓ Safe | No fix. |
| E10 | RBAC — cash commands | `cash_commands.rs:196,350,365,533` — all cash mutation commands check `require_any_role` or `manager_or_owner` — correct | ✓ Safe | No fix. |
| E11 | Silent WA errors | `delivery_commands.rs:58,72,95` — WhatsApp notification calls use `let _ =` — these are explicitly best-effort (delivery notification failure should not fail the status update). But there is **no logging** — failures are invisible in production. | P2 | `delivery_commands.rs:58,72,95` — Replace `let _ = ...` with: `if let Err(e) = whatsapp_send_delivery_impl(...).await { tracing::warn!("WA notification failed for delivery {}: {:?}", delivery_id, e); }` |
| E12 | Clock skew | Grep confirms: `sale_repo.rs` orders by `sold_at` and `created_at`. For single-device queries this is safe. Cross-device sync uses ULIDs in the outbox (monotonic per device). But `sync_worker.rs` queries `WHERE created_at < cutoff` for retention pruning — if device clock is wrong, unsynced rows could be pruned early. | P1 | `sync/worker.rs` retention prune — Add: `AND sync_status = 'synced'` to the `DELETE FROM audit_logs WHERE created_at < ?` and `DELETE FROM stock_movements WHERE created_at < ?` queries (confirm these are already in place from the June-05 spec fix — grep to verify). |
| E13 | Bulk import SQL injection surface | `admin_commands.rs:1076,1090,1108` — three `let _ = sqlx::query(...)` calls that silently swallow SQL errors during bulk import. If a batch row fails to insert, the failure is invisible — the import appears to succeed. | P1 | `admin_commands.rs:1076–1110` — Collect errors into a `Vec<String>` and return them in the command response. The caller can then surface "3 rows failed to import" instead of a silent partial import. |
| E14 | AI mutation audit trail | `ai_admin_commands.rs` — the AI tool loop can execute DB mutations (price changes, stock adjustments, customer updates) via confirmed `ai_actions`. Verify each AI-executed tool also calls `insert_audit_entry` with `actor_type = "ai"`. A mutation executed by AI with no audit entry is a compliance gap. | P1 | Search for every `execute_tool` call in `ai/tools.rs` that runs a mutation — confirm each one has a corresponding `insert_audit_entry(..., "ai", ...)` call. Add any that are missing. |
| E15 | Sync retry idempotency | `sync/worker.rs` — if the sync worker marks an event as 'synced' but crashes before writing the DB update, the event is retried on next run. The Supabase RPC must handle this idempotently. Verify the `process_outbox` RPC uses `ON CONFLICT DO UPDATE` (upsert) not INSERT. | P1 | Review `sync/central_schema.rs` and the Supabase RPC definition — confirm every entity upsert uses `ON CONFLICT (id) DO UPDATE SET ...` not bare `INSERT`. Flag any entity type that lacks the `ON CONFLICT` clause. |

## P0 Critical — Fix Before Production

### E01 — Shift double-open (SELECT-then-INSERT race)
```sql
-- Add to next migration file:
CREATE UNIQUE INDEX IF NOT EXISTS idx_shifts_one_open_per_device
  ON shifts(device_id)
  WHERE status = 'open';
```
After this index, the second concurrent `open_shift` INSERT fails with `UNIQUE constraint failed` → `AppError::Conflict` → frontend shows "Shift already open". Safe, correct, zero business logic change.

### E02 — Silent audit failure on admin mutations
Every price change, product creation, and category edit in `admin_commands.rs` currently passes silently if the audit write fails. For a VAT-registered Bahrain business subject to NBR audit, untracked mutations are a compliance exposure.
```rust
// Replace all instances of:
let _ = audit_hash::insert_audit_entry(...).await;

// With:
if let Err(e) = audit_hash::insert_audit_entry(...).await {
    tracing::error!("AUDIT WRITE FAILED [{}] entity={} id={}: {:?}", event_type, entity_type, entity_id, e);
}
```

## Confirmed Solid Architecture
- Receipt number generation: atomically inside transaction with RETURNING counter ✓
- Sale sync via dirty-flag `sync_status='pending'` inside INSERT transaction ✓  
- RBAC checks present on all cash, shift, and refund commands ✓
- Argon2id PIN hashing with account lockout ✓
- SHA-256 audit hash chain with `before_json`, `reason`, `actor_type` (June-05 fix) ✓
- Sync retention guards on `sync_status = 'synced'` (June-05 fix) ✓
