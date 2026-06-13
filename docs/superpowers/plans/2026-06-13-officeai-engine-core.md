# OfficeAI Engine Core (A1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the deterministic, reversible, checkpointed bulk-operation engine — Run ledger + declarative Selector + batched price-adjust executor with whole-run undo — proven end-to-end by a `cargo test` that runs "increase all Toys › Girls by 20%" over a synthetic DB and reverses it.

**Architecture:** A new `src-tauri/src/ai/engine/` module. A `Selector` compiles a declarative filter (category subtree via recursive CTE, active flag, name text) to SQL. The batch executor walks matches by keyset cursor (`product_id > :cursor ORDER BY product_id LIMIT 100`), applies exact fils math per batch inside a transaction, records a reverse-snapshot to an undo log after each batch, and updates a checkpoint on the run row. Undo replays the log in reverse. No LLM, no Tauri commands, no UI in this plan — those are A2/A3. This phase ships a tested Rust library subsystem.

**Tech Stack:** Rust, sqlx (runtime queries, SQLite), tokio tests, chrono, ulid. Verification: `cd src-tauri && cargo test --lib && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings`.

---

## Context for the engineer

- This is an offline-first Tauri 2 POS. The Rust crate is `C:\Users\super\ZAN\zanpos\src-tauri`. Run all `cargo` commands from there.
- Money is integer **minor units (fils)**; BHD has 3 decimal places, so 12.500 BHD = `12500`. Never use floats for stored money.
- The **current selling price** of a product is the `product_prices` row with `price_type='selling' AND effective_to IS NULL`. Prices are effective-dated: to change a price you close the open row (`effective_to = now`) and insert a new open row. This is how `bulk_update_prices` already works (`src-tauri/src/ai/tools.rs:4905`) and how undo restores the prior price.
- Categories are an adjacency list: `categories.parent_category_id` points to the parent (NULL at root). "Toys › Girls and everything under it" needs a recursive CTE.
- Errors use `crate::errors::{AppError, AppResult}`; `AppError::Validation(String)` exists. sqlx errors convert into `AppError` via `?` (existing code relies on this).
- Tests follow the in-memory pattern in `src-tauri/src/db/repositories/refund_repo.rs:499` and `src-tauri/src/inventory/movements.rs:514`: connect `sqlite::memory:`, then `sqlx::migrate!("./migrations").run(&pool)`. The new migration `0007_run_ledger.sql` is picked up automatically by that macro.
- sqlx queries here are **runtime** (`sqlx::query("…").bind(…)`), not the compile-time `query!` macro — no `DATABASE_URL` needed.

## File structure

- Create `src-tauri/migrations/0007_run_ledger.sql` — `ai_runs` + `ai_run_undo_log` tables.
- Create `src-tauri/src/ai/engine/mod.rs` — module root, public re-exports, shared types (`PriceOp`).
- Create `src-tauri/src/ai/engine/selector.rs` — `Selector` struct + SQL compilation + count/id-batch queries.
- Create `src-tauri/src/ai/engine/runs.rs` — Run ledger repo (create/status/progress/undo-log CRUD).
- Create `src-tauri/src/ai/engine/batch.rs` — price math + batched executor + `undo_run`.
- Modify `src-tauri/src/ai/mod.rs` — add `pub mod engine;`.

Each file has one responsibility; tests live in a `#[cfg(test)] mod tests` block in the file they exercise (`selector.rs`, `batch.rs`).

---

### Task 1: Run ledger migration

**Files:**
- Create: `src-tauri/migrations/0007_run_ledger.sql`
- Modify: `src-tauri/src/ai/mod.rs` (add module declaration so the test compiles)
- Create: `src-tauri/src/ai/engine/mod.rs` (minimal, so the module exists)

- [ ] **Step 1: Write the migration**

Create `src-tauri/migrations/0007_run_ledger.sql`:

```sql
-- AI bulk-operation Run ledger: durable, checkpointed, reversible tasks.
CREATE TABLE ai_runs (
    run_id            TEXT PRIMARY KEY,
    op_id             TEXT NOT NULL,
    selector_json     TEXT NOT NULL,
    params_json       TEXT NOT NULL,
    status            TEXT NOT NULL DEFAULT 'previewing',
    total_count       INTEGER NOT NULL DEFAULT 0,
    done_count        INTEGER NOT NULL DEFAULT 0,
    checkpoint_cursor TEXT,
    error             TEXT,
    created_by        TEXT NOT NULL,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL
);
CREATE INDEX idx_ai_runs_status ON ai_runs(status);

CREATE TABLE ai_run_undo_log (
    entry_id     TEXT PRIMARY KEY,
    run_id       TEXT NOT NULL REFERENCES ai_runs(run_id),
    batch_seq    INTEGER NOT NULL,
    reverse_json TEXT NOT NULL,
    applied      INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL
);
CREATE INDEX idx_ai_run_undo_run ON ai_run_undo_log(run_id);
```

- [ ] **Step 2: Create the module root**

Create `src-tauri/src/ai/engine/mod.rs`:

```rust
//! Deterministic bulk-operation engine: declarative selectors, checkpointed
//! batched execution, and whole-run undo. See docs/superpowers/specs.

pub mod batch;
pub mod runs;
pub mod selector;

/// A price mutation. Exact integer (fils) math; never floats for the result.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(tag = "mode", content = "value")]
pub enum PriceOp {
    /// Percentage change, e.g. +20.0 means ×1.20.
    Percent(f64),
    /// Add/subtract a fixed number of fils.
    Absolute(i64),
    /// Set every matched product to this fils value.
    Set(i64),
}
```

- [ ] **Step 3: Declare the module**

In `src-tauri/src/ai/mod.rs`, add alongside the existing `pub mod …` lines:

```rust
pub mod engine;
```

- [ ] **Step 4: Verify it compiles and the migration applies**

Run: `cd src-tauri && cargo build --lib`
Expected: builds (warnings about unused `PriceOp` are fine for now).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/migrations/0007_run_ledger.sql src-tauri/src/ai/engine/mod.rs src-tauri/src/ai/mod.rs
git commit -m "feat(engine): run ledger migration + engine module scaffold"
```

---

### Task 2: Selector — declarative filter → SQL

**Files:**
- Create: `src-tauri/src/ai/engine/selector.rs`

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/ai/engine/selector.rs` with the test first:

```rust
use crate::errors::AppResult;
use sqlx::SqlitePool;

/// Declarative product filter. A1 supports the fields the first demo needs;
/// price_range / stock / not_sold_since are added in later phases.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Selector {
    /// Resolve this category AND all descendants (adjacency list → recursive CTE).
    pub category_subtree: Option<String>,
    /// Restrict to active (true) or inactive (false) products.
    pub active: Option<bool>,
    /// Case-insensitive LIKE over name / sku / barcode.
    pub text: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup() -> SqlitePool {
        let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        // categories: Toys (root) → Girls (child) → Dolls (grandchild); plus Boys
        let now = "2026-01-01T00:00:00Z";
        for (id, parent) in [("toys", None), ("girls", Some("toys")), ("dolls", Some("girls")), ("boys", Some("toys"))] {
            sqlx::query("INSERT INTO categories (category_id,parent_category_id,name,sort_order,is_active,created_at,updated_at) VALUES (?,?,?,0,1,?,?)")
                .bind(id).bind(parent).bind(id).bind(now).bind(now).execute(&pool).await.unwrap();
        }
        // products: 2 under girls, 1 under dolls (descendant), 1 under boys (excluded)
        for (id, cat) in [("p_g1","girls"), ("p_g2","girls"), ("p_d1","dolls"), ("p_b1","boys")] {
            sqlx::query("INSERT INTO products (product_id,category_id,name,is_active,currency,reorder_point,created_at,updated_at) VALUES (?,?,?,1,'BHD',0,?,?)")
                .bind(id).bind(cat).bind(id).bind(now).bind(now).execute(&pool).await.unwrap();
        }
        pool
    }

    #[tokio::test]
    async fn subtree_includes_descendants_excludes_siblings() {
        let pool = setup().await;
        let sel = Selector { category_subtree: Some("girls".into()), ..Default::default() };
        let count = sel.count(&pool).await.unwrap();
        assert_eq!(count, 3); // p_g1, p_g2, p_d1 — NOT p_b1
    }

    #[tokio::test]
    async fn empty_selector_matches_all_active() {
        let pool = setup().await;
        let sel = Selector::default();
        assert_eq!(sel.count(&pool).await.unwrap(), 4);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd src-tauri && cargo test --lib engine::selector`
