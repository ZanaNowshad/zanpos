# Ghost Barcode Lookup — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When a cashier scans an unknown barcode, record it silently, then let the manager trigger online lookups (UPCitemdb → Open Food Facts → AI fallback) from the Products tab and one-click create missing products from the results.

**Architecture:** Lazy resolution — on scan failure a SQLite INSERT fires (fire-and-forget); a manager-only "Look up now" button in BackOffice Products tab triggers HTTP lookups for all pending barcodes; results displayed as dismissible cards with "Create Product" pre-fill.

**Tech Stack:** Rust/Tauri commands, `reqwest` (already in Cargo.toml), `sqlx`, React/TypeScript, UPCitemdb free API, Open Food Facts API, optional Anthropic/OpenAI fallback via OS credential store.

**Spec:** `docs/superpowers/specs/2026-05-29-ghost-barcode-design.md`

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `src-tauri/migrations/0025_unknown_barcodes.sql` | CREATE | DB schema for ghost barcodes table |
| `src-tauri/src/commands/ghost_barcode_commands.rs` | CREATE | All 6 Tauri commands + HTTP lookup chain |
| `src-tauri/src/commands/mod.rs` | MODIFY | Add `pub mod ghost_barcode_commands;` |
| `src-tauri/src/lib.rs` | MODIFY | Register 6 commands in `invoke_handler` |
| `src/types.ts` | MODIFY | Add 4 new TypeScript interfaces |
| `src/tauri/commands.ts` | MODIFY | Add 6 command wrapper functions |
| `src/components/GhostBarcodesPanel.tsx` | CREATE | Panel component with loading/results/dismiss UI |
| `src/components/BackOfficeModal.tsx` | MODIFY | Load summary on mount, badge on Products nav, pass props |
| `src/pages/PosPage.tsx` | MODIFY | Call `recordUnknownBarcode` in scan error handler |
| `src/App.css` | MODIFY | Ghost panel + badge styles |

---

## Task 1: SQLite Migration

**Files:**
- Create: `src-tauri/migrations/0025_unknown_barcodes.sql`

- [ ] **Step 1: Create migration file**

```sql
-- Migration 0025: Ghost barcode lookup table
-- Records barcodes that failed product lookup so managers can
-- resolve them later via online barcode databases.

CREATE TABLE IF NOT EXISTS unknown_barcodes (
    id              TEXT PRIMARY KEY NOT NULL,
    barcode         TEXT NOT NULL UNIQUE,
    scan_count      INTEGER NOT NULL DEFAULT 1,
    first_seen_at   INTEGER NOT NULL,
    last_seen_at    INTEGER NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending'
                        CHECK(status IN ('pending','found','not_found','dismissed')),
    product_name    TEXT,
    brand           TEXT,
    category        TEXT,
    image_url       TEXT,
    raw_json        TEXT
);

CREATE INDEX IF NOT EXISTS idx_unknown_barcodes_status
    ON unknown_barcodes(status);
```

- [ ] **Step 2: Verify migration compiles and applies**

Run from `src-tauri/`:
```bash
cargo test -- --test-thread=1 2>&1 | tail -20
```
Expected: all existing tests pass (migration is applied in-memory via `sqlx::migrate!("./migrations")`).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/migrations/0025_unknown_barcodes.sql
git commit -m "feat: add unknown_barcodes migration (ghost barcode lookup)"
```

---

## Task 2: Rust DB Commands

**Files:**
- Create: `src-tauri/src/commands/ghost_barcode_commands.rs`

This task adds all commands **except** `resolve_ghost_barcodes` (HTTP chain is Task 3).

- [ ] **Step 1: Create the module with structs and DB-only commands**

Create `src-tauri/src/commands/ghost_barcode_commands.rs`:

```rust
use crate::commands::rbac;
use crate::errors::{AppError, AppResult};
use crate::AppState;
use serde::{Deserialize, Serialize};
use tauri::State;
use ulid::Ulid;

// ── Output types ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct GhostBarcode {
    pub id: String,
    pub barcode: String,
    pub scan_count: i64,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    pub status: String,
    pub product_name: Option<String>,
    pub brand: Option<String>,
    pub category: Option<String>,
    pub image_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GhostSummary {
    pub pending: i64,
    pub found: i64,
    pub not_found: i64,
}

#[derive(Debug, Serialize)]
pub struct ProductPrefill {
    pub name: String,
    pub barcode: String,
    pub brand: Option<String>,
    pub category: Option<String>,
    pub image_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ResolveResult {
    pub resolved: i64,
    pub not_found: i64,
}

// ── Helper: current time as Unix milliseconds ─────────────────────────────────

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Called by the POS frontend whenever a barcode scan fails.
/// No RBAC — any authenticated user (cashier) may call this.
/// Fire-and-forget from the frontend: returns Ok(()) always.
#[tauri::command]
pub async fn ghost_record(
    barcode: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    if barcode.trim().is_empty() {
        return Ok(());
    }
    let id = Ulid::new().to_string();
    let now = now_ms();
    sqlx::query(
        "INSERT INTO unknown_barcodes (id, barcode, scan_count, first_seen_at, last_seen_at)
         VALUES (?, ?, 1, ?, ?)
         ON CONFLICT(barcode) DO UPDATE
           SET scan_count   = scan_count + 1,
               last_seen_at = excluded.last_seen_at
         WHERE status = 'pending'",
    )
    .bind(&id)
    .bind(&barcode)
    .bind(now)
    .bind(now)
    .execute(&state.db)
    .await?;
    Ok(())
}

/// Returns counts of pending/found/not_found barcodes.
/// Used by BackOffice nav badge. Manager/owner only.
#[tauri::command]
pub async fn ghost_summary(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<GhostSummary> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT status, COUNT(*) as cnt
         FROM unknown_barcodes
         WHERE status != 'dismissed'
         GROUP BY status",
    )
    .fetch_all(&state.db)
    .await?;

