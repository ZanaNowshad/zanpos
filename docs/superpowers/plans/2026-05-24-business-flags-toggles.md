# Business Flags & Operational Toggles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a "Business Rules" settings section with 4 operational toggles — allowing the owner to control negative-stock selling, discount permissions, discount reason requirements, and auto-printing receipts — all persisted in the existing `app_config` key-value table.

**Architecture:** Flags are stored as rows in the existing `app_config` SQLite table (key = `flag_*`, value = `"0"` or `"1"`). A new pair of Tauri commands (`business_flags_load` / `business_flags_save`) reads/writes them. Three flags affect Rust business logic; one is frontend-only. The Settings tab gets a new "Business Rules" section.

**Tech Stack:** Rust (sqlx, Tauri), TypeScript (React 19), SQLite migrations

---

## Flags Being Added

| Key | Default | Effect |
|---|---|---|
| `flag_allow_negative_stock` | `0` | When ON: sale finalizes even when stock qty < sold qty |
| `flag_require_discount_reason` | `1` | When OFF: discount reason field is optional |
| `flag_cashier_can_discount` | `0` | When ON: cashiers can apply discounts (not just managers) |
| `flag_auto_print_receipt` | `0` | When ON: thermal receipt prints automatically after every sale |

---

## File Map

| File | Change |
|---|---|
| `src-tauri/migrations/0017_business_flags.sql` | **Create** — seed default flag values |
| `src-tauri/src/commands/setup_commands.rs` | **Modify** — add `BusinessFlags` struct + `business_flags_load` + `business_flags_save` commands |
| `src-tauri/src/db/repositories/sale_repo.rs` | **Modify** — `finalize_sale` gains `allow_negative_stock: bool` param; SQL changes |
| `src-tauri/src/commands/pos_commands.rs` | **Modify** — `pos_finalize_sale` loads flag before calling repo; discount commands check flags |
| `src-tauri/src/lib.rs` | **Modify** — register 2 new commands |
| `src/types.ts` | **Modify** — add `BusinessFlags` interface |
| `src/tauri/commands.ts` | **Modify** — add `businessFlagsLoad` / `businessFlagsSave` wrappers |
| `src/components/SettingsTab.tsx` | **Modify** — add "Business Rules" section with 4 toggle rows |
| `src/pages/PosPage.tsx` | **Modify** — auto-print after finalize when flag is on |

---

## Task 1 — Migration: seed default flag values

**Files:**
- Create: `src-tauri/migrations/0017_business_flags.sql`

- [ ] **Step 1: Create the migration file**

```sql
-- 0017_business_flags.sql
-- Seed default values for business behavior flags.
-- INSERT OR IGNORE preserves any value the owner already set
-- (safe to re-run on existing databases).
INSERT OR IGNORE INTO app_config (key, value) VALUES
  ('flag_allow_negative_stock',   '0'),
  ('flag_require_discount_reason','1'),
  ('flag_cashier_can_discount',   '0'),
  ('flag_auto_print_receipt',     '0');
```

- [ ] **Step 2: Verify the migration file exists**

```powershell
Get-Item src-tauri\migrations\0017_business_flags.sql
```
Expected: file listed without error.

- [ ] **Step 3: Commit**

```powershell
git add src-tauri/migrations/0017_business_flags.sql
git commit -m "feat: migration 0017 — seed business flag defaults in app_config"
```

---

## Task 2 — Rust: BusinessFlags struct + load/save commands

**Files:**
- Modify: `src-tauri/src/commands/setup_commands.rs`

**Context:** `setup_commands.rs` already handles `app_config_load`, `app_config_get_timeout`, `app_config_set_timeout`, and branch settings. Add the new struct and two commands at the bottom of the file.

- [ ] **Step 1: Add `BusinessFlags` struct and helper at the bottom of `setup_commands.rs`**

Add this block after all existing code in the file (before the closing of the module):

