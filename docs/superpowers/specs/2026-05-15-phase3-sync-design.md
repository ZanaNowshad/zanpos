# Phase 3 — Central Sync Design
**Date:** 2026-05-15  
**Project:** ZANPOS  
**Status:** Approved

---

## 1. Goal

Implement bidirectional sync between local SQLite devices and a central Supabase PostgreSQL database. Devices push sales/shifts/refunds/audit events up; pull catalog changes (products, prices, users, categories) down. A Tokio background task runs every 30 seconds. The central schema is automatically applied on first store setup.

---

## 2. Architecture Overview

```
┌──────────────────────────────────────────────────┐
│  Windows Device (Tauri / SQLite)                 │
│                                                  │
│  POS ops → local SQLite (immediate, offline-ok)  │
│      ↓                                           │
│  sync_queue (outbox, populated after every write)│
│      ↓  push every 30s                           │
│  SyncWorker ─────────────────────────────────→   │
│                pull every 30s ←────────────────  │
│  apply inbox events → local SQLite (catalog)     │
└──────────────────────────────────────────────────┘
                      ↕ HTTPS / reqwest
┌──────────────────────────────────────────────────┐
│  Supabase (PostgreSQL)                           │
│                                                  │
│  sync_events  (global log, BIGSERIAL sequence)   │
│  products, categories, users, tax_rules  (LWW)   │
│  sales, payments, refunds, audit_logs  (append)  │
│                                                  │
│  apply_sync_event() RPC — LWW upserts            │
└──────────────────────────────────────────────────┘
```

**What gets pushed (Device → Central):**

| Entity | Operation |
|---|---|
| `sale` | create (immutable) |
| `sale_items` | append (immutable) |
| `payment` | append (immutable) |
| `shift` | create + update (open/close) |
| `refund` | create (immutable) |
| `refund_items` | append (immutable) |
| `audit_log` | append (immutable) |
| `product` | update (AI admin mutations only) |
| `product_price` | append (AI admin price changes) |

**What gets pulled (Central → Device):**

| Entity | Behavior |
|---|---|
| `product` | UPSERT if central `updated_at` is newer |
| `product_price` | INSERT OR IGNORE (append-only) |
| `category` | UPSERT if central `updated_at` is newer |
| `tax_rule` | UPSERT if central `updated_at` is newer |
| `user` | UPSERT if central `updated_at` is newer |

**Local-only (never synced):** `held_carts`, `ai_actions`, `undo_records`, `app_config`

**Conflict resolution:** Last-write-wins by `updated_at`. The central `apply_sync_event()` RPC applies `ON CONFLICT DO UPDATE SET ... WHERE central.updated_at < EXCLUDED.updated_at`. Append-only entities use `ON CONFLICT DO NOTHING`.

---

## 3. Central Supabase Schema

The full SQL is embedded as a Rust string constant in `src/sync/central_schema.rs` and applied automatically during store setup via the Supabase Management API.

### 3a. `sync_events` — canonical global event log

```sql
CREATE TABLE IF NOT EXISTS sync_events (
    global_sequence  BIGSERIAL PRIMARY KEY,
    device_id        TEXT NOT NULL,
    branch_id        TEXT NOT NULL,
    entity_type      TEXT NOT NULL,
    entity_id        TEXT NOT NULL,
    operation        TEXT NOT NULL,
    payload_json     JSONB NOT NULL,
    payload_hash     TEXT NOT NULL,
    idempotency_key  TEXT NOT NULL UNIQUE,
    local_sequence   BIGINT NOT NULL,
    created_at       TEXT NOT NULL,
    received_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_se_sequence ON sync_events (global_sequence);
CREATE INDEX IF NOT EXISTS idx_se_device   ON sync_events (device_id, local_sequence);
CREATE INDEX IF NOT EXISTS idx_se_entity   ON sync_events (entity_type, entity_id);
```

### 3b. Mirror entity tables

One table per synced entity, schemas matching the local SQLite definitions (same column names/types). Notable additions:
- Mutable tables (`products`, `categories`, `users`, `tax_rules`) include an `updated_at TEXT NOT NULL` column for LWW comparison.
- Append-only tables (`sales`, `sale_items`, `payments`, `shifts`, `refunds`, `refund_items`, `audit_logs`) have their primary key as the conflict target with `DO NOTHING`.