    let mut summary = GhostSummary { pending: 0, found: 0, not_found: 0 };
    for (status, cnt) in rows {
        match status.as_str() {
            "pending"   => summary.pending   = cnt,
            "found"     => summary.found     = cnt,
            "not_found" => summary.not_found = cnt,
            _ => {}
        }
    }
    Ok(summary)
}

/// Full list of non-dismissed barcodes, ordered by scan_count DESC.
/// Manager/owner only.
#[tauri::command]
pub async fn ghost_list(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<GhostBarcode>> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let rows = sqlx::query_as!(
        GhostBarcode,
        "SELECT id, barcode, scan_count, first_seen_at, last_seen_at,
                status, product_name, brand, category, image_url
         FROM unknown_barcodes
         WHERE status != 'dismissed'
         ORDER BY scan_count DESC, last_seen_at DESC"
    )
    .fetch_all(&state.db)
    .await?;
    Ok(rows)
}

/// Set a ghost barcode to 'dismissed' — removes it from the panel.
/// Manager/owner only.
#[tauri::command]
pub async fn ghost_dismiss(
    id: String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    sqlx::query("UPDATE unknown_barcodes SET status = 'dismissed' WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;
    Ok(())
}

/// Returns product data pre-filled from a 'found' ghost barcode row.
/// Used by "Create Product" button to pre-fill the product form.
/// Manager/owner only.
#[tauri::command]
pub async fn ghost_prefill(
    id: String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<ProductPrefill> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let row: Option<(String, String, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT product_name, barcode, brand, category, image_url
             FROM unknown_barcodes
             WHERE id = ? AND status = 'found'",
        )
        .bind(&id)
        .fetch_optional(&state.db)
        .await?;

    let (name, barcode, brand, category, image_url) =
        row.ok_or_else(|| AppError::NotFound("Ghost barcode not found or not resolved".into()))?;

    Ok(ProductPrefill { name, barcode, brand, category, image_url })
}
```

- [ ] **Step 2: Add module to mod.rs**

Edit `src-tauri/src/commands/mod.rs` — add one line in alphabetical order:

```rust
pub mod ghost_barcode_commands;
```

(Add after `pub mod device_commands;`)

- [ ] **Step 3: Check compilation**

```bash
cd src-tauri && cargo check 2>&1 | grep -E "^error" | head -20
```
Expected: no `error` lines (warnings are fine).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/ghost_barcode_commands.rs src-tauri/src/commands/mod.rs
git commit -m "feat: ghost barcode DB commands (record, summary, list, dismiss, prefill)"
```

---

## Task 3: HTTP Lookup Chain (`ghost_resolve`)

**Files:**
- Modify: `src-tauri/src/commands/ghost_barcode_commands.rs`

Append `ghost_resolve` to the file created in Task 2.

- [ ] **Step 1: Append the resolve command**

Add the following to the bottom of `ghost_barcode_commands.rs` (after `ghost_prefill`):

```rust
// ── HTTP lookup helpers ───────────────────────────────────────────────────────

/// Try UPCitemdb free tier.
/// Returns Some((name, brand, category, image_url, raw_json)) on hit.
async fn lookup_upcitemdb(
    client: &reqwest::Client,
    barcode: &str,
) -> Option<(String, Option<String>, Option<String>, Option<String>, String)> {
    let url = format!(
        "https://api.upcitemdb.com/prod/trial/lookup?upc={}",
        barcode
    );
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let text = resp.text().await.ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;

    let item = json.get("items")?.get(0)?;
    let name = item.get("title")?.as_str()?.to_string();
    if name.is_empty() {
        return None;
    }
    let brand     = item.get("brand").and_then(|v| v.as_str()).map(str::to_string);
    let category  = item.get("category").and_then(|v| v.as_str()).map(str::to_string);
    let image_url = item
        .get("images")
        .and_then(|v| v.get(0))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    Some((name, brand, category, image_url, text))
}

/// Try Open Food Facts.
/// Returns Some((name, brand, category, image_url, raw_json)) on hit.
async fn lookup_off(
    client: &reqwest::Client,
    barcode: &str,
) -> Option<(String, Option<String>, Option<String>, Option<String>, String)> {
    let url = format!(
        "https://world.openfoodfacts.org/api/v0/product/{}.json",
        barcode
    );
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .header("User-Agent", "ZANPOS/1.0 (contact@zanpos.app)")
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let text = resp.text().await.ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;

    if json.get("status")?.as_i64()? != 1 {
        return None;
    }
    let product  = json.get("product")?;
    let name = product
        .get("product_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if name.is_empty() {
        return None;
    }
    let brand     = product.get("brands").and_then(|v| v.as_str()).map(|s| {
        // brands is comma-separated; take first
        s.split(',').next().unwrap_or(s).trim().to_string()
    });
    let category  = product
        .get("categories_tags")
        .and_then(|v| v.get(0))
        .and_then(|v| v.as_str())
        .map(|s| s.trim_start_matches("en:").replace('-', " "))
        .map(|s| {
            let mut c = s.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        });
    let image_url = product
        .get("image_url")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    Some((name, brand, category, image_url, text))
}

/// Try AI fallback (one-shot Anthropic or OpenAI call).
/// Returns Some((name, brand, category)) on success.
async fn lookup_ai(
    client: &reqwest::Client,
    barcode: &str,
    pool: &sqlx::SqlitePool,
) -> Option<(String, Option<String>, Option<String>)> {
    // Determine configured provider
    let provider: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_config WHERE key = 'ai_provider'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .flatten();

    let prompt = format!(
        "The barcode {} was scanned at a retail POS but was not found in the product database. \
         Based on this barcode number, identify the product if you can. \
         Reply with ONLY valid JSON in this exact format: \
         {{\"product_name\": \"\", \"brand\": \"\", \"category\": \"\"}} \
         If you cannot identify the product, reply with: \
         {{\"product_name\": null, \"brand\": null, \"category\": null}}",
        barcode
    );

    match provider.as_deref() {
        Some("anthropic") | None => {
            // Try Anthropic
            let key = crate::secure_store::get_secret("anthropic_api_key")?;
            if key.is_empty() {
                return None;
            }
            let body = serde_json::json!({
                "model": "claude-haiku-4-5",
                "max_tokens": 128,
                "messages": [{ "role": "user", "content": prompt }]
            });
            let resp = client
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", &key)
                .header("anthropic-version", "2023-06-01")
                .json(&body)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
                .ok()?;
            let text = resp.text().await.ok()?;
            let json: serde_json::Value = serde_json::from_str(&text).ok()?;
            let content = json
                .get("content")?
                .get(0)?
                .get("text")?
                .as_str()?;
            parse_ai_json(content)
        }
        Some("openai") => {
            let key = crate::secure_store::get_secret("openai_api_key")?;
            if key.is_empty() {
                return None;
            }
            let base_url: String = sqlx::query_scalar(
                "SELECT value FROM app_config WHERE key = 'openai_base_url'",
            )
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .flatten()
            .unwrap_or_else(|| "https://api.openai.com/v1".into());
            let model: String = sqlx::query_scalar(
                "SELECT value FROM app_config WHERE key = 'openai_model'",
            )
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .flatten()
            .unwrap_or_else(|| "gpt-4o-mini".into());

            let body = serde_json::json!({
                "model": model,
                "max_tokens": 128,
                "messages": [{ "role": "user", "content": prompt }]
            });
            let resp = client
                .post(format!("{}/chat/completions", base_url))
                .bearer_auth(&key)
                .json(&body)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
                .ok()?;
            let text = resp.text().await.ok()?;
            let json: serde_json::Value = serde_json::from_str(&text).ok()?;
            let content = json
                .get("choices")?
                .get(0)?
                .get("message")?
                .get("content")?
                .as_str()?;
            parse_ai_json(content)
        }
        _ => None,
    }
}

/// Parse the JSON blob returned by the AI into (name, brand, category).
/// Returns None if product_name is null or missing.
fn parse_ai_json(text: &str) -> Option<(String, Option<String>, Option<String>)> {
    // Find the first '{' and last '}' to handle LLM preamble/postamble
    let start = text.find('{')?;
    let end   = text.rfind('}')?;
    let json: serde_json::Value = serde_json::from_str(&text[start..=end]).ok()?;
    let name = json.get("product_name")?.as_str()?.to_string();
    if name.is_empty() {
        return None;
    }
    let brand    = json.get("brand").and_then(|v| v.as_str()).map(str::to_string);
    let category = json.get("category").and_then(|v| v.as_str()).map(str::to_string);
    Some((name, brand, category))
}

// ── Resolve command ───────────────────────────────────────────────────────────

/// Runs the HTTP lookup chain for all 'pending' barcodes.
/// Stops at the first tier that returns a result for each barcode.
/// Updates rows in place and returns a summary.
/// Manager/owner only.
#[tauri::command]
pub async fn ghost_resolve(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<ResolveResult> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;

    // Fetch all pending barcodes
    let pending: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, barcode FROM unknown_barcodes WHERE status = 'pending'",
    )
    .fetch_all(&state.db)
    .await?;

    if pending.is_empty() {
        return Ok(ResolveResult { resolved: 0, not_found: 0 });
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Internal(format!("HTTP client build failed: {e}")))?;

    let mut resolved  = 0i64;
    let mut not_found = 0i64;

    for (id, barcode) in &pending {
        // Tier 1: UPCitemdb
        if let Some((name, brand, category, image_url, raw_json)) =
            lookup_upcitemdb(&client, barcode).await
        {
            sqlx::query(
                "UPDATE unknown_barcodes
                 SET status='found', product_name=?, brand=?, category=?, image_url=?, raw_json=?
                 WHERE id=?",
            )
            .bind(&name)
            .bind(&brand)
            .bind(&category)
            .bind(&image_url)
            .bind(&raw_json)
            .bind(id)
            .execute(&state.db)
            .await?;
            resolved += 1;
            continue;
        }

        // Tier 2: Open Food Facts
        if let Some((name, brand, category, image_url, raw_json)) =
            lookup_off(&client, barcode).await
        {
            sqlx::query(
                "UPDATE unknown_barcodes
                 SET status='found', product_name=?, brand=?, category=?, image_url=?, raw_json=?
                 WHERE id=?",
            )
            .bind(&name)
            .bind(&brand)
            .bind(&category)
            .bind(&image_url)
            .bind(&raw_json)
            .bind(id)
            .execute(&state.db)
            .await?;
            resolved += 1;
            continue;
        }

        // Tier 3: AI fallback
        if let Some((name, brand, category)) = lookup_ai(&client, barcode, &state.db).await {
            let raw_json = serde_json::json!({
                "source": "ai_fallback",
                "product_name": name,
                "brand": brand,
                "category": category
            })
            .to_string();
            sqlx::query(
                "UPDATE unknown_barcodes
                 SET status='found', product_name=?, brand=?, category=?, raw_json=?
                 WHERE id=?",
            )
            .bind(&name)
            .bind(&brand)
            .bind(&category)
            .bind(&raw_json)
            .bind(id)
            .execute(&state.db)
            .await?;
            resolved += 1;
            continue;
        }

        // All tiers exhausted
        sqlx::query("UPDATE unknown_barcodes SET status='not_found' WHERE id=?")
            .bind(id)
            .execute(&state.db)
            .await?;
        not_found += 1;
    }

    Ok(ResolveResult { resolved, not_found })
}
```

- [ ] **Step 2: Verify compilation**

```bash
cd src-tauri && cargo check 2>&1 | grep -E "^error" | head -20
```
Expected: no `error` lines.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/ghost_barcode_commands.rs
git commit -m "feat: ghost barcode HTTP lookup chain (UPCitemdb + OFF + AI fallback)"
```

---

## Task 4: Register Commands in lib.rs

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add 6 commands to `invoke_handler`**

In `src-tauri/src/lib.rs`, find the block ending with:
```rust
            commands::migration_commands::migration_rollback,
            commands::migration_commands::migration_agent_chat,
```

Add after `migration_agent_chat,`:
```rust
            // Ghost barcode lookup
            commands::ghost_barcode_commands::ghost_record,
            commands::ghost_barcode_commands::ghost_summary,
            commands::ghost_barcode_commands::ghost_list,
            commands::ghost_barcode_commands::ghost_resolve,
            commands::ghost_barcode_commands::ghost_dismiss,
            commands::ghost_barcode_commands::ghost_prefill,
```

- [ ] **Step 2: Build and run tests**

```bash
cd src-tauri && cargo test 2>&1 | tail -20
```
Expected: all tests pass; no failures.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: register ghost barcode commands in Tauri invoke handler"
```

---

## Task 5: TypeScript Types and Command Wrappers

**Files:**
- Modify: `src/types.ts`
- Modify: `src/tauri/commands.ts`

- [ ] **Step 1: Add types to `src/types.ts`**

Append to the end of `src/types.ts`:

```typescript
// ── Ghost Barcode types ───────────────────────────────────────────────────────

export interface GhostBarcode {
  id: string;
  barcode: string;
  scan_count: number;
  first_seen_at: number;
  last_seen_at: number;
  status: 'pending' | 'found' | 'not_found' | 'dismissed';
  product_name: string | null;
  brand: string | null;
  category: string | null;
  image_url: string | null;
}

export interface GhostSummary {
  pending: number;
  found: number;
  not_found: number;
}

export interface ProductPrefill {
  name: string;
  barcode: string;
  brand: string | null;
  category: string | null;
  image_url: string | null;
}

export interface ResolveResult {
  resolved: number;
  not_found: number;
}
```

- [ ] **Step 2: Add imports to `src/tauri/commands.ts`**

In the `import type { ... }` block at the top of `commands.ts`, add `GhostBarcode, GhostSummary, ProductPrefill, ResolveResult` to the import list (alphabetical order among the G entries, or append after the last import).

The import block currently has types like `AdminProduct`, `AdminUserRow`, etc. Add:
```typescript
  GhostBarcode,
  GhostSummary,
  ProductPrefill,
  ResolveResult,
```

- [ ] **Step 3: Add command wrappers to `src/tauri/commands.ts`**

Append to the end of `src/tauri/commands.ts`:

```typescript
// ── Ghost barcode lookup ──────────────────────────────────────────────────────

/** Record a failed barcode scan. Fire-and-forget — never throws. */
export const ghostRecord = (barcode: string): Promise<void> =>
  invoke("ghost_record", { barcode }).catch(() => {});

/** Get counts of pending/found/not_found ghost barcodes. Manager+ only. */
export const ghostSummary = (actorUserId: string): Promise<GhostSummary> =>
  invoke("ghost_summary", { actorUserId });

/** Full list of non-dismissed ghost barcodes. Manager+ only. */
export const ghostList = (actorUserId: string): Promise<GhostBarcode[]> =>
  invoke("ghost_list", { actorUserId });

/** Run HTTP lookup chain for all pending barcodes. Manager+ only. */
export const ghostResolve = (actorUserId: string): Promise<ResolveResult> =>
  invoke("ghost_resolve", { actorUserId });

/** Dismiss a ghost barcode (removes from panel). Manager+ only. */
export const ghostDismiss = (id: string, actorUserId: string): Promise<void> =>
  invoke("ghost_dismiss", { id, actorUserId });

/** Get product form pre-fill data from a 'found' ghost barcode. Manager+ only. */
export const ghostPrefill = (id: string, actorUserId: string): Promise<ProductPrefill> =>
  invoke("ghost_prefill", { id, actorUserId });
```

- [ ] **Step 4: TypeScript check**

```bash
cd .. && npx tsc --noEmit 2>&1 | grep -E "error TS" | head -20
```
Expected: no TypeScript errors.

- [ ] **Step 5: Commit**

```bash
git add src/types.ts src/tauri/commands.ts
git commit -m "feat: ghost barcode TypeScript types and command wrappers"
```

---

## Task 6: GhostBarcodesPanel Component

**Files:**
- Create: `src/components/GhostBarcodesPanel.tsx`

- [ ] **Step 1: Create the component**

```tsx
import { useState, useEffect, useCallback } from "react";
import type { GhostBarcode, GhostSummary, ProductPrefill } from "../types";
import {
  ghostList,
  ghostResolve,
  ghostDismiss,
  ghostPrefill,
} from "../tauri/commands";

interface Props {
  sessionUserId: string;
  summary: GhostSummary;
  /** Called after "Create Product" to pre-fill the product form */
  onCreateProduct: (prefill: ProductPrefill) => void;
  /** Called after any action that changes the count (dismiss/resolve) */
  onCountChange: () => void;
}

export default function GhostBarcodesPanel({
  sessionUserId,
  summary,
  onCreateProduct,
  onCountChange,
}: Props) {
  const [expanded, setExpanded]   = useState(false);
  const [items, setItems]         = useState<GhostBarcode[]>([]);
  const [loading, setLoading]     = useState(false);
  const [resolving, setResolving] = useState(false);
  const [error, setError]         = useState<string | null>(null);

  const total = summary.pending + summary.found + summary.not_found;

  // Load items whenever the panel is expanded
  const loadItems = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const rows = await ghostList(sessionUserId);
      setItems(rows);
    } catch (e) {
      setError(typeof e === "string" ? e : "Failed to load ghost barcodes");
    } finally {
      setLoading(false);
    }
  }, [sessionUserId]);

  useEffect(() => {
    if (expanded) loadItems();
  }, [expanded, loadItems]);

  const handleResolve = async () => {
    setResolving(true);
    setError(null);
    try {
      await ghostResolve(sessionUserId);
      await loadItems();
      onCountChange();
    } catch (e) {
      setError(typeof e === "string" ? e : "Lookup failed");
    } finally {
      setResolving(false);
    }
  };

  const handleDismiss = async (id: string) => {
    try {
      await ghostDismiss(id, sessionUserId);
      setItems(prev => prev.filter(i => i.id !== id));
      onCountChange();
    } catch (e) {
      setError(typeof e === "string" ? e : "Dismiss failed");
    }
  };

  const handleCreateProduct = async (id: string) => {
    try {
      const prefill = await ghostPrefill(id, sessionUserId);
      await ghostDismiss(id, sessionUserId);
      setItems(prev => prev.filter(i => i.id !== id));
      onCountChange();
      onCreateProduct(prefill);
    } catch (e) {
      setError(typeof e === "string" ? e : "Failed to get product data");
    }
  };

  if (total === 0) return null;

  return (
    <div className="ghost-panel">
      <button
        className="ghost-panel-header"
        onClick={() => setExpanded(e => !e)}
      >
        <span className="ghost-panel-badge">{total}</span>
        <span className="ghost-panel-title">
          Unrecognized barcodes
          {summary.pending > 0 && ` · ${summary.pending} pending lookup`}
          {summary.found   > 0 && ` · ${summary.found} ready to create`}
        </span>
        <span className="ghost-panel-chevron">{expanded ? "▲" : "▼"}</span>
      </button>

      {expanded && (
        <div className="ghost-panel-body">
          {error && <div className="ghost-error">{error}</div>}

          {summary.pending > 0 && (
            <div className="ghost-actions-row">
              <button
                className="ghost-lookup-btn"
                onClick={handleResolve}
                disabled={resolving}
              >
                {resolving ? "Looking up…" : `Look up ${summary.pending} barcode${summary.pending !== 1 ? "s" : ""} now`}
              </button>
            </div>
          )}

          {loading && <div className="ghost-loading">Loading…</div>}

          {!loading && items.length > 0 && (
            <div className="ghost-list">
              {items.map(item => (
                <div
                  key={item.id}
                  className={`ghost-item ghost-item-${item.status}`}
                >
                  <div className="ghost-item-left">
                    {item.image_url && (
                      <img
                        src={item.image_url}
                        alt={item.product_name ?? ""}
                        className="ghost-item-img"
                        onError={e => { (e.target as HTMLImageElement).style.display = "none"; }}
                      />
                    )}
                    <div className="ghost-item-info">
                      {item.status === "found" ? (
                        <>
                          <div className="ghost-item-name">{item.product_name}</div>
                          {item.brand    && <div className="ghost-item-meta">{item.brand}</div>}
                          {item.category && <div className="ghost-item-meta">{item.category}</div>}
                        </>
                      ) : item.status === "not_found" ? (
                        <div className="ghost-item-name ghost-not-found">
                          Not found in any database
                        </div>
                      ) : (
                        <div className="ghost-item-name ghost-pending">
                          Pending lookup…
                        </div>
                      )}
                      <div className="ghost-item-barcode">
                        #{item.barcode}
                        <span className="ghost-item-count">
                          · scanned {item.scan_count}×
                        </span>
                      </div>
                    </div>
                  </div>

                  <div className="ghost-item-right">
                    {item.status === "found" && (
                      <button
                        className="ghost-btn ghost-btn-create"
                        onClick={() => handleCreateProduct(item.id)}
                      >
                        + Create Product
                      </button>
                    )}
                    <button
                      className="ghost-btn ghost-btn-dismiss"
                      onClick={() => handleDismiss(item.id)}
                    >
                      Dismiss
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}

          {!loading && items.length === 0 && (
            <div className="ghost-empty">No unrecognized barcodes to show.</div>
          )}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: TypeScript check**

```bash
npx tsc --noEmit 2>&1 | grep -E "error TS" | head -20
```
Expected: no TypeScript errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/GhostBarcodesPanel.tsx
git commit -m "feat: GhostBarcodesPanel component with lookup/dismiss/prefill UI"
```

---

## Task 7: Wire Up — BackOfficeModal Badge + ProductsTab + PosPage Hook

**Files:**
- Modify: `src/components/BackOfficeModal.tsx`
- Modify: `src/components/ProductsTab.tsx`
- Modify: `src/pages/PosPage.tsx`

### Part A — BackOfficeModal: badge + load summary + pass props

- [ ] **Step 1: Add imports to `BackOfficeModal.tsx`**

At the top of `src/components/BackOfficeModal.tsx`, add:

```tsx
import { useEffect, useCallback, useState } from "react";
import GhostBarcodesPanel from "./GhostBarcodesPanel";
import type { GhostSummary, ProductPrefill } from "../types";
import { ghostSummary } from "../tauri/commands";
```

(The file currently has `import { type ReactNode, useMemo, useState }` — replace the `useState` import to also cover `useEffect` and `useCallback`, and keep `useMemo`.)

Full updated import line:
```tsx
import { type ReactNode, useCallback, useEffect, useMemo, useState } from "react";
```

- [ ] **Step 2: Add ghost state inside the `BackOfficeModal` component**

In the component body, after the existing state:
```tsx
const [showBulkStockTake, setShowBulkStockTake] = useState(false);
const [showSyncQueue, setShowSyncQueue]         = useState(false);
```

Add:
```tsx
const [ghostSum, setGhostSum] = useState<GhostSummary>({ pending: 0, found: 0, not_found: 0 });
const [productPrefill, setProductPrefill] = useState<ProductPrefill | null>(null);

const refreshGhostSummary = useCallback(async () => {
  if (!isManager) return;
  try {
    const s = await ghostSummary(sessionUser.user_id);
    setGhostSum(s);
  } catch { /* non-fatal */ }
}, [isManager, sessionUser.user_id]);

// Load summary when back office opens
useEffect(() => { refreshGhostSummary(); }, [refreshGhostSummary]);
```

- [ ] **Step 3: Add badge number to the Products nav item**

Find the nav item rendering loop in `BackOfficeModal.tsx`:
```tsx
<button
  key={item.id}
  className={`bo-nav-item${tab === item.id ? " bo-nav-item-active" : ""}`}
  onClick={() => setTab(item.id)}
>
  <span className="bo-nav-icon">{TAB_ICON[item.id]}</span>
  <span className="bo-nav-label">{item.label}</span>
</button>
```

Replace with:
```tsx
<button
  key={item.id}
  className={`bo-nav-item${tab === item.id ? " bo-nav-item-active" : ""}`}
  onClick={() => setTab(item.id)}
>
  <span className="bo-nav-icon">{TAB_ICON[item.id]}</span>
  <span className="bo-nav-label">{item.label}</span>
  {item.id === "products" && isManager && (ghostSum.pending + ghostSum.found) > 0 && (
    <span className="bo-nav-ghost-badge">
      {ghostSum.pending + ghostSum.found}
    </span>
  )}
</button>
```

- [ ] **Step 4: Handle productPrefill → switch to products tab and pass prefill**

When `productPrefill` is set, switch to the products tab automatically. In the component, add a `useEffect`:

```tsx
useEffect(() => {
  if (productPrefill) setTab("products");
}, [productPrefill]);
```

- [ ] **Step 5: Pass ghost props to ProductsTab**

Find:
```tsx
{tab === "products"   && <ProductsTab      sessionUserId={sessionUser.user_id} />}
```

Replace with:
```tsx
{tab === "products"   && (
  <ProductsTab
    sessionUserId={sessionUser.user_id}
    ghostSummary={ghostSum}
    ghostPrefill={productPrefill}
    onGhostPrefillConsumed={() => setProductPrefill(null)}
    onGhostCountChange={refreshGhostSummary}
    onCreateProductFromGhost={(prefill) => setProductPrefill(prefill)}
  />
)}
```

### Part B — ProductsTab: render GhostBarcodesPanel

- [ ] **Step 6: Add ghost props to `ProductsTab` interface and imports**

In `src/components/ProductsTab.tsx`, replace the existing import and Props:

```tsx
// Line 3 — expand the type import to include ghost types:
import type { AdminProduct, CategoryRow, GhostSummary, ProductBarcodeRow, ProductPrefill, TaxRuleRow } from "../types";
```

```tsx
// Line 10-12 — replace the Props interface entirely:
interface Props {
  sessionUserId: string;
  ghostSummary?: GhostSummary;
  ghostPrefill?: ProductPrefill | null;
  onGhostPrefillConsumed?: () => void;
  onGhostCountChange?: () => void;
  onCreateProductFromGhost?: (prefill: ProductPrefill) => void;
}
```

Add after the existing imports (line 9):
```tsx
import GhostBarcodesPanel from "./GhostBarcodesPanel";
```

Update the component function signature (line 26):
```tsx
export default function ProductsTab({
  sessionUserId,
  ghostSummary,
  ghostPrefill,
  onGhostPrefillConsumed,
  onGhostCountChange,
  onCreateProductFromGhost,
}: Props) {
```

- [ ] **Step 7: Consume `ghostPrefill` to pre-fill the create form**

In `ProductsTab.tsx`, add a `useEffect` that fires when `ghostPrefill` arrives:

```tsx
useEffect(() => {
  if (!ghostPrefill) return;
  // Start creating a new product, pre-fill the form fields
  setCreating(true);
  setForm(prev => ({
    ...prev,
    name:     ghostPrefill.name,
    barcode:  ghostPrefill.barcode,
  }));
  onGhostPrefillConsumed?.();
}, [ghostPrefill]);
```

(Check existing state variable names in `ProductsTab.tsx` — look for `setCreating` and `setForm`. Adapt field names to match what's already there.)

- [ ] **Step 8: Render `GhostBarcodesPanel` at the top of `ProductsTab`**

Find the return JSX in `ProductsTab.tsx`. At the very beginning of the component's returned JSX (before the product list/form), add:

```tsx
{ghostSummary && (ghostSummary.pending + ghostSummary.found + ghostSummary.not_found) > 0 && (
  <GhostBarcodesPanel
    sessionUserId={sessionUserId}
    summary={ghostSummary}
    onCreateProduct={(prefill) => onCreateProductFromGhost?.(prefill)}
    onCountChange={() => onGhostCountChange?.()}
  />
)}
```

### Part C — PosPage: record on scan failure

- [ ] **Step 9: Add `ghostRecord` to PosPage imports**

In `src/pages/PosPage.tsx`, find the import from `../tauri/commands`. Add `ghostRecord` to the destructured imports.

- [ ] **Step 10: Call `ghostRecord` in the scan error handler**

Find the `handleBarcode` function (around line 412):
```tsx
  const handleBarcode = useCallback((barcode: string, qty?: number) => {
    const effectiveQty = qty ?? (parseInt(numpadRef.current) || 1);
    scanQueueRef.current = scanQueueRef.current.then(async () => {
      try {
        await addByBarcode(barcode, effectiveQty);
        barcodeRef.current?.flashSuccess();
        setNumpadValue("1");
      } catch {
        barcodeRef.current?.flashError();
      } finally {
        focusBarcode();
      }
    });
  }, [addByBarcode, focusBarcode]);
```

Replace the `catch` block:
```tsx
      } catch {
        barcodeRef.current?.flashError();
        ghostRecord(barcode); // fire-and-forget, never throws
      } finally {
```

- [ ] **Step 11: TypeScript check**

```bash
npx tsc --noEmit 2>&1 | grep -E "error TS" | head -20
```

Fix any type errors before committing. Common issues:
- `ProductsTab` props type: make sure all new props are optional (`?`) in the interface since `ProductsTab` is also rendered without ghost props from other places (check — it's only rendered from `BackOfficeModal` but better to be safe)
- `GhostSummary` import in `ProductsTab` — ensure it's imported

- [ ] **Step 12: Commit**

```bash
git add src/components/BackOfficeModal.tsx src/components/ProductsTab.tsx src/pages/PosPage.tsx
git commit -m "feat: wire ghost barcode panel into BackOffice Products tab and POS scan handler"
```

---

## Task 8: CSS Styles

**Files:**
- Modify: `src/App.css`

- [ ] **Step 1: Append ghost barcode styles to `src/App.css`**

```css
/* ── Ghost Barcode Panel ──────────────────────────────────────────────────── */

.ghost-panel {
  border: 1px solid var(--accent);
  border-radius: 10px;
  margin-bottom: 1.25rem;
  overflow: hidden;
  background: color-mix(in srgb, var(--accent) 6%, var(--surface));
}

.ghost-panel-header {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 0.625rem;
  padding: 0.75rem 1rem;
  background: transparent;
  border: none;
  cursor: pointer;
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--text);
  text-align: left;
}
.ghost-panel-header:hover { background: color-mix(in srgb, var(--accent) 10%, transparent); }

.ghost-panel-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 22px;
  height: 22px;
  padding: 0 6px;
  border-radius: 11px;
  background: var(--accent);
  color: #fff;
  font-size: 0.75rem;
  font-weight: 700;
  flex-shrink: 0;
}

.ghost-panel-title { flex: 1; }
.ghost-panel-chevron { color: var(--text-muted); font-size: 0.75rem; }

.ghost-panel-body {
  border-top: 1px solid color-mix(in srgb, var(--accent) 20%, transparent);
  padding: 0.75rem 1rem 1rem;
  display: flex;
  flex-direction: column;
  gap: 0.625rem;
}

.ghost-error {
  color: var(--danger, #e53935);
  font-size: 0.8125rem;
  padding: 0.5rem 0.75rem;
  background: color-mix(in srgb, var(--danger, #e53935) 10%, transparent);
  border-radius: 6px;
}

.ghost-loading, .ghost-empty {
  color: var(--text-muted);
  font-size: 0.8125rem;
  padding: 0.5rem 0;
}

.ghost-actions-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.ghost-lookup-btn {
  padding: 0.4rem 0.875rem;
  border-radius: 6px;
  border: none;
  background: var(--accent);
  color: #fff;
  font-size: 0.8125rem;
  font-weight: 600;
  cursor: pointer;
  transition: opacity 0.15s;
}
.ghost-lookup-btn:disabled { opacity: 0.6; cursor: wait; }
.ghost-lookup-btn:not(:disabled):hover { opacity: 0.88; }

.ghost-list {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.ghost-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  padding: 0.625rem 0.875rem;
  border-radius: 8px;
  background: var(--surface-raised, var(--surface));
  border: 1px solid var(--border-subtle, color-mix(in srgb, var(--border) 50%, transparent));
}
.ghost-item-found    { border-color: color-mix(in srgb, var(--accent) 35%, transparent); }
.ghost-item-not_found { opacity: 0.7; }

.ghost-item-left {
  display: flex;
  align-items: center;
  gap: 0.625rem;
  min-width: 0;
  flex: 1;
}

.ghost-item-img {
  width: 40px;
  height: 40px;
  object-fit: contain;
  border-radius: 4px;
  background: var(--surface-muted, #f4f4f4);
  flex-shrink: 0;
}

.ghost-item-info {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.ghost-item-name {
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.ghost-not-found { color: var(--text-muted); font-weight: 400; font-style: italic; }
.ghost-pending   { color: var(--text-muted); font-weight: 400; }

.ghost-item-meta {
  font-size: 0.75rem;
  color: var(--text-muted);
}

.ghost-item-barcode {
  font-size: 0.75rem;
  color: var(--text-muted);
  font-family: monospace;
}
.ghost-item-count { margin-left: 0.25rem; }

.ghost-item-right {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  flex-shrink: 0;
}

.ghost-btn {
  padding: 0.3rem 0.625rem;
  border-radius: 5px;
  border: 1px solid var(--border);
  font-size: 0.75rem;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.12s;
}
.ghost-btn-create {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
}
.ghost-btn-create:hover { opacity: 0.88; }
.ghost-btn-dismiss {
  background: transparent;
  color: var(--text-muted);
}
.ghost-btn-dismiss:hover { background: var(--surface-raised, var(--surface)); }

/* Badge on BackOffice Products nav item */
.bo-nav-ghost-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 18px;
  height: 18px;
  padding: 0 5px;
  border-radius: 9px;
  background: var(--accent);
  color: #fff;
  font-size: 0.65rem;
  font-weight: 700;
  margin-left: auto;
}
```

- [ ] **Step 2: Verify no TypeScript/build errors**

```bash
npx tsc --noEmit 2>&1 | grep -E "error TS" | head -10
```
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/App.css
git commit -m "feat: ghost barcode panel CSS styles and nav badge"
```

---

## Task 9: Build + Smoke Test

**Files:** None modified — verification only.

- [ ] **Step 1: Run full Rust test suite**

```bash
cd src-tauri && cargo test 2>&1 | tail -30
```
Expected: all tests pass.

- [ ] **Step 2: Run TypeScript check**

```bash
cd .. && npx tsc --noEmit
```
Expected: exit code 0, no output.

- [ ] **Step 3: Build release binary**

```bash
cd src-tauri && cargo build --release 2>&1 | tail -10
```
Expected: `Finished release profile` with no errors.

- [ ] **Step 4: Manual smoke test**

Run `target/release/zanpos.exe` and verify:
1. App starts without crash
2. Log in as cashier, scan a known-missing barcode → app shows error flash (same as before) — no crash
3. Log in as manager → open Back Office → Products tab shows ghost badge if there are pending barcodes
4. Click "Look up now" → results appear (or "Not found" for test barcodes)
5. Click "Dismiss" → item disappears from panel
6. Click "Create Product" on a found barcode → product form opens with name/barcode pre-filled

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "feat: ghost barcode lookup — complete implementation

When a cashier scans an unknown barcode it is silently recorded.
Manager can open Back Office > Products and trigger online lookups
(UPCitemdb → Open Food Facts → AI fallback). Found barcodes appear
as one-click 'Create Product' cards. Dismissed barcodes are hidden.
"
```