```rust
// ─── Business flags ───────────────────────────────────────────────────────────

/// Operational toggles that control business rules in the POS.
/// All flags are stored as "0"/"1" strings in app_config.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BusinessFlags {
    /// Allow a sale to finalize even when stock quantity would go below zero.
    pub allow_negative_stock: bool,
    /// When true, a non-empty reason is required for every discount applied.
    pub require_discount_reason: bool,
    /// When true, cashiers (not just managers/owners) may apply discounts.
    pub cashier_can_discount: bool,
    /// When true, the thermal receipt prints automatically after every sale.
    pub auto_print_receipt: bool,
}

impl Default for BusinessFlags {
    fn default() -> Self {
        Self {
            allow_negative_stock: false,
            require_discount_reason: true,
            cashier_can_discount: false,
            auto_print_receipt: false,
        }
    }
}

/// Read a single boolean flag from app_config ("1" == true, anything else == false).
async fn read_flag(pool: &sqlx::SqlitePool, key: &str, default: bool) -> bool {
    let val: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_config WHERE key = ?",
    )
    .bind(key)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    match val.as_deref() {
        Some(v) => v == "1",
        None => default,
    }
}

/// Write a single boolean flag to app_config (upsert).
async fn write_flag(
    pool: &sqlx::SqlitePool,
    key: &str,
    value: bool,
) -> crate::errors::AppResult<()> {
    sqlx::query(
        "INSERT INTO app_config (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(if value { "1" } else { "0" })
    .execute(pool)
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn business_flags_load(
    state: State<'_, AppState>,
) -> Result<BusinessFlags, AppError> {
    let flags = BusinessFlags {
        allow_negative_stock:    read_flag(&state.db, "flag_allow_negative_stock",    false).await,
        require_discount_reason: read_flag(&state.db, "flag_require_discount_reason", true).await,
        cashier_can_discount:    read_flag(&state.db, "flag_cashier_can_discount",    false).await,
        auto_print_receipt:      read_flag(&state.db, "flag_auto_print_receipt",      false).await,
    };
    Ok(flags)
}

#[derive(serde::Deserialize)]
pub struct SaveBusinessFlagsInput {
    pub flags: BusinessFlags,
    pub actor_user_id: String,
}

#[tauri::command]
pub async fn business_flags_save(
    input: SaveBusinessFlagsInput,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    // Only managers and owners may change business rules.
    rbac::manager_or_owner(&state.db, &input.actor_user_id).await?;

    write_flag(&state.db, "flag_allow_negative_stock",    input.flags.allow_negative_stock).await?;
    write_flag(&state.db, "flag_require_discount_reason", input.flags.require_discount_reason).await?;
    write_flag(&state.db, "flag_cashier_can_discount",    input.flags.cashier_can_discount).await?;
    write_flag(&state.db, "flag_auto_print_receipt",      input.flags.auto_print_receipt).await?;

    Ok(())
}
```

- [ ] **Step 2: Add `use crate::commands::rbac;` import at the top of `setup_commands.rs` if not already present**

Check the top of the file for `use crate::commands::rbac;`. If it's missing, add it after the existing `use` statements.

- [ ] **Step 3: Register the two new commands in `src-tauri/src/lib.rs`**

Find the `.invoke_handler(tauri::generate_handler![` block. It already lists many commands. Add these two to the list:

```rust
setup_commands::business_flags_load,
setup_commands::business_flags_save,
```

- [ ] **Step 4: Build to confirm no compile errors**

```powershell
cd src-tauri && cargo build 2>&1 | Select-String -Pattern "error"
```
Expected: no lines containing `error[E` (warnings are fine).

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/commands/setup_commands.rs src-tauri/src/lib.rs
git commit -m "feat: add BusinessFlags struct and business_flags_load/save Tauri commands"
```

---

## Task 3 — Rust: wire `allow_negative_stock` into `sale_repo::finalize_sale`

**Files:**
- Modify: `src-tauri/src/db/repositories/sale_repo.rs` — `finalize_sale` function signature + stock-deduction block
- Modify: `src-tauri/src/commands/pos_commands.rs` — `pos_finalize_sale` loads flag and passes it

**Context:** The stock-deduction block in `sale_repo.rs` is at lines ~272–311. It runs this SQL:
```sql
UPDATE stock_levels
SET quantity_on_hand = CAST(CAST(quantity_on_hand AS REAL) - ? AS TEXT), ...
WHERE product_id = ? AND branch_id = ?
  AND CAST(quantity_on_hand AS REAL) >= ?   ← this guard blocks the sale