### 3c. `apply_sync_event()` RPC

Single PostgreSQL function called per push event. It:
1. INSERTs into `sync_events` with `ON CONFLICT (idempotency_key) DO NOTHING` (idempotent replay).
2. Routes to the correct entity table by `p_entity_type`.
3. For mutable entities: `ON CONFLICT (pk) DO UPDATE SET ... WHERE entity.updated_at < EXCLUDED.updated_at`.
4. For append-only entities: `ON CONFLICT (pk) DO NOTHING`.

Defined as `SECURITY DEFINER` so the anon/service role key can call it via REST RPC.

---

## 4. Store Setup & Auto-Migration

### 4a. Credentials

| Input | Purpose | Stored |
|---|---|---|
| Supabase Project URL (e.g. `https://xyz.supabase.co`) | REST API base URL for sync | ✅ `supabase_url` in `app_config` |
| Service Role Key | Bypasses RLS for all push/pull REST calls | ✅ `supabase_service_key` in `app_config` |
| Personal Access Token (PAT) | One-time Management API call to create central schema | ❌ Never stored, discarded after migration |

PAT is generated from: `https://supabase.com/dashboard/account/tokens`

### 4b. Auto-migration steps (Rust command: `admin_setup_supabase`)

1. Validate service role key: `GET {url}/rest/v1/` with `apikey: {service_role_key}` → expect 200.
2. Extract project ref from URL (`xyz` from `https://xyz.supabase.co`).
3. POST embedded SQL to Management API: `POST https://api.supabase.com/v1/projects/{ref}/database/query` with `Authorization: Bearer {PAT}`, body `{ "query": "<central_schema.sql>" }`.
4. On success: write `supabase_url` and `supabase_service_key` to `app_config`. PAT is dropped from memory.
5. On failure: return structured error to frontend (invalid URL, bad key, SQL error).

### 4c. Re-migration

Calling `admin_setup_supabase` again (e.g. changing Supabase project) re-runs the migration safely — all central DDL uses `CREATE TABLE IF NOT EXISTS` and `CREATE OR REPLACE FUNCTION`.

---

## 5. New Rust Modules

```
src-tauri/src/sync/
  mod.rs               — public module exports
  worker.rs            — SyncWorker struct, Tokio interval loop
  supabase_client.rs   — SupabaseClient: push_event(), pull_events(), validate(), migrate()
  outbox.rs            — enqueue_sale(), enqueue_shift(), enqueue_refund(),
                         enqueue_product(), enqueue_product_price(), enqueue_audit_log()
  inbox.rs             — apply_event() dispatch + per-entity UPSERT to local SQLite
  central_schema.rs    — pub const CENTRAL_SCHEMA_SQL: &str = "..."
```

### 5a. SyncWorker (`worker.rs`)

```rust
pub struct SyncWorker {
    pool: SqlitePool,
    // reads config fresh from app_config each tick
}

impl SyncWorker {
    pub fn spawn(pool: SqlitePool) -> Arc<Self>  // starts Tokio interval task
    pub async fn run_once(&self)                 // push_pending + pull_new; exposed for manual trigger
    async fn push_pending(&self) -> AppResult<u32>
    async fn pull_new(&self) -> AppResult<u32>
}
```

- Interval: 30 seconds.
- Reads `supabase_url` and `supabase_service_key` from `app_config` on each tick; skips silently if either is empty (not configured yet).
- Batch size: up to 50 `sync_queue` events per push cycle.
- Exponential backoff on repeated network errors: 30s → 60s → 120s → 300s cap. Resets to 30s on any success.
- Updates `sync_state.last_successful_sync_at` after each successful cycle.
- Writes `last_error` to the failed `sync_queue` row (not to `sync_state`) so the UI can surface the most recent error.

### 5b. SupabaseClient (`supabase_client.rs`)

```rust
pub struct SupabaseClient {
    base_url: String,
    service_key: String,
    http: reqwest::Client,
}

impl SupabaseClient {
    pub async fn validate(&self) -> AppResult<()>
    pub async fn migrate(&self, pat: &str, project_ref: &str) -> AppResult<()>
    pub async fn push_event(&self, event: &SyncQueueRow) -> AppResult<()>
    pub async fn pull_events(&self, since_seq: i64, exclude_device: &str)
        -> AppResult<Vec<SyncEventRow>>
}
```

