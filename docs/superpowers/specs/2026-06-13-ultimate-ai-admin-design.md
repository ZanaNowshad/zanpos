# Ultimate AI Admin (ZanAI) — Design Spec

**Date:** 2026-06-13
**Status:** Approved design → ready for implementation planning
**Branch base:** `feature/officeai-merge` (the OfficeAI workspace this builds on)

## Context

ZANPOS (Tauri 2 + React 19 + Rust, `C:\Users\super\ZAN\zanpos`) has an AI admin (ZanAI, formerly AMWAJ) wired as a copilot inside the new OfficeAI workspace. Its agent uses a free-form tool loop (`MAX_TURNS=8`) over ~115 tools in `src-tauri/src/ai/tools.rs`. This architecture fails the owner's real workload in four ways, all confirmed in code:

1. **Bulk operations break.** `bulk_update_prices` (`tools.rs:4905`) takes a literal `updates: [{product_id, price_minor}]` array. To do "increase all girls' toys by 20%" the model must load every matching product into context (to get IDs + current prices), compute each new price itself (arithmetic errors), and emit a 300+ element array in one shot (context overflow). Its undo is `_no_undo`.
2. **No transactional safety for batches.** Most mutations run statement-by-statement on `pool` (only `adjust_stock`/`stock_take` use `pool.begin()`/`tx.commit()`, `tools.rs:4442–4498`). A half-finished bulk op corrupts data.
3. **Loose tools invite mistakes.** Generic multi-field mutations let the model guess fields and values; nothing validates intent before it hits the DB.
4. **AI is bolted on, not central.** The admin navigates tabs and forms; the AI is a side panel.

This spec defines the upgrade: an **AI-first admin panel** where intent is the front door and a **deterministic engine** does all exact work. The model declares typed intent; Rust resolves filters, does fils-exact math, executes in checkpointed transactional batches, and records a reversible snapshot. Mistakes become structurally impossible; bulk scales to thousands of rows without touching the context window.

## Locked decisions

- **Re-centering:** AI-first landing + omnipresent command bar on every tab; the 12 data tabs remain as the inspection/trust rail (never removed — nothing becomes unreachable).
- **Provider auth:** Defensible default (API keys + OpenAI-compatible presets: Groq, OpenRouter, DeepSeek, Ollama + legitimate Google OAuth), PLUS a separate "Advanced · at your own risk" section adding PKCE ChatGPT/Claude subscription sign-in behind an explicit ToS-risk warning. Subscription OAuth is never the default path.
- **Safety:** Every mutation is preview-then-confirm. Nothing touches data without the human approving the diff. Bulk and multi-step tasks are fully undoable as a unit.
- **Sequencing:** The deterministic engine (Phase A) is built first; experience layers stack on top. This is non-negotiable — skipping it is the trap the prior attempt fell into.

## Architecture — six layers

### Layer 1 — Operation Registry (the "fixed tools, no choice" core)

Replace loose tools with a registry of typed `Operation`s. Rust trait (new module `src-tauri/src/ai/ops/mod.rs`):

```rust
#[async_trait]
pub trait Operation: Send + Sync {
    fn id(&self) -> &'static str;                 // "product.set_price", "bulk.price_adjust"
    fn kind(&self) -> OpKind;                      // Row | Bulk | Read
    fn schema(&self) -> serde_json::Value;         // JSON Schema for typed slots (fed to the model)
    /// Validate slots against types AND the live DB. Returns field-level errors the
    /// model must fix. An invalid input can never proceed to preview/commit.
    async fn validate(&self, db: &SqlitePool, input: &Value) -> Result<Validated, Vec<FieldError>>;
    /// Build a human-facing preview: description, count (bulk), sample diff rows.
    async fn preview(&self, db: &SqlitePool, v: &Validated) -> Result<Preview, AppError>;
    /// Execute inside a transaction. For bulk, this is called per-batch by the engine.
    async fn commit(&self, tx: &mut Transaction<'_, Sqlite>, v: &Validated, batch: Option<&Batch>) -> Result<CommitResult, AppError>;
}
```