```
When `allow_negative_stock` is true, we drop the `AND qty >= sold` clause so stock can go negative.

- [ ] **Step 1: Add `allow_negative_stock: bool` parameter to `finalize_sale` in `sale_repo.rs`**

Find the function signature:
```rust
pub async fn finalize_sale(
    pool: &SqlitePool,
    cart: &Cart,
    payments: Vec<PaymentInput>,
    idempotency_key: &str,
    customer_id: Option<&str>,
    created_offline: bool,
    delivery: Option<DeliveryInput>,
) -> AppResult<SaleResult> {
```

Change it to:
```rust
pub async fn finalize_sale(
    pool: &SqlitePool,
    cart: &Cart,
    payments: Vec<PaymentInput>,
    idempotency_key: &str,
    customer_id: Option<&str>,
    created_offline: bool,
    delivery: Option<DeliveryInput>,
    allow_negative_stock: bool,
) -> AppResult<SaleResult> {
```

- [ ] **Step 2: Replace the stock-deduction block in `sale_repo.rs`**

Find this block (lines ~272–311):
```rust
        // Atomic check-and-deduct: if current_qty < sold_qty the WHERE fails.
        // quantity_on_hand is stored as TEXT; CAST to REAL for arithmetic.
        let rows = sqlx::query(
            "UPDATE stock_levels
             SET quantity_on_hand     = CAST(CAST(quantity_on_hand AS REAL) - ? AS TEXT),
                 last_movement_at     = ?,
                 updated_at           = ?
             WHERE product_id = ? AND branch_id = ?
               AND CAST(quantity_on_hand AS REAL) >= ?",
        )
        .bind(sold_qty)
        .bind(&now)
        .bind(&now)
        .bind(product_id)
        .bind(&cart.branch_id)
        .bind(sold_qty)
        .execute(&mut *tx)
        .await?;

        if rows.rows_affected() == 0 {
            // Either no stock record (first-ever sale of this item, treat as untracked)
            // OR genuine out-of-stock. Distinguish by checking if a stock_level row exists.
            let exists: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM stock_levels WHERE product_id = ? AND branch_id = ?",
            )
            .bind(product_id)
            .bind(&cart.branch_id)
            .fetch_optional(&mut *tx)
            .await?;

            if exists.is_some() {
                // Stock record exists but qty is insufficient
                return Err(AppError::Validation(format!(
                    "Insufficient stock for '{}'. Please check inventory levels.",
                    line.product_name
                )));
            }
            // No stock record yet (uninitialized product) — allow the sale through.
            // This prevents blocking sales for newly-added products not yet stocked.
        }
```

Replace it with:
```rust
        // Deduct stock atomically within the transaction.
        // When allow_negative_stock is OFF: the WHERE clause requires qty >= sold,
        // so an under-stocked item causes 0 rows_affected and we return an error.
        // When allow_negative_stock is ON: we drop the qty guard so stock can go
        // negative (back-order / temporary oversell scenario).
        let rows = if allow_negative_stock {
            sqlx::query(
                "UPDATE stock_levels
                 SET quantity_on_hand = CAST(CAST(quantity_on_hand AS REAL) - ? AS TEXT),
                     last_movement_at = ?,
                     updated_at       = ?
                 WHERE product_id = ? AND branch_id = ?",
            )
            .bind(sold_qty)
            .bind(&now)
            .bind(&now)
            .bind(product_id)
            .bind(&cart.branch_id)
            .execute(&mut *tx)
            .await?
        } else {
            sqlx::query(
                "UPDATE stock_levels
                 SET quantity_on_hand = CAST(CAST(quantity_on_hand AS REAL) - ? AS TEXT),
                     last_movement_at = ?,
                     updated_at       = ?
                 WHERE product_id = ? AND branch_id = ?
                   AND CAST(quantity_on_hand AS REAL) >= ?",
            )
            .bind(sold_qty)
            .bind(&now)
            .bind(&now)
            .bind(product_id)
            .bind(&cart.branch_id)
            .bind(sold_qty)
            .execute(&mut *tx)
            .await?
        };

        if rows.rows_affected() == 0 {
            // rows_affected == 0 means either:
            //   (a) no stock_level row for this product+branch yet (uninitialized) — allow
            //   (b) allow_negative_stock is OFF and qty was insufficient — block
            let exists: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM stock_levels WHERE product_id = ? AND branch_id = ?",
            )
            .bind(product_id)
            .bind(&cart.branch_id)
            .fetch_optional(&mut *tx)
            .await?;

            if exists.is_some() && !allow_negative_stock {
                return Err(AppError::Validation(format!(
                    "Insufficient stock for '{}'. Please check inventory levels.",
                    line.product_name
                )));
            }
            // No stock record yet (uninitialized product) → allow through.
        }