- All REST calls include headers: `apikey: {service_key}`, `Authorization: Bearer {service_key}`, `Content-Type: application/json`.
- `push_event` calls `POST {url}/rest/v1/rpc/apply_sync_event`.
- `pull_events` calls `GET {url}/rest/v1/sync_events?global_sequence=gt.{since}&device_id=neq.{exclude}&order=global_sequence.asc&limit=100`.
- `migrate` calls `POST https://api.supabase.com/v1/projects/{ref}/database/query` with `Authorization: Bearer {pat}`.

### 5c. Outbox (`outbox.rs`)

Called at the end of every repository write that should sync. Each function:
1. Serialises the entity to `payload_json`.
2. Computes `payload_hash` (SHA-256 of payload_json).
3. Generates a new ULID for `sync_event_id`.
4. Gets `local_sequence` = `SELECT COALESCE(MAX(local_sequence), 0) + 1 FROM sync_queue WHERE device_id = ?`.
5. INSERTs into `sync_queue` with `status = 'pending'`.

```rust
pub async fn enqueue_sale(pool, sale, items, payments) -> AppResult<()>
pub async fn enqueue_shift(pool, shift) -> AppResult<()>
pub async fn enqueue_refund(pool, refund, items) -> AppResult<()>
pub async fn enqueue_product(pool, product) -> AppResult<()>
pub async fn enqueue_product_price(pool, price) -> AppResult<()>
pub async fn enqueue_audit_log(pool, log) -> AppResult<()>
```

### 5d. Inbox (`inbox.rs`)

```rust
pub async fn apply_event(pool: &SqlitePool, event: &SyncEventRow) -> AppResult<()>
```

Dispatches by `entity_type`:
- `product` → `INSERT OR REPLACE INTO products ... WHERE ? > (SELECT updated_at FROM products WHERE product_id = ?)` (only if remote is newer)
- `product_price` → `INSERT OR IGNORE INTO product_prices ...`
- `category` → `INSERT OR REPLACE` with `updated_at` guard
- `tax_rule` → `INSERT OR REPLACE` with `updated_at` guard
- `user` → `INSERT OR REPLACE` with `updated_at` guard
- `sale`, `payment`, `refund`, `shift`, `audit_log` from other devices → `INSERT OR IGNORE` (append-only, deduplicated by PK)

---

## 6. Repository Changes

Every repository write that produces sync data must call the corresponding `outbox::enqueue_*` function **within the same logical operation** (not a separate transaction to avoid partial writes):

| Repository function | Outbox call added |
|---|---|
| `sale_repo::create_sale()` | `enqueue_sale(sale, items, payments)` |
| `shift_repo::open_shift()` | `enqueue_shift(shift)` |
| `shift_repo::close_shift()` | `enqueue_shift(shift)` |
| `refund_repo::create_refund()` | `enqueue_refund(refund, items)` |
| `ai/tools.rs::execute_mutation()` — product update | `enqueue_product(product)` |
| `ai/tools.rs::execute_mutation()` — price update | `enqueue_product_price(price)` |
| audit_log writes (in `tools.rs`) | `enqueue_audit_log(log)` |

The `sync_status` column on `sales`, `shifts`, `refunds` (already in schema) is updated to `'synced'` by `SyncWorker::push_pending()` after successful push.

---

## 7. New & Updated Commands

### New Tauri commands

| Command | Purpose |
|---|---|
| `admin_setup_supabase(url, service_key, pat)` | Validate + migrate + store config |
| `admin_get_supabase_status()` → `{configured: bool}` | For setup wizard to check if already configured |

### Updated Tauri commands

| Command | Change |
|---|---|
| `sync_status` | `online` field now reflects real connectivity (last push/pull within 90s = online) |
| `sync_trigger_now` | Calls `SyncWorker::run_once()` instead of returning placeholder |

---

## 8. New Local Migration (`0004_phase3.sql`)

```sql
-- Add Supabase config keys
INSERT OR IGNORE INTO app_config (key, value) VALUES ('supabase_url', '');
INSERT OR IGNORE INTO app_config (key, value) VALUES ('supabase_service_key', '');

-- Add last_sync_online flag
INSERT OR IGNORE INTO app_config (key, value) VALUES ('sync_last_online_at', '');
```