- **Row ops** act on one entity: `product.create`, `product.set_price`, `product.rename`, `product.set_active`, `customer.create`, `customer.update`, `category.create`, etc. Typed slots validated against the DB (category exists, barcode unique, price > 0, fils precision).
- **Bulk ops** act on a declarative `Selector`, never IDs: `bulk.price_adjust { selector, mode: Percent|Absolute|Set, value }`, `bulk.set_active`, `bulk.move_category`, `bulk.set_reorder_point`.

The model's whole job: choose an operation id and fill its slots. `validate()` is the gate — a `FieldError` list goes back to the model for bounded self-correction (max 2 retries) and never reaches `commit()`.

### Layer 2 — Selector + Batch Engine (the "chunking" core)

New module `src-tauri/src/ai/engine.rs`. A `Selector` is a typed filter that compiles to a parameterized SQL `WHERE`:

```rust
pub struct Selector {
    pub entity: Entity,                 // Product | Customer | ...
    pub category_subtree: Option<String>,   // resolves the category tree (Toys › Girls and descendants)
    pub tag: Option<String>,
    pub price_range: Option<(i64, i64)>,
    pub stock: Option<StockFilter>,         // low | out | below_reorder
    pub not_sold_since: Option<NaiveDate>,
    pub active: Option<bool>,
    pub text: Option<String>,
}
```

Engine API:
- `resolve_count(&Selector) -> i64` — cheap `SELECT COUNT(*)`.
- `resolve_sample(&Selector, n) -> Vec<Row>` — n rows for the preview diff.
- `execute_batched(run_id, op, &Selector, batch_size=100)` — cursor over matching primary keys (keyset pagination, `WHERE id > :last ORDER BY id LIMIT 100`), each batch in its own transaction, writes a checkpoint + reverse-snapshot after each, emits `RunProgress`. The full match set is never materialized in memory or context.

Fils-exact arithmetic (×1.20, round to 3 dp, currency from `DEVICE.currency_exponent`) lives here, in one place, tested.

### Layer 3 — Run ledger (durable tasks + universal undo)

New SQLite tables (migration in `src-tauri/migrations/`):

- `runs` — `run_id, plan_json, status (planning|previewing|awaiting_confirm|executing|paused|done|failed|undone), op_id, selector_json, total_count, done_count, checkpoint_cursor, created_by, created_at, updated_at`.
- `run_steps` — for multi-op plans: `step_id, run_id, seq, op_id, input_json, status, result_json`.
- `run_undo_log` — `entry_id, run_id, batch_seq, reverse_snapshot_json, applied (bool)`.

Capabilities: resume after crash/close (checkpoint cursor), **undo-whole-run** (replay `run_undo_log` in reverse inside a transaction), live progress. Replaces the per-action `undo_records` for bulk; row ops keep a single-entry run. Runs outlive the chat session.

### Layer 4 — Agent orchestrator (planner, not free-for-all)

Refactor `src-tauri/src/commands/ai_admin_commands.rs` + `ai/streaming.rs`. The model is given **read tools + the operation registry schemas** as its toolset. Flow shifts from "8 free turns" to plan-and-execute:

1. Model emits a **Plan**: an ordered list of operation invocations (commonly one bulk op).
2. Each read/query step may chain (bounded), returning summaries/counts — never giant payloads.
3. Each mutating step → `validate()` → `preview()` → surfaced to human as a Run awaiting confirm → on confirm, engine executes the Run.
4. Anthropic streaming preserved. New stream events ride the existing `Channel<StreamEvent>` (extend `domain/ai_admin.rs`): `RunPreview { run_id, op_id, description, count, sample }`, `RunProgress { run_id, done, total }`, `RunDone { run_id }`, `RunFailed { run_id, at, error }`. The existing `Navigate` event (open_tab) stays.

### Layer 5 — Provider auth

Extend `src-tauri/src/ai/provider.rs` + the provider setup UI (`src/officeai/ProviderSetup.tsx`).
- Keep keyring (`secure_store.rs`) + API keys + existing OpenAI-compatible path.
- Add one-tap base-URL presets: Groq, OpenRouter, DeepSeek, local Ollama.
- Add Google OAuth (legitimate consumer OAuth) — PKCE, tokens in keyring with refresh.
- Add "Advanced · at your own risk" section: ChatGPT + Claude subscription PKCE OAuth, gated behind an explicit warning that provider ToS enforcement can block it. Tokens in keyring.

### Layer 6 — AI-first frontend (the experience)