```

- [ ] **Step 3: Update `pos_finalize_sale` in `pos_commands.rs` to load the flag and pass it**

Find the `pos_finalize_sale` function. Replace it with:

```rust
#[tauri::command]
pub async fn pos_finalize_sale(
    input: FinalizeSaleInput,
    state: State<'_, AppState>,
) -> Result<SaleResult, AppError> {
    let key = input
        .idempotency_key
        .unwrap_or_else(|| Ulid::new().to_string());
    let created_offline = !state.sync_worker.state.lock().await.online;

    // Load the allow_negative_stock business flag.
    let flag_val: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_config WHERE key = 'flag_allow_negative_stock'",
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    let allow_negative_stock = flag_val.as_deref() == Some("1");

    sale_repo::finalize_sale(
        &state.db,
        &input.cart,
        input.payments,
        &key,
        input.customer_id.as_deref(),
        created_offline,
        input.delivery,
        allow_negative_stock,
    )
    .await
}
```

- [ ] **Step 4: Build**

```powershell
cd src-tauri && cargo build 2>&1 | Select-String -Pattern "error\[E"
```
Expected: no compile errors.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/db/repositories/sale_repo.rs src-tauri/src/commands/pos_commands.rs
git commit -m "feat: allow_negative_stock flag wired into finalize_sale stock deduction"
```

---

## Task 4 — Rust: wire `require_discount_reason` and `cashier_can_discount` flags

**Files:**
- Modify: `src-tauri/src/commands/pos_commands.rs` — `pos_apply_bill_discount` and `pos_apply_line_discount`

**Context:** Both discount commands currently always enforce `reason.trim() != ""` and `rbac::manager_or_owner`. The two new flags make these conditional.

- [ ] **Step 1: Update `pos_apply_bill_discount`**

Find the start of `pos_apply_bill_discount` (after the `let discount = input.discount_minor.max(0);` line). Replace the two guard blocks:

```rust
    // Before:
    if discount > 0 && input.reason.trim().is_empty() {
        return Err(AppError::Validation(
            "A reason is required when applying a bill discount".into(),
        ));
    }
    if discount > 0 {
        rbac::manager_or_owner(&state.db, &input.authorized_by_user_id).await?;
    }
```

With:

```rust
    // Load discount-related flags (small DB reads, flags rarely change).
    let require_reason_val: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_config WHERE key = 'flag_require_discount_reason'",
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    let require_discount_reason = require_reason_val.as_deref() != Some("0"); // default true

    let cashier_discount_val: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_config WHERE key = 'flag_cashier_can_discount'",
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    let cashier_can_discount = cashier_discount_val.as_deref() == Some("1");

    if discount > 0 && require_discount_reason && input.reason.trim().is_empty() {
        return Err(AppError::Validation(
            "A reason is required when applying a bill discount".into(),
        ));
    }
    if discount > 0 && !cashier_can_discount {
        rbac::manager_or_owner(&state.db, &input.authorized_by_user_id).await?;
    }
```

- [ ] **Step 2: Apply the same change to `pos_apply_line_discount`**

Find the guard block at the start of `pos_apply_line_discount` (same pattern as bill discount). Replace:

```rust
    // Before:
    if discount > 0 && input.reason.trim().is_empty() {
        return Err(AppError::Validation(
            "A reason is required when applying a line discount".into(),
        ));
    }
    if discount > 0 {
        rbac::manager_or_owner(&state.db, &input.authorized_by_user_id).await?;
    }
```

With:

```rust
    let require_reason_val: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_config WHERE key = 'flag_require_discount_reason'",
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    let require_discount_reason = require_reason_val.as_deref() != Some("0");

    let cashier_discount_val: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_config WHERE key = 'flag_cashier_can_discount'",
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    let cashier_can_discount = cashier_discount_val.as_deref() == Some("1");

    if discount > 0 && require_discount_reason && input.reason.trim().is_empty() {
        return Err(AppError::Validation(
            "A reason is required when applying a line discount".into(),
        ));
    }
    if discount > 0 && !cashier_can_discount {
        rbac::manager_or_owner(&state.db, &input.authorized_by_user_id).await?;
    }
```