No structural changes to existing tables — `sync_queue` and `sync_state` were already created in `0001_initial.sql`.

---

## 9. Frontend Changes

### AdminChatPage — Sync Setup step

New `SetupStep` value: `sync_setup` (inserted before `pick_provider`).

UI flow on first launch:
1. **Sync Setup card** — three inputs: Project URL, Service Role Key, PAT.
2. Helper text links pointing to Supabase dashboard locations.
3. "Connect & Migrate" → calls `adminSetupSupabase()` → shows spinner with message "Creating central tables…".
4. On success → step advances to `pick_provider`.
5. If user has already configured sync (checked via `adminGetSupabaseStatus()`) → skip directly to `pick_provider` or `done`.

Gear icon (already exists) → re-open settings. Settings modal now has two tabs: **Sync** (URL + service key, no PAT field) and **AI Provider** (existing flow).

### POS Topbar — Sync status badge

Already wired to `syncStatus` command. Changes:
- `online: true` = green dot (synced within 90s)
- `online: false` + `pending_events > 0` = yellow dot with count badge
- `online: false` + `last_error` = red dot
- Clicking badge opens a small popover: last sync time, pending count, last error message, "Sync Now" button

---

## 10. New TypeScript Types & Commands

```typescript
// types.ts additions
export interface SupabaseStatus {
  configured: boolean;
}

// commands.ts additions
export const adminSetupSupabase = (url: string, serviceKey: string, pat: string): Promise<void> =>
  invoke("admin_setup_supabase", { url, serviceKey, pat });

export const adminGetSupabaseStatus = (): Promise<SupabaseStatus> =>
  invoke("admin_get_supabase_status");
```

---

## 11. Error Handling & Resilience

| Scenario | Behaviour |
|---|---|
| Network unreachable | SyncWorker catches error, exponential backoff, `sync_queue.status` stays `'pending'` |
| Supabase returns 4xx | Marks event as `'failed'`, stores `last_error`, surfaces in sync badge |
| Idempotency conflict (event already synced) | Server returns 200 (ON CONFLICT DO NOTHING), client marks `'synced'` |
| Pull returns 0 events | No-op, watermark unchanged, cycle completes successfully |
| Migration failure during setup | Return structured error to frontend; nothing stored in `app_config` |
| Partial push batch | Events not yet confirmed stay `'pending'`; retried next cycle |

Retry cap: after `attempt_count >= 10`, event is marked `'failed'` and skipped (prevents infinite retry of a corrupt event). A future admin UI can surface and clear failed events.

---

## 12. Files Created / Modified Summary

### New files
```
src-tauri/src/sync/mod.rs
src-tauri/src/sync/worker.rs
src-tauri/src/sync/supabase_client.rs
src-tauri/src/sync/outbox.rs
src-tauri/src/sync/inbox.rs
src-tauri/src/sync/central_schema.rs
src-tauri/migrations/0004_phase3.sql
src/components/SyncStatusBadge.tsx       (optional extraction from inline JSX)
```

### Modified files
```
src-tauri/src/lib.rs                     — register new commands, spawn SyncWorker
src-tauri/src/commands/mod.rs            — add sync_admin_commands module
src-tauri/src/commands/sync_commands.rs  — implement sync_trigger_now, add admin_setup_supabase
src-tauri/src/db/repositories/sale_repo.rs    — add outbox::enqueue_sale call
src-tauri/src/db/repositories/shift_repo.rs   — add outbox::enqueue_shift call
src-tauri/src/db/repositories/refund_repo.rs  — add outbox::enqueue_refund call
src-tauri/src/ai/tools.rs               — add outbox calls on product/price mutations
src/types.ts                             — add SupabaseStatus
src/tauri/commands.ts                    — add adminSetupSupabase, adminGetSupabaseStatus
src/pages/AdminChatPage.tsx              — add sync_setup step, gear settings tabs
```

---

## 13. Out of Scope for Phase 3

- Multi-branch cross-device held cart sync (local-only by design)
- SQLCipher encryption of local SQLite (Phase 5)
- Conflict resolution UI for rejected events (Phase 5)
- Real-time push notifications from Supabase (polling is sufficient for Phase 3)
- Web admin dashboard (no third page — two-page constraint holds)
