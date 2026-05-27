# WhatsApp Contact Import — Design Spec

**Date:** 2026-05-27  
**Status:** Approved  
**Feature:** Automatically import all WhatsApp contacts into the POS customers table on QR scan, and via a manual button in Settings.

---

## 1. Problem Statement

When a cashier/owner connects WhatsApp for the first time (QR scan), their phone's entire contact book is available via Baileys. Today none of those contacts flow into the POS `customers` table — the cashier must manually add each customer one by one. The request: import all contacts automatically on connect, and expose a manual re-import button.

---

## 2. Design Decisions

| Question | Decision |
|---|---|
| Duplicates | `INSERT OR IGNORE` on `customers.phone` UNIQUE index |
| Notification | Toast banner showing "X contacts imported from WhatsApp" |
| Trigger | Both: every QR scan (auto on `onConnected`) AND manual "Import Contacts" button in Settings → WhatsApp |
| Architecture | Approach A: sidecar accumulates contacts; Rust calls `GET /contacts`, inserts into SQLite |

---

## 3. Data Flow

```
WhatsApp connects (QR scanned)
         │
         ▼ (1) contacts.upsert event fires in Baileys sidecar
Node sidecar: accumulates contacts in contactsMap{}
         │
         │ (2) onConnected callback fires in WhatsAppQRModal
         ▼
Rust command: whatsapp_import_contacts
         │ (3) GET http://127.0.0.1:3131/contacts
         │ (4) batch INSERT OR IGNORE into customers
         │ (5) return { imported, skipped, total }
         ▼
Frontend: toast "247 contacts imported from WhatsApp"
```

Manual path: Settings → WhatsApp → "Import Contacts" button → same Rust command.

---

## 4. Data Mapping

| WhatsApp (Baileys `contacts.upsert`) | `customers` column | Notes |
|---|---|---|
| `c.notify` \|\| `c.name` \|\| stripped phone | `name` | Use first non-empty field |
| `c.id` (e.g. `97333050666@s.whatsapp.net`) | `phone` | Strip `@s.whatsapp.net`, prefix `+` |
| `Ulid::new()` | `customer_id` | New ULID per row |
| active branch `branch_id` | `branch_id` | From `branches WHERE is_active=1` |
| `0` | `loyalty_points` | Default |
| `utc_now` | `created_at` | RFC-3339 |

Deduplication: `INSERT OR IGNORE` on `UNIQUE INDEX idx_customers_phone_unique ON customers(phone) WHERE phone IS NOT NULL`.

---

## 5. Components

### 5a. Node Sidecar (`server.js`)

- New state: `let contactsMap = {}` — keyed by JID, value `{ id, name }`.
- In `startBaileys()`, add `sock.ev.on("contacts.upsert", contacts => ...)` listener that merges contacts into `contactsMap`.
- New endpoint: `GET /contacts` → returns `Object.values(contactsMap)` as JSON array.
- Contacts accumulate over sidecar lifetime; map is never cleared (reconnect adds more contacts).

### 5b. Migration `0023_customers_phone_unique.sql`

```sql
-- Remove duplicate phones (keep oldest row)
DELETE FROM customers
WHERE customer_id NOT IN (
    SELECT MIN(customer_id) FROM customers
    WHERE phone IS NOT NULL GROUP BY phone
) AND phone IS NOT NULL;

-- Partial unique index (nulls are excluded — multiple null-phone customers allowed)
CREATE UNIQUE INDEX IF NOT EXISTS idx_customers_phone_unique
    ON customers(phone) WHERE phone IS NOT NULL;
```

### 5c. Rust (`whatsapp_commands.rs`)

New public types:
- `SidecarContact { id: String, name: String }` — deserialized from sidecar
- `ImportContactsResult { imported: usize, skipped: usize, total: usize }` — returned to frontend

New command: `whatsapp_import_contacts(actor_user_id, state)`:
1. RBAC: `manager_or_owner`
2. `GET /contacts` from sidecar, deserialize
3. Fetch active `branch_id`
4. Loop: strip JID suffix, prefix `+`, `INSERT OR IGNORE`, count rows_affected
5. Return `ImportContactsResult`

### 5d. `lib.rs`

Register `whatsapp_import_contacts` in `.invoke_handler(tauri::generate_handler![...])`.

### 5e. `src/tauri/commands.ts`

```ts
export function whatsappImportContacts(actorUserId: string): Promise<ImportContactsResult> {
  return invoke("whatsapp_import_contacts", { actorUserId });
}
```

New TS type:
```ts
export interface ImportContactsResult {
  imported: number;
  skipped: number;
  total: number;
}
```

### 5f. `src/components/SettingsTab.tsx` — `WhatsAppSettingsSection`

- Add `importing` boolean state and `importMsg` string state.
- Add `handleImportContacts()` async function: calls `whatsappImportContacts`, sets `importMsg`.
- Add "📥 Import Contacts" button below the connect/disconnect row, visible when `isManager`.
- Modify `onConnected` prop passed to `WhatsAppQRModal` to also call `handleImportContacts()` after `refresh()`.

---

## 6. Error Handling

| Scenario | Behavior |
|---|---|
| Sidecar not running | Rust returns `AppError::Internal("Sidecar unreachable")`; frontend shows error toast |
| `/contacts` returns empty array | `ImportContactsResult { imported: 0, skipped: 0, total: 0 }`; toast shows "0 contacts imported" |
| All contacts already exist | `imported: 0, skipped: N, total: N`; toast shows "0 new contacts (N already existed)" |
| DB error on INSERT | Rust returns `AppError`; frontend shows error toast |
| Called when not manager/owner | Rust returns `AppError::Unauthorized` |

---

## 7. Constraints

- **No batch transaction**: Individual inserts — if one fails, others proceed. Acceptable for a best-effort import.
- **No sync**: Imported contacts are local-only (no sync_queue entry needed — customers are branch-scoped).
- **No whatsapp_status check in Rust**: Rust blindly calls `/contacts`; if sidecar returns empty, user gets "0 contacts" toast and can retry manually.
- **Timing**: After QR scan, Baileys fires `contacts.upsert` asynchronously. A 1–2s delay before all contacts arrive is expected. The auto-trigger fires immediately on connect; a manual re-import 5s later will capture any stragglers.