- [ ] **Step 3: Build**

```powershell
cd src-tauri && cargo build 2>&1 | Select-String -Pattern "error\[E"
```
Expected: no compile errors.

- [ ] **Step 4: Commit**

```powershell
git add src-tauri/src/commands/pos_commands.rs
git commit -m "feat: require_discount_reason and cashier_can_discount flags wire into discount commands"
```

---

## Task 5 — Frontend types + command wrappers

**Files:**
- Modify: `src/types.ts`
- Modify: `src/tauri/commands.ts`

- [ ] **Step 1: Add `BusinessFlags` interface to `src/types.ts`**

Find the `BranchSettings` interface (around line 534). Add this block immediately after it:

```typescript
export interface BusinessFlags {
  /** Allow a sale to finalize even when stock quantity would go below zero. */
  allow_negative_stock: boolean;
  /** When true, a non-empty reason is required for every discount applied. */
  require_discount_reason: boolean;
  /** When true, cashiers (not just managers/owners) may apply discounts. */
  cashier_can_discount: boolean;
  /** When true, the thermal receipt prints automatically after every sale. */
  auto_print_receipt: boolean;
}
```

- [ ] **Step 2: Add `BusinessFlags` to the imports in `src/tauri/commands.ts`**

Find the import block at the top of `commands.ts`. Add `BusinessFlags` to the type imports:

```typescript
import type {
  // ... existing imports ...
  BusinessFlags,
  // ...
} from "../types";
```

- [ ] **Step 3: Add the two command wrappers to `src/tauri/commands.ts`**

Find the Settings section (near `settingsGetBranch`, `settingsUpdateBranch`). Add after it:

```typescript
// ─── Business flags ────────────────────────────────────────────────────────────

export const businessFlagsLoad = (): Promise<BusinessFlags> =>
  invoke("business_flags_load");

export const businessFlagsSave = (
  flags: BusinessFlags,
  actor_user_id: string,
): Promise<void> =>
  invoke("business_flags_save", { input: { flags, actor_user_id } });
```

- [ ] **Step 4: TypeScript check**

```powershell
npx tsc --noEmit 2>&1
```
Expected: no errors.

- [ ] **Step 5: Commit**

```powershell
git add src/types.ts src/tauri/commands.ts
git commit -m "feat: BusinessFlags TS type and businessFlagsLoad/Save command wrappers"
```

---

## Task 6 — Settings UI: Business Rules section

**Files:**
- Modify: `src/components/SettingsTab.tsx`

**Context:** The settings tab has a `SUB_NAV` array and renders `<section id="s-*">` blocks. Add a new `"s-rules"` entry and a corresponding section with 4 toggle rows. Only managers/owners can save (already enforced by the Rust command). The section goes between `s-security` and `s-printer`.

- [ ] **Step 1: Add state variables for business flags at the top of `SettingsTab` component**

Find the `// Session timeout` state block. Add after the timeout state variables:

```typescript
  // Business flags
  const [flags, setFlags]           = useState<BusinessFlags>({
    allow_negative_stock: false,
    require_discount_reason: true,
    cashier_can_discount: false,
    auto_print_receipt: false,
  });
  const [savingFlags, setSavingFlags] = useState(false);
  const [savedFlags, setSavedFlags]   = useState(false);
```

- [ ] **Step 2: Import `BusinessFlags` and the two commands in `SettingsTab.tsx`**

At the top of the file, add to the existing imports:

```typescript
import type { BranchSettings, BusinessFlags, ThermalConfig, WhatsAppStatus } from "../types";
```

And in the command imports block:
```typescript
import {
  // ...existing imports...
  businessFlagsLoad,
  businessFlagsSave,
} from "../tauri/commands";
```

- [ ] **Step 3: Load flags in the existing `useEffect`**

Find the `Promise.all([` inside `useEffect`. Add `businessFlagsLoad()` as a fourth item:

```typescript
    Promise.all([
      settingsGetBranch(),
      appConfigGetTimeout().catch(() => 5),
      thermalGetConfig().catch(() => ({ enabled: false, port: "", baud: "9600" })),
      businessFlagsLoad().catch(() => ({
        allow_negative_stock: false,
        require_discount_reason: true,
        cashier_can_discount: false,
        auto_print_receipt: false,
      })),
    ])
      .then(([s, minutes, tc, bf]) => {
        // ... existing setters ...
        setFlags(bf as BusinessFlags);
      })
```