Builds on the OfficeAI workspace already in `src/officeai/`.
- OfficeAI opens on the **Assistant landing** (default tab = `assistant`, not `products`).
- **Omnipresent command bar** atop every data tab — one component that routes intent to the orchestrator (text / pasted data / question).
- **RunPanel** — grow `src/components/ConfirmActionModal.tsx` into a richer panel: plan list, preview diff table, live progress bar, pause / undo-whole-run controls. Driven by the new Run stream events in `useChatController.ts`.
- **Generated views** (Phase C) — intent assembles a scoped, actionable table whose row actions route back through operations.
- **Proactive "Noticed for you" cards** (Phase D) — a background scan surfaces situations (below-cost prices, imminent stockouts, dead stock, anomalies) as one-click reversible actions.

## Build phases (each ships working)

- **Phase A — Engine spine.** `ops/` registry + `engine.rs` (Selector + Batch) + Run ledger migration + 4 flagship ops (`product.set_price`, `bulk.price_adjust`, `product.create`, `product.set_active`). Wire orchestrator + RunPanel. **Acceptance: "increase price of all Toys › Girls by 20%" runs end-to-end — preview 312 + sample diff, confirm, checkpointed batches, live progress, undo-whole-run restores every price.**
- **Phase B — CRUD breadth.** Migrate the remaining ~52 mutations from `tools.rs` into typed operations; add bulk variants that matter (`bulk.move_category`, `bulk.set_reorder_point`, `bulk.set_active`, bulk tagging). Read tools stay as-is (they already return bounded summaries).
- **Phase C — AI-first shell.** Assistant landing + omnipresent command bar + generated views.
- **Phase D — Proactive layer.** Background situation scan → reversible cards.
- **Phase E — Provider auth.** Presets + Google OAuth + risk-gated subscription OAuth.

Engine-first (A) is required before any other phase. B–E may reorder by appetite.

## Error handling

- **Validation errors** → structured `FieldError[]` returned to the model for bounded self-correction; never reach `commit()`.
- **Mid-run batch failure** → current batch's transaction rolls back; Run marked `failed` at checkpoint N; batches already committed remain undoable via `run_undo_log`; UI shows exactly where it stopped with resume / undo options.
- **Provider / stream errors** → existing handling, surfaced on the Run rather than as a bare chat error.
- **Selector resolves to 0 rows** → preview says so; no Run created.

## Testing

- **Per-operation Rust unit tests:** `validate()` rejects bad input with the right `FieldError`; `preview()` math exact across fils-rounding edge cases (e.g. 4.250 ×1.2 = 5.100, half-fils rounding rule); `commit()` runs in a transaction; undo restores prior state.
- **Selector → SQL tests:** category subtree resolution, empty match, combined filters.
- **Batch engine integration test:** 1000-row synthetic product set; full run correctness; crash mid-run → resume from checkpoint; undo-whole-run restores every row.
- **Frontend:** the 3 existing vitest files stay green; add `useChatController` tests for the new Run events (preview → confirm → progress → done; undo path).
- **Verification gate:** `npm run check` + `cargo fmt --check && cargo clippy --all-targets -D warnings && cargo test --lib`.

## Key files

- New: `src-tauri/src/ai/ops/` (registry + per-entity operation modules), `src-tauri/src/ai/engine.rs` (Selector + Batch), `src-tauri/migrations/<n>_run_ledger.sql`.
- Modified: `src-tauri/src/ai/tools.rs` (read tools stay; mutations migrate out over phases), `src-tauri/src/commands/ai_admin_commands.rs` (planner), `src-tauri/src/ai/streaming.rs` (Run events), `src-tauri/src/domain/ai_admin.rs` (event enum), `src-tauri/src/ai/provider.rs` (auth).
- Frontend: `src/officeai/OfficeAIPage.tsx` (assistant landing + command bar), `src/officeai/useChatController.ts` (Run events), `src/components/ConfirmActionModal.tsx` → RunPanel, `src/officeai/ProviderSetup.tsx` (auth expansion), `src/types.ts` (event mirror).

## Out of scope (YAGNI for v1)

- Multi-store / cross-branch bulk ops (single active branch only, per current DB).
- Scheduled/recurring Runs.
- Voice input.
- Fine-grained per-operation RBAC beyond the existing owner/manager gate (revisit after Phase B).