Expected: FAIL — `no method named count found for struct Selector`.

- [ ] **Step 3: Implement the selector SQL compilation**

Add to `src-tauri/src/ai/engine/selector.rs` (above the `#[cfg(test)]` block):

```rust
/// A compiled WHERE fragment with ordered bind values (all TEXT in this phase).
pub struct Compiled {
    /// Optional recursive-CTE prelude (empty string when no subtree filter).
    pub cte: String,
    /// SQL boolean conditions joined with AND, referencing `p` (products alias).
    pub where_sql: String,
    /// Bind values, in the order the placeholders appear across cte + where_sql.
    pub binds: Vec<String>,
}

impl Selector {
    /// Build the CTE + WHERE for this selector. Always excludes soft-deleted rows.
    pub fn compile(&self) -> Compiled {
        let mut cte = String::new();
        let mut conds: Vec<String> = vec!["p.deleted_at IS NULL".into()];
        let mut binds: Vec<String> = Vec::new();

        if let Some(cat) = &self.category_subtree {
            cte = "WITH RECURSIVE subtree(category_id) AS (\
                     SELECT category_id FROM categories WHERE category_id = ? \
                     UNION ALL \
                     SELECT c.category_id FROM categories c \
                       JOIN subtree s ON c.parent_category_id = s.category_id) ".into();
            binds.push(cat.clone());
            conds.push("p.category_id IN (SELECT category_id FROM subtree)".into());
        }
        if let Some(active) = self.active {
            conds.push(format!("p.is_active = {}", if active { 1 } else { 0 }));
        }
        if let Some(text) = &self.text {
            conds.push("(p.name LIKE ? OR p.sku LIKE ? OR p.barcode LIKE ?)".into());
            let like = format!("%{}%", text);
            binds.push(like.clone());
            binds.push(like.clone());
            binds.push(like);
        }
        Compiled { cte, where_sql: conds.join(" AND "), binds }
    }

    /// Count products matching this selector.
    pub async fn count(&self, pool: &SqlitePool) -> AppResult<i64> {
        let c = self.compile();
        let sql = format!("{} SELECT COUNT(*) FROM products p WHERE {}", c.cte, c.where_sql);
        let mut q = sqlx::query_scalar::<_, i64>(&sql);
        for b in &c.binds { q = q.bind(b); }
        Ok(q.fetch_one(pool).await?)
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd src-tauri && cargo test --lib engine::selector`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ai/engine/selector.rs
git commit -m "feat(engine): declarative Selector with category-subtree recursive CTE"
```

---

### Task 3: Run ledger repository

**Files:**
- Create: `src-tauri/src/ai/engine/runs.rs`

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/ai/engine/runs.rs`:

```rust
use crate::errors::AppResult;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct Run {
    pub run_id: String,
    pub op_id: String,
    pub status: String,
    pub total_count: i64,
    pub done_count: i64,
    pub checkpoint_cursor: Option<String>,
}

/// Create a run row in 'previewing' status. Returns the new run_id (ULID).
pub async fn create_run(
    pool: &SqlitePool,
    op_id: &str,
    selector_json: &str,
    params_json: &str,
    total_count: i64,
    created_by: &str,
) -> AppResult<String> {
    let run_id = ulid::Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO ai_runs (run_id,op_id,selector_json,params_json,status,total_count,done_count,created_by,created_at,updated_at) \
         VALUES (?,?,?,?,'previewing',?,0,?,?,?)",
    )
    .bind(&run_id).bind(op_id).bind(selector_json).bind(params_json)
    .bind(total_count).bind(created_by).bind(&now).bind(&now)
    .execute(pool).await?;
    Ok(run_id)
}

pub async fn set_status(pool: &SqlitePool, run_id: &str, status: &str) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE ai_runs SET status=?, updated_at=? WHERE run_id=?")
        .bind(status).bind(&now).bind(run_id).execute(pool).await?;
    Ok(())
}

pub async fn set_failed(pool: &SqlitePool, run_id: &str, error: &str) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE ai_runs SET status='failed', error=?, updated_at=? WHERE run_id=?")
        .bind(error).bind(&now).bind(run_id).execute(pool).await?;
    Ok(())
}

pub async fn get_run(pool: &SqlitePool, run_id: &str) -> AppResult<Run> {
    let row = sqlx::query_as::<_, (String, String, String, i64, i64, Option<String>)>(
        "SELECT run_id,op_id,status,total_count,done_count,checkpoint_cursor FROM ai_runs WHERE run_id=?",
    )
    .bind(run_id).fetch_one(pool).await?;
    Ok(Run { run_id: row.0, op_id: row.1, status: row.2, total_count: row.3, done_count: row.4, checkpoint_cursor: row.5 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let p = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&p).await.unwrap();
        p
    }

    #[tokio::test]
    async fn create_then_status_roundtrip() {
        let pool = pool().await;
        let id = create_run(&pool, "bulk.price_adjust", "{}", "{}", 312, "U1").await.unwrap();
        let r = get_run(&pool, &id).await.unwrap();
        assert_eq!(r.status, "previewing");
        assert_eq!(r.total_count, 312);
        set_status(&pool, &id, "done").await.unwrap();
        assert_eq!(get_run(&pool, &id).await.unwrap().status, "done");
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd src-tauri && cargo test --lib engine::runs`
Expected: FAIL until the file compiles — run it to confirm it builds then passes. (If the module isn't found, ensure `mod.rs` from Task 1 lists `pub mod runs;`.)

- [ ] **Step 3: (implementation already written in Step 1)**

This task's implementation and test are in the same file. Proceed to verify.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd src-tauri && cargo test --lib engine::runs`
Expected: PASS (1 test).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ai/engine/runs.rs
git commit -m "feat(engine): run ledger repository (create/status/get)"
```

---

### Task 4: Price math (pure, exact)

**Files:**
- Modify: `src-tauri/src/ai/engine/mod.rs` (add `apply_price` + tests)

- [ ] **Step 1: Write the failing test**

Append to `src-tauri/src/ai/engine/mod.rs`:

```rust
/// Apply a price operation to a current fils value. Result is clamped to >= 0.
/// Percent uses f64 internally then rounds to the nearest fils — the ONLY place
/// a float touches money, and the result is immediately an integer.
pub fn apply_price(current_minor: i64, op: &PriceOp) -> i64 {
    let result = match op {
        PriceOp::Percent(p) => (current_minor as f64 * (1.0 + p / 100.0)).round() as i64,
        PriceOp::Absolute(d) => current_minor + d,
        PriceOp::Set(v) => *v,
    };
    result.max(0)
}

#[cfg(test)]
mod price_tests {
    use super::*;

    #[test]
    fn percent_exact_fils() {
        assert_eq!(apply_price(12500, &PriceOp::Percent(20.0)), 15000); // 12.500 → 15.000
        assert_eq!(apply_price(4250, &PriceOp::Percent(20.0)), 5100);   // 4.250 → 5.100
    }

    #[test]
    fn percent_rounds_to_nearest_fils() {
        assert_eq!(apply_price(333, &PriceOp::Percent(20.0)), 400); // 399.6 → 400
    }

    #[test]
    fn absolute_and_set() {
        assert_eq!(apply_price(1000, &PriceOp::Absolute(-250)), 750);
        assert_eq!(apply_price(1000, &PriceOp::Set(99)), 99);
    }

    #[test]
    fn never_negative() {
        assert_eq!(apply_price(100, &PriceOp::Absolute(-500)), 0);
    }
}
```

- [ ] **Step 2: Run to verify it fails, then passes**

Run: `cd src-tauri && cargo test --lib engine::price_tests`
Expected: after writing (the impl is in the same step), PASS (4 tests). If you split impl out first, the failing run shows `apply_price not found`.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/ai/engine/mod.rs
git commit -m "feat(engine): exact fils price math (percent/absolute/set, clamped)"
```

---

### Task 5: Batched price-adjust executor

**Files:**
- Create: `src-tauri/src/ai/engine/batch.rs`

This is the heart: keyset cursor over selector matches, per-batch transaction, reverse-snapshot to the undo log, checkpoint after each batch.

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/ai/engine/batch.rs`:

```rust
use crate::ai::engine::selector::Selector;
use crate::ai::engine::{runs, PriceOp, apply_price};
use crate::errors::AppResult;
use sqlx::SqlitePool;

/// One product's pre-change price, for undo.
#[derive(serde::Serialize, serde::Deserialize)]
struct Reverse { product_id: String, prev_price_minor: i64 }

/// Execute a price adjustment across every product matching `selector`, in
/// keyset batches. Each batch is one transaction: it rewrites prices and writes
/// a reverse-snapshot, then advances the run checkpoint. Returns rows changed.
pub async fn execute_price_adjust(
    pool: &SqlitePool,
    run_id: &str,
    selector: &Selector,
    op: &PriceOp,
    batch_size: i64,
) -> AppResult<i64> {
    runs::set_status(pool, run_id, "executing").await?;
    let c = selector.compile();
    let mut cursor = String::new(); // product_id > '' matches all (TEXT keyset)
    let mut batch_seq: i64 = 0;
    let mut total_changed: i64 = 0;

    loop {
        // Fetch the next batch of (product_id, current selling price) past the cursor.
        let sql = format!(
            "{} SELECT p.product_id, pp.price_minor FROM products p \
             JOIN product_prices pp ON pp.product_id = p.product_id \
               AND pp.price_type='selling' AND pp.effective_to IS NULL \
             WHERE {} AND p.product_id > ? ORDER BY p.product_id LIMIT ?",
            c.cte, c.where_sql
        );
        let mut q = sqlx::query_as::<_, (String, i64)>(&sql);
        for b in &c.binds { q = q.bind(b); }
        q = q.bind(&cursor).bind(batch_size);
        let rows: Vec<(String, i64)> = q.fetch_all(pool).await?;
        if rows.is_empty() { break; }

        let now = chrono::Utc::now().to_rfc3339();
        let mut tx = pool.begin().await?;
        let mut reverses: Vec<Reverse> = Vec::with_capacity(rows.len());

        for (pid, current) in &rows {
            let new_price = apply_price(*current, op);
            // Close the open selling-price row.
            sqlx::query("UPDATE product_prices SET effective_to=?, sync_status='pending' \
                         WHERE product_id=? AND price_type='selling' AND effective_to IS NULL")
                .bind(&now).bind(pid).execute(&mut *tx).await?;
            // Insert the new open row, attributed to this run.
            let price_id = ulid::Ulid::new().to_string();
            sqlx::query("INSERT INTO product_prices \
                         (price_id,product_id,branch_id,price_type,price_minor,currency,effective_from,created_by_user_id,created_by_ai_action_id,created_at) \
                         VALUES (?,?,NULL,'selling',?,'BHD',?,?,?,?)")
                .bind(&price_id).bind(pid).bind(new_price).bind(&now)
                .bind("AI_ADMIN").bind(run_id).bind(&now).execute(&mut *tx).await?;
            reverses.push(Reverse { product_id: pid.clone(), prev_price_minor: *current });
        }

        // Reverse-snapshot for this batch.
        let entry_id = ulid::Ulid::new().to_string();
        let reverse_json = serde_json::to_string(&reverses).unwrap_or_else(|_| "[]".into());
        sqlx::query("INSERT INTO ai_run_undo_log (entry_id,run_id,batch_seq,reverse_json,applied,created_at) VALUES (?,?,?,?,0,?)")
            .bind(&entry_id).bind(run_id).bind(batch_seq).bind(&reverse_json).bind(&now)
            .execute(&mut *tx).await?;

        // Advance the checkpoint on the run row.
        let last_pid = rows.last().map(|r| r.0.clone()).unwrap();
        total_changed += rows.len() as i64;
        sqlx::query("UPDATE ai_runs SET done_count=?, checkpoint_cursor=?, updated_at=? WHERE run_id=?")
            .bind(total_changed).bind(&last_pid).bind(&now).bind(run_id).execute(&mut *tx).await?;

        tx.commit().await?;
        cursor = last_pid;
        batch_seq += 1;
    }

    runs::set_status(pool, run_id, "done").await?;
    Ok(total_changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup() -> SqlitePool {
        let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let now = "2026-01-01T00:00:00Z";
        for (id, parent) in [("toys", None), ("girls", Some("toys")), ("boys", Some("toys"))] {
            sqlx::query("INSERT INTO categories (category_id,parent_category_id,name,sort_order,is_active,created_at,updated_at) VALUES (?,?,?,0,1,?,?)")
                .bind(id).bind(parent).bind(id).bind(now).bind(now).execute(&pool).await.unwrap();
        }
        // 250 girls products @ 12.500, 1 boys product @ 12.500 (must NOT change)
        for i in 0..250 {
            let pid = format!("g{:04}", i);
            seed_product(&pool, &pid, "girls", 12500, now).await;
        }
        seed_product(&pool, "b0001", "boys", 12500, now).await;
        pool
    }

    async fn seed_product(pool: &SqlitePool, pid: &str, cat: &str, price: i64, now: &str) {
        sqlx::query("INSERT INTO products (product_id,category_id,name,is_active,currency,reorder_point,created_at,updated_at) VALUES (?,?,?,1,'BHD',0,?,?)")
            .bind(pid).bind(cat).bind(pid).bind(now).bind(now).execute(pool).await.unwrap();
        let price_id = format!("pr_{}", pid);
        sqlx::query("INSERT INTO product_prices (price_id,product_id,branch_id,price_type,price_minor,currency,effective_from,created_by_user_id,created_at) VALUES (?,?,NULL,'selling',?,'BHD',?,'seed',?)")
            .bind(&price_id).bind(pid).bind(price).bind(now).bind(now).execute(pool).await.unwrap();
    }

    async fn current_price(pool: &SqlitePool, pid: &str) -> i64 {
        sqlx::query_scalar::<_, i64>("SELECT price_minor FROM product_prices WHERE product_id=? AND price_type='selling' AND effective_to IS NULL")
            .bind(pid).fetch_one(pool).await.unwrap()
    }

    #[tokio::test]
    async fn bulk_adjust_changes_only_matched_with_exact_math() {
        let pool = setup().await;
        let sel = Selector { category_subtree: Some("girls".into()), ..Default::default() };
        let total = sel.count(&pool).await.unwrap();
        let run_id = runs::create_run(&pool, "bulk.price_adjust", "{}", "{}", total, "U1").await.unwrap();

        let changed = execute_price_adjust(&pool, &run_id, &sel, &PriceOp::Percent(20.0), 100).await.unwrap();

        assert_eq!(changed, 250);
        assert_eq!(current_price(&pool, "g0000").await, 15000); // 12.500 → 15.000
        assert_eq!(current_price(&pool, "g0249").await, 15000);
        assert_eq!(current_price(&pool, "b0001").await, 12500); // boys untouched
        assert_eq!(runs::get_run(&pool, &run_id).await.unwrap().status, "done");
        assert_eq!(runs::get_run(&pool, &run_id).await.unwrap().done_count, 250);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd src-tauri && cargo test --lib engine::batch`
Expected: FAIL — `execute_price_adjust not found` until implemented (impl is in the same Step 1 block; if you wrote test-first separately, this is where it fails).

- [ ] **Step 3: Confirm implementation present**

The executor in Step 1 is the implementation. Ensure `mod.rs` lists `pub mod batch;` (from Task 1).

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd src-tauri && cargo test --lib engine::batch`
Expected: PASS (1 test) — 250 rows changed across 3 batches (100/100/50), boys product unchanged.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ai/engine/batch.rs
git commit -m "feat(engine): checkpointed batched price-adjust executor"
```

---

### Task 6: Whole-run undo

**Files:**
- Modify: `src-tauri/src/ai/engine/batch.rs` (add `undo_run` + test)

- [ ] **Step 1: Write the failing test**

Add to `src-tauri/src/ai/engine/batch.rs` (before the `#[cfg(test)]` block, add the function; add the test inside the existing `mod tests`):

Function:

```rust
/// Reverse a completed run: replay every un-applied undo-log entry in reverse
/// batch order, restoring each product's prior selling price. Idempotent per entry.
pub async fn undo_run(pool: &SqlitePool, run_id: &str) -> AppResult<i64> {
    let entries: Vec<(String, String)> = sqlx::query_as(
        "SELECT entry_id, reverse_json FROM ai_run_undo_log WHERE run_id=? AND applied=0 ORDER BY batch_seq DESC",
    )
    .bind(run_id).fetch_all(pool).await?;

    let now = chrono::Utc::now().to_rfc3339();
    let mut restored = 0i64;
    let mut tx = pool.begin().await?;
    for (entry_id, reverse_json) in &entries {
        let reverses: Vec<Reverse> = serde_json::from_str(reverse_json).unwrap_or_default();
        for r in &reverses {
            sqlx::query("UPDATE product_prices SET effective_to=?, sync_status='pending' \
                         WHERE product_id=? AND price_type='selling' AND effective_to IS NULL")
                .bind(&now).bind(&r.product_id).execute(&mut *tx).await?;
            let price_id = ulid::Ulid::new().to_string();
            sqlx::query("INSERT INTO product_prices \
                         (price_id,product_id,branch_id,price_type,price_minor,currency,effective_from,created_by_user_id,created_by_ai_action_id,created_at) \
                         VALUES (?,?,NULL,'selling',?,'BHD',?,?,?,?)")
                .bind(&price_id).bind(&r.product_id).bind(r.prev_price_minor).bind(&now)
                .bind("AI_ADMIN").bind(run_id).bind(&now).execute(&mut *tx).await?;
            restored += 1;
        }
        sqlx::query("UPDATE ai_run_undo_log SET applied=1 WHERE entry_id=?")
            .bind(entry_id).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE ai_runs SET status='undone', updated_at=? WHERE run_id=?")
        .bind(&now).bind(run_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(restored)
}
```

Also derive `Default` on `Reverse` so `unwrap_or_default()` compiles — change its derive line to:

```rust
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Reverse { product_id: String, prev_price_minor: i64 }
```

Test (add inside `mod tests`):

```rust
    #[tokio::test]
    async fn undo_restores_every_price() {
        let pool = setup().await;
        let sel = Selector { category_subtree: Some("girls".into()), ..Default::default() };
        let total = sel.count(&pool).await.unwrap();
        let run_id = runs::create_run(&pool, "bulk.price_adjust", "{}", "{}", total, "U1").await.unwrap();
        execute_price_adjust(&pool, &run_id, &sel, &PriceOp::Percent(20.0), 100).await.unwrap();
        assert_eq!(current_price(&pool, "g0000").await, 15000);

        let restored = undo_run(&pool, &run_id).await.unwrap();

        assert_eq!(restored, 250);
        assert_eq!(current_price(&pool, "g0000").await, 12500); // back to 12.500
        assert_eq!(current_price(&pool, "g0249").await, 12500);
        assert_eq!(runs::get_run(&pool, &run_id).await.unwrap().status, "undone");
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd src-tauri && cargo test --lib engine::batch::tests::undo_restores_every_price`
Expected: FAIL — `undo_run not found`.

- [ ] **Step 3: (implementation written in Step 1)**

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd src-tauri && cargo test --lib engine::batch`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ai/engine/batch.rs
git commit -m "feat(engine): whole-run undo restores prior prices"
```

---

### Task 7: Full-suite green + clippy + fmt

**Files:** none (verification + cleanup only)

- [ ] **Step 1: Run the whole engine suite**

Run: `cd src-tauri && cargo test --lib engine`
Expected: PASS — selector (2), runs (1), price (4), batch (2) = 9 tests.

- [ ] **Step 2: Format + lint**

Run: `cd src-tauri && cargo fmt --all && cargo clippy --all-targets -- -D warnings`
Expected: no warnings. Fix any clippy findings (likely `needless_borrow` or `format!` lints) inline, keeping behavior identical.

- [ ] **Step 3: Confirm nothing else broke**

Run: `cd src-tauri && cargo test --lib`
Expected: the full lib suite passes (engine tests plus all pre-existing tests).

- [ ] **Step 4: Commit**

```bash
git add -A src-tauri/src
git commit -m "chore(engine): fmt + clippy clean for engine core"
```

---

## Self-Review

**1. Spec coverage (Phase A engine spine):**
- Operation Registry trait — *deferred to A2 by design* (A1 ships a concrete `execute_price_adjust`; A2 generalizes it into the `Operation` trait). Noted in spec/plan; not a gap.
- Selector + server-side filter resolution — Task 2 (subtree CTE, active, text). price_range/stock/not_sold_since explicitly deferred (later phases) — they need price/stock joins; the toys demo doesn't require them.
- Batched, transactional, checkpointed execution — Task 5.
- Exact fils math in one place — Task 4.
- Run ledger + whole-run undo — Tasks 3 & 6.
- Preview (count + sample) — `count()` in Task 2; sample-rows query is part of A2's preview surfacing (the orchestrator builds the human preview). A1 proves count; sample is a trivial extension. Acceptable boundary.

**2. Placeholder scan:** No TBD/TODO; every step has complete, runnable code and exact commands.

**3. Type consistency:** `Selector`, `Compiled`, `PriceOp`, `apply_price`, `runs::{create_run,set_status,set_failed,get_run,Run}`, `execute_price_adjust`, `undo_run`, `Reverse` are used with identical signatures across tasks. Module paths (`crate::ai::engine::…`) consistent with the `mod.rs`/`ai/mod.rs` wiring in Task 1.

## Out of scope (this plan)

- The `Operation` trait + registry + the other 3 flagship ops (`product.set_price`, `product.create`, `product.set_active`) — **Plan A2**, which also refactors `execute_price_adjust` to call through the trait.
- LLM/orchestrator wiring, new `StreamEvent` variants, Tauri commands — **Plan A2**.
- RunPanel UI, assistant landing, command bar — **Plan A3**.
- Resume-after-crash using `checkpoint_cursor` (the column is written every batch; the resume entry point is A2 when the orchestrator owns run lifecycle).