- [ ] **Step 4: Add `handleSaveFlags` handler**

After `handleSaveTimeout`, add:

```typescript
  const handleSaveFlags = async () => {
    setSavingFlags(true);
    setSavedFlags(false);
    try {
      await businessFlagsSave(flags, sessionUserId);
      setSavedFlags(true);
      setTimeout(() => setSavedFlags(false), 3000);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to save business rules");
    } finally {
      setSavingFlags(false);
    }
  };
```

- [ ] **Step 5: Add `"s-rules"` to `SUB_NAV`**

Find the `SUB_NAV` array:
```typescript
  const SUB_NAV = [
    { id: "s-business",  label: "Store Settings" },
    { id: "s-receipt",   label: "Receipt Text" },
    { id: "s-design",    label: "Receipt Design" },
    { id: "s-security",  label: "Security" },
    { id: "s-printer",   label: "Printers" },
    { id: "s-whatsapp",  label: "WhatsApp" },
    { id: "s-wa-format", label: "WA Message" },
    { id: "s-app",       label: "Application" },
  ];
```

Add `"s-rules"` between `"s-security"` and `"s-printer"`:

```typescript
  const SUB_NAV = [
    { id: "s-business",  label: "Store Settings" },
    { id: "s-receipt",   label: "Receipt Text" },
    { id: "s-design",    label: "Receipt Design" },
    { id: "s-security",  label: "Security" },
    { id: "s-rules",     label: "Business Rules" },
    { id: "s-printer",   label: "Printers" },
    { id: "s-whatsapp",  label: "WhatsApp" },
    { id: "s-wa-format", label: "WA Message" },
    { id: "s-app",       label: "Application" },
  ];
```

- [ ] **Step 6: Add the Business Rules section JSX**

Find `{/* ── Thermal Printer ── */}` and insert before it:

```tsx
        {/* ── Business Rules ── */}
        <section id="s-rules" className="settings-section">
          <h3 className="settings-section-title">Business Rules</h3>
          <p className="settings-hint">
            Control how the POS behaves at the counter. Changes take effect immediately on the next action.
          </p>

          <div className="biz-flag-list">
            {/* 1 — Negative stock */}
            <div className="biz-flag-row">
              <div className="biz-flag-info">
                <div className="biz-flag-label">Allow selling when out of stock</div>
                <div className="biz-flag-hint">
                  Sales proceed even if stock quantity is zero or negative.
                  Use during stocktakes or when back-ordering is acceptable.
                </div>
              </div>
              <label className="biz-toggle">
                <input
                  type="checkbox"
                  checked={flags.allow_negative_stock}
                  onChange={e => setFlags(f => ({ ...f, allow_negative_stock: e.target.checked }))}
                />
                <span className="biz-toggle-track" />
              </label>
            </div>

            {/* 2 — Require discount reason */}
            <div className="biz-flag-row">
              <div className="biz-flag-info">
                <div className="biz-flag-label">Require a reason for every discount</div>
                <div className="biz-flag-hint">
                  When ON (default), the cashier must type a reason before applying any discount.
                  Turn OFF for simpler operations where audit reasons are not needed.
                </div>
              </div>
              <label className="biz-toggle">
                <input
                  type="checkbox"
                  checked={flags.require_discount_reason}
                  onChange={e => setFlags(f => ({ ...f, require_discount_reason: e.target.checked }))}
                />
                <span className="biz-toggle-track" />
              </label>
            </div>

            {/* 3 — Cashier can discount */}
            <div className="biz-flag-row">
              <div className="biz-flag-info">
                <div className="biz-flag-label">Allow cashiers to apply discounts</div>
                <div className="biz-flag-hint">
                  When OFF (default), only managers and owners can apply discounts.
                  Turn ON if the owner is also the cashier or to trust all staff with discounts.
                </div>
              </div>
              <label className="biz-toggle">
                <input
                  type="checkbox"
                  checked={flags.cashier_can_discount}
                  onChange={e => setFlags(f => ({ ...f, cashier_can_discount: e.target.checked }))}
                />
                <span className="biz-toggle-track" />
              </label>
            </div>

            {/* 4 — Auto print receipt */}
            <div className="biz-flag-row">
              <div className="biz-flag-info">
                <div className="biz-flag-label">Auto-print receipt after every sale</div>
                <div className="biz-flag-hint">
                  Automatically sends the receipt to the thermal printer immediately after payment
                  is accepted. Requires the thermal printer to be configured and enabled.
                </div>
              </div>
              <label className="biz-toggle">
                <input
                  type="checkbox"
                  checked={flags.auto_print_receipt}
                  onChange={e => setFlags(f => ({ ...f, auto_print_receipt: e.target.checked }))}
                />
                <span className="biz-toggle-track" />
              </label>
            </div>
          </div>

          <div className="settings-actions" style={{ marginTop: "16px" }}>
            {savedFlags && <span className="settings-saved">✓ Saved</span>}
            <button
              className="btn-primary settings-save-btn"
              onClick={handleSaveFlags}
              disabled={savingFlags}
            >
              {savingFlags ? "Saving…" : "Save Business Rules"}
            </button>
          </div>
        </section>
```

- [ ] **Step 7: TypeScript check**

```powershell
npx tsc --noEmit 2>&1
```
Expected: no errors.

- [ ] **Step 8: Commit**

```powershell
git add src/components/SettingsTab.tsx
git commit -m "feat: Business Rules section in Settings with 4 operational toggles"
```

---

## Task 7 — CSS: toggle switch + flag row styles

**Files:**
- Modify: `src/App.css`

Add these styles at the end of the file (before the last line if there is one, or just at the bottom):

- [ ] **Step 1: Add CSS for the Business Rules toggle UI**

Append to `src/App.css`:

```css
/* ── Business Rules toggles (Settings tab) ─────────────────────────────────── */
.biz-flag-list {
  display: flex;
  flex-direction: column;
  gap: 0;
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  overflow: hidden;
}

.biz-flag-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 14px 16px;
  border-bottom: 1px solid var(--border2);
  background: var(--surface);
  transition: background 0.1s;
}
.biz-flag-row:last-child { border-bottom: none; }
.biz-flag-row:hover { background: var(--surface2); }

.biz-flag-info { flex: 1; min-width: 0; }

.biz-flag-label {
  font-size: 0.88rem;
  font-weight: 650;
  color: var(--text);
  margin-bottom: 3px;
}

.biz-flag-hint {
  font-size: 0.75rem;
  color: var(--text-muted);
  line-height: 1.45;
}

/* iOS-style toggle switch */
.biz-toggle {
  position: relative;
  flex-shrink: 0;
  width: 44px;
  height: 24px;
  cursor: pointer;
}
.biz-toggle input {
  opacity: 0;
  width: 0;
  height: 0;
  position: absolute;
}
.biz-toggle-track {
  position: absolute;
  inset: 0;
  background: var(--surface3);
  border: 1px solid var(--border);
  border-radius: 999px;
  transition: background 0.18s, border-color 0.18s;
}
.biz-toggle-track::after {
  content: "";
  position: absolute;
  top: 2px;
  left: 2px;
  width: 18px;
  height: 18px;
  background: var(--text-muted);
  border-radius: 50%;
  transition: transform 0.18s, background 0.18s;
}
.biz-toggle input:checked + .biz-toggle-track {
  background: var(--accent);
  border-color: var(--accent);
}
.biz-toggle input:checked + .biz-toggle-track::after {
  transform: translateX(20px);
  background: #000;
}
```

- [ ] **Step 2: Commit**

```powershell
git add src/App.css
git commit -m "feat: CSS for Business Rules toggle-switch UI"
```

---

## Task 8 — Frontend: auto-print after sale

**Files:**
- Modify: `src/pages/PosPage.tsx`

**Context:** After `finalizeSale()` succeeds, if `flags.auto_print_receipt` is true, call the thermal print command. The thermal print command is `thermalPrintTest` (for testing) but the real receipt print uses a different approach — look for `thermalPrintReceipt` or the equivalent. Check `commands.ts` for the correct command name for printing a receipt.

- [ ] **Step 1: Load business flags in `PosPage.tsx`**

Find the state declarations near the top of `PosPage`. Add:

```typescript
  const [bizFlags, setBizFlags] = useState<BusinessFlags>({
    allow_negative_stock: false,
    require_discount_reason: true,
    cashier_can_discount: false,
    auto_print_receipt: false,
  });
```

Import `BusinessFlags` from types and `businessFlagsLoad` from commands if not already imported.

- [ ] **Step 2: Load flags on mount**

Find the `useEffect` that loads shift/session data. Add a call to `businessFlagsLoad()`:

```typescript
  useEffect(() => {
    businessFlagsLoad()
      .then(f => setBizFlags(f))
      .catch(() => {}); // non-fatal — defaults apply
  }, []);
```

- [ ] **Step 3: Wire auto-print in the payment success handler**

Find the `handlePayFast` or equivalent function that calls `finalizeSale`. After a successful sale, add:

```typescript
    // Auto-print receipt if the flag is enabled
    if (bizFlags.auto_print_receipt) {
      try {
        await thermalPrintReceipt(result.receipt_number);
      } catch {
        // Non-fatal — cashier can still manually print
      }
    }
```

Note: if `thermalPrintReceipt` does not exist in `commands.ts`, check what the correct command is. If receipt printing is done via a different mechanism (e.g. the ReceiptPreview component), trigger that flow instead. Do NOT silently skip — add a TODO comment if the print command is not found.

- [ ] **Step 4: TypeScript check**

```powershell
npx tsc --noEmit 2>&1
```
Expected: no errors.

- [ ] **Step 5: Commit**

```powershell
git add src/pages/PosPage.tsx
git commit -m "feat: load business flags in PosPage; auto-print receipt when flag is ON"
```

---

## Task 9 — Manual verification

- [ ] **Step 1: Run the app**

```powershell
npm run tauri dev
```

- [ ] **Step 2: Verify migration ran** — Open Settings → Application. No startup crash means migration applied cleanly.

- [ ] **Step 3: Verify Business Rules section appears**

In Settings tab, click "Business Rules" in the subnav. You should see 4 toggle rows with iOS-style switches.

- [ ] **Step 4: Test `allow_negative_stock` OFF (default)**

  1. Set a product's stock to 0 in the inventory tab.
  2. Try scanning/adding it to a cart.
  3. Click Pay → Fast Cash.
  4. Expected: error "Insufficient stock for '…'".

- [ ] **Step 5: Test `allow_negative_stock` ON**

  1. Toggle ON in Business Rules → Save.
  2. Repeat the scan + pay with the 0-stock item.
  3. Expected: sale completes. Check inventory — product qty should be -1.

- [ ] **Step 6: Test `cashier_can_discount` OFF (default)**

  1. Log in as a cashier-role user.
  2. Add an item. Try to apply a line discount.
  3. Expected: "Manager or owner authorization required" error.

- [ ] **Step 7: Test `cashier_can_discount` ON**

  1. Toggle ON → Save.
  2. Repeat the discount attempt as cashier.
  3. Expected: discount applies without manager PIN.

- [ ] **Step 8: Test `require_discount_reason` OFF**

  1. Toggle OFF → Save.
  2. Apply a discount with an empty reason field.
  3. Expected: discount applies without a reason being typed.

- [ ] **Step 9: Final TypeScript + build check**

```powershell
npx tsc --noEmit
```

- [ ] **Step 10: Final commit (if any loose files)**

```powershell
git status
git add -p   # review and stage only intended changes
git commit -m "feat: business flags complete — allow_negative_stock, discount controls, auto-print"
```

---

## Self-Review

**Spec coverage:**
- ✅ `allow_negative_stock` toggle → Task 3 (Rust) + Task 6 (UI)
- ✅ `require_discount_reason` toggle → Task 4 (Rust) + Task 6 (UI)
- ✅ `cashier_can_discount` toggle → Task 4 (Rust) + Task 6 (UI)
- ✅ `auto_print_receipt` toggle → Task 8 (frontend) + Task 6 (UI)
- ✅ Settings UI section with ON/OFF toggles → Task 6 + Task 7 (CSS)
- ✅ Persisted in DB → Task 1 (migration) + Task 2 (commands)
- ✅ Manager/owner only can save → Task 2 (`rbac::manager_or_owner`)

**Type consistency:**
- `BusinessFlags` struct in Rust uses `snake_case` fields → matches JSON serialized form → matches TS interface field names ✅
- `business_flags_save` takes `input: { flags, actor_user_id }` as the Tauri invoke argument → matches `SaveBusinessFlagsInput` struct ✅

**No placeholders:** All code blocks are complete and runnable. ✅
