# AI Unified Action Layer — Design Spec

**Date:** 2026-06-19
**Status:** Proposed
**Roadmap item:** 1.1 — Unify tools + intents into one coherent action layer
**Scope:** `src-tauri/src/ai/*` + `src-tauri/src/commands/ai_admin_commands.rs`
**Owner:** spec agent (design only; no code in this document)

---

## 1. Context & Problem

ZanAI ("OfficeAI"/AI Admin) currently exposes **three overlapping capability surfaces** to the model, and they have drifted out of sync. The model is told one vocabulary in the system prompt, handed a different vocabulary as tool definitions, and routed through a third deterministic engine — and nothing keeps the three aligned. The result is that the model regularly calls the wrong thing, calls a deprecated thing, or calls a thing the prompt never told it about.

### 1.1 The three surfaces

1. **Low-level tools** — `src-tauri/src/ai/tools_catalogue.rs::all_tool_definitions()` returns **228 `ToolDef`s** (verified count of `ToolDef {` literals). `tools.rs::all_tool_definitions()` (line 34) is a thin re-export of the catalogue, and `tools.rs::filtered_tool_definitions()` (line 254) is what is actually handed to the model in every entry point — `ai_admin_commands.rs::ai_chat` line 502, `ai_chat_stream` line ~1110. After feature-toggle filtering the model still sees on the order of ~200 tools. Mutations are gated by a **hand-maintained allowlist** `tools.rs::MUTATION_TOOLS` (lines 42–137) whose own docstring warns: *"Omitting a mutation tool from this list makes it silently execute without confirmation."*

2. **High-level intents** — `src-tauri/src/ai/intent_engine.rs` exposes ~20 business "intents" via `all_intents()` (lines 65–88). But the intent surface is **internally inconsistent**:
   - `INTENT_NAMES` (lines 45–64) lists **18** names.
   - `all_intents()` returns **20** `IntentDef`s — it adds `adjust_prices_batch` and `bulk_price_adjust`, both of which are *not* in `INTENT_NAMES`, so they are advertised by schema generation but unreachable through the `INTENT_NAMES.contains(...)` dispatch guard.
   - `execute_intent()` (lines 116–150) handles ~16 names and **hard-rejects** `adjust_prices_batch` with a "consolidated, use the engine op instead" error (lines 123–125).
   - `is_mutation_intent()` (lines 32–42) names only **6**.
   - The intent surface is only ever consulted inside the streaming dispatch (`streaming.rs` lines 380, 731) — the **non-streaming** `run_tool_loop` (`ai_admin_commands.rs` lines 915–1070) never checks `INTENT_NAMES` at all, so intents are silently unavailable on the blocking path.

3. **Deterministic bulk Registry** — `src-tauri/src/ai/engine/ops.rs` defines the `Operation` trait (lines 9–33) and a `Registry` (lines 50–65), with **7 concrete ops** registered: `BulkPriceAdjust`, `BulkProductArchive`, `BulkPromotionApply`, `BulkPromotionRemove`, `BulkReorderPointUpdate`, `BulkStockVarianceFix`, `BulkSupplierPriceSync`. These follow the run/preview/confirm/undo lifecycle (`engine/runs.rs`, `engine/batch.rs`). This is the **cleanest abstraction in the codebase** and is the model this spec generalises from.

### 1.2 The drift, concretely

The system prompt (`ai_admin_commands.rs::build_system_prompt`, lines 773–896) tells the model, verbatim: *"You are given **intents** — high-level business actions"* (line 795) and lists an **"Available Intents"** block (lines 865–891) of ~19 actions. That contract is false in three ways:

- **Wrong vocabulary handed over.** The model is told it has intents, but `filtered_tool_definitions()` hands it ~200 low-level tools. The intent schemas are never even merged into the tool list passed to the provider — the prompt advertises a menu the model was not given.
- **Advertised actions that don't exist as advertised.** The prompt's intent list names `update_product` (line 882) as the path for the WhatsApp price-update workflow (lines 861–862). But `update_product` is **not** in `MUTATION_TOOLS`, **not** in `is_readonly_core`, and the live catalogue implements `update_product_full` / `update_product_name` / `update_product_price` instead. `tools.rs` lines 154–156 document this exact gap: *"`update_product` is advertised in the system prompt even though the live catalogue uses `update_product_full`/`update_product_name`."* When the model obeys the prompt and calls `update_product`, dispatch falls through all four branches and the call dead-ends.
- **One name, two incompatible execution models.** `bulk_price_adjust` is listed under **"Mutation (confirm before running)"** intents (line 883), implying the standard `MutationPending` → `ai_execute_action` confirm flow. But it is actually an **engine op** (`streaming.rs::ENGINE_OPS` line 306) that takes the *completely different* `RunPreview` → `ai_run_execute` path. The model cannot tell from the prompt which confirmation UX it will trigger.

Additional internal drift that compounds unreliability:

- **Duplicated dispatch.** The 4-way dispatch ladder (engine op → intent → mutation tool → read tool) is **copy-pasted** between the Anthropic path (`streaming.rs` ~300–553) and the OpenAI path (`streaming.rs` ~640–900), each ~250 lines. The `Registry` is **constructed and all 7 ops re-registered twice** (lines 321–328 and 658–665). `ENGINE_OPS` is duplicated (lines 305–313, 641–649) and its comment says *"All 6 engine ops"* while listing 7.
- **The Registry is under-used at runtime.** Even though 7 ops can each `create_run`, `ai_run_execute` (`ai_admin_commands.rs` lines 693–753) only knows how to replay a `PriceOp` via `batch::execute_price_adjust` (line 723). A run created by `bulk.product_archive` or `bulk.promotion_apply` is previewed but its executor path is price-specific — the deterministic engine's preview and commit halves are not symmetric across ops.
- **Three mutation-classification lists** must agree but are maintained separately: `MUTATION_TOOLS` (tools.rs), `is_mutation_intent` (intent_engine.rs), and `mutation_risk`/`LOW_RISK_MUTATIONS` (tools.rs lines 157–203).

**Net effect:** the model's reliability depends on which of three desynchronised lists it happens to hit, and the prompt actively points it at names that fail. This is a correctness problem, not a polish problem.

---

## 2. Proposed Design — One Action Layer

Introduce a single **Action** abstraction that is the model's *entire* public contract. Intents become the **public vocabulary** (~25–30 named actions, exactly mirrored in the system prompt); low-level tools become **private implementation details** that Actions call internally and that are **no longer handed to the model**. The design generalises the existing `engine/ops.rs` `Operation` trait so that *every* action — read, single mutation, and bulk — flows through one registry and one dispatch path.

### 2.1 The `Action` trait

Generalise `Operation` (ops.rs lines 9–33) into an `Action` trait that covers reads and single mutations, not just batched bulk ops. Sketch (Rust-shaped pseudocode, not final code):

```
pub enum ActionKind { Read, Mutation, BulkRun }

pub trait Action: Send + Sync {
    fn name(&self) -> &'static str;          // public, model-visible (e.g. "update_product")
    fn description(&self) -> &'static str;   // single source for the system-prompt line
    fn schema(&self) -> Value;               // JSON Schema handed to the provider
    fn kind(&self) -> ActionKind;
    fn risk(&self, input: &Value) -> Risk { Risk::High } // reuse tools::Risk

    // Reads + single mutations: validate then run, returning structured data.
    fn run<'a>(&'a self, ctx: &'a ActionCtx, input: &'a Value)
        -> Pin<Box<dyn Future<Output = AppResult<ActionResult>> + Send + 'a>>;

    // BulkRun actions ALSO implement the existing preview/commit_batch pair
    // (unchanged from Operation) so the run/checkpoint/undo machinery is reused.
}
```

Key points:

- `Action::name()` / `description()` / `schema()` are the **single source of truth**. `build_system_prompt` is generated by iterating the registry instead of being a hand-written markdown block — this structurally eliminates prompt-vs-dispatch drift, because the prompt and the tool list are emitted from the same `Vec<&dyn Action>`.
- Existing `engine/ops.rs::Operation` is **a subset** of `Action` (the `BulkRun` kind). Bulk ops keep `preview` + `commit_batch` verbatim; we only add the thin `name/description/schema/kind/risk` metadata most of them already have as `id()`/`schema()`.
- An Action's `run()` **delegates to existing tool functions** — `tools::execute_read_tool`, `tools::execute_mutation`, `intent_engine` helpers. No business logic is rewritten in phase 1; Actions are a façade.

### 2.2 Intents-as-contract, tools-as-implementation

- The **~20 intents become Actions directly** (they already have `name`/`description`/`parameters` in `IntentDef`, lines 24–29 — a near-perfect fit). Their `run()` calls the existing `execute_intent` body.
- **High-value mutations the prompt advertises but that only exist as tools** (the `update_product` gap, lines 154–156) become Actions whose `run()` maps to the real tool (`update_product_full`/`update_product_price`) and reshapes parameters. This **closes the dead-end** without the model ever seeing the low-level name.
- The **7 engine ops become `BulkRun` Actions**, surfaced under their public names.
- The remaining ~200 catalogue tools are **demoted to private**: they stay in `tools_catalogue.rs` and stay callable by Action `run()` bodies, but are **removed from what `filtered_tool_definitions` passes to the model**. The model never chooses among 200 tools again.

Target surface: **~25–30 Actions**, each appearing exactly once in (a) the registry, (b) the provider tool list, and (c) the generated system prompt.

### 2.3 The registry as the one dispatch table

Replace the duplicated 4-way ladders and the twice-built `Registry` with **one `ActionRegistry` built once** and shared by both provider paths:

```
pub fn build_action_registry() -> ActionRegistry { /* register all Actions once */ }

// Single dispatch entry, called by BOTH Anthropic and OpenAI loops:
async fn dispatch_action(ctx, registry, name, input, on_event) -> DispatchOutcome
```

`dispatch_action` collapses `streaming.rs` lines ~300–553 and ~640–900 into one function. The `ENGINE_OPS` const, the duplicate `Registry::new()` blocks, and the `INTENT_NAMES.contains` guards all disappear — membership and routing are answered by `registry.find(name).kind()`.

---

## 3. How the cross-cutting concerns map onto Actions

### 3.1 Dispatch

`dispatch_action` branches on `Action::kind()` (and `risk()` for mutations):

| `kind()` | risk | Path | Existing analogue |
|----------|------|------|-------------------|
| `Read` | — | `run()` → append `ToolResult`, continue loop | `streaming.rs` read-tool branch (lines 496–545) |
| `Mutation` | `Low` | `run()` immediately via `auto_apply_mutation`, emit `MutationApplied`, continue | `streaming.rs` lines 470–496 |
| `Mutation` | `High` | build action row, emit `MutationPending`, return | `streaming.rs` lines 466–494 |
| `BulkRun` | (always High) | `validate` → `preview` → `create_run` → emit `RunPreview`, return | `streaming.rs` lines 314–377 |

This is exactly the routing that exists today — the change is that it lives in **one** function keyed off trait metadata instead of three desynchronised name lists duplicated across two providers.

### 3.2 Dry-run / preview

Two preview shapes exist and **both are preserved**:

- **Single mutations** preview via `tools::dry_run_mutation` (`tools.rs` line 2354), surfaced as `ToolPreview` in `MutationPending`. For an Action this is its `run()` in a "describe only" mode, or a dedicated `preview()` default that calls `dry_run_mutation`.
- **Bulk runs** preview via `Operation::preview` (ops.rs lines 22–26) → `RunPreview` with a count + samples. Unchanged.

The Action layer does **not** unify the two preview *event types* (`MutationPending` vs `RunPreview`) — those map to distinct, already-built confirm UIs. It unifies *how the model reaches them*, so the model can no longer pick a name (`bulk_price_adjust`) whose advertised confirm UX (line 883) disagrees with its real one.

### 3.3 Mutation confirm + the autonomy tier

The autonomy tier (`tools.rs` lines 143–248: `Risk`, `mutation_risk`, `LOW_RISK_MUTATIONS`, `price_change_is_high_risk`, `resolve_mutation_risk`) is **already wired** into both streaming paths (`streaming.rs` lines 466–496 Anthropic, ~135–139 in the OpenAI block) and `auto_apply_mutation` (lines 1175+). The unified layer **adopts this as-is**:

- `Action::risk(input)` is the trait-level home for what `mutation_risk` computes statically; `resolve_mutation_risk` (which does the async >50% price-swing escalation, lines 237–248) remains the dispatch-time call. Moving the *static* classification onto the trait means `LOW_RISK_MUTATIONS` stops being a third free-floating list — it becomes per-Action metadata, removing one drift source.
- **Low risk** → `auto_apply_mutation` runs it, emits `MutationApplied` + `undo_id`, loop continues (self-correction on failure already implemented, lines 497+).
- **High risk** → `MutationPending` → `ai_execute_action` (lines 551–630), unchanged.

Net: the autonomy tier is **not redesigned**; the Action layer simply gives `risk` a single canonical location and one dispatch site instead of two.

### 3.4 Undo

Two undo mechanisms exist and **both stay**:

- **Single mutations** — `ai_execute_action` and `auto_apply_mutation` both create an undo record (`create_undo_record`) carrying `rollback_tool` + `rollback_input_json` + `undo_snapshot_json` (the `execute_mutation` results, `tools.rs` lines 3534+). `ai_undo_action` (lines 647–686) replays it via `tools::execute_undo`.
- **Bulk runs** — `ai_run_undo` (lines 757–769) replays the per-row undo log in reverse via `batch::undo_run` (`engine/batch.rs` line 122).

The Action's `kind()` already tells callers which undo channel applies, so no new undo plumbing is needed. **Known asymmetry to fix opportunistically (not part of 1.1's contract work):** `ai_run_execute` only executes price adjusts (`batch::execute_price_adjust`, line 723) even though 7 ops can create runs — the `BulkRun` executor should dispatch through `Operation::commit_batch` per `op_id` so archive/promotion/reorder runs actually execute. Flag this to the autonomy/engine owner; it is a latent correctness bug independent of the prompt-drift fix.

---

## 4. Migration / Cutover Plan (incremental — no big-bang)

The five core files total ~8,900 lines (`tools.rs` 218 KB, `tools_catalogue.rs` 102 KB, `intent_engine.rs`, `streaming.rs` 1,228 lines, `engine/ops.rs`). A rewrite is off the table. The cutover is **additive and reversible at every step** — the old dispatch ladder keeps working until the registry fully subsumes it.

### Phase 0 — Scaffolding (no behaviour change)
- Add the `Action` trait + `ActionRegistry` + `dispatch_action` in a new `src-tauri/src/ai/actions/` module (or extend `engine/ops.rs`). Do **not** wire it into `streaming.rs` yet.
- Add a unit test asserting `registry.all_names()` has no duplicates and every name is unique across kinds.
- **Verification:** `cargo build` + `cargo test`; existing dispatch untouched, so all current behaviour is preserved.

### Phase 1 — Make the prompt honest (highest ROI, lowest risk)
- Generate the **"Available Actions"** block in `build_system_prompt` (lines 865–891) by iterating the registry's read + single-mutation Actions, instead of the hand-written list. Even before dispatch is unified, this kills the worst drift: the prompt can no longer advertise a name that isn't registered.
- Register **first the Actions that fix today's dead-ends**, in this order:
  1. `update_product` (the documented gap, tools.rs 154–156) → maps to `update_product_full`/`update_product_price`.
  2. The ~16 working intents (already clean `IntentDef`s) → wrap `execute_intent`.
  3. `bulk_price_adjust` and the other 6 engine ops → `BulkRun` Actions, so their prompt line and their real `RunPreview` path finally agree.
- Keep the old dispatch ladder live. New Actions are reachable because their names already route through the existing engine-op / intent / tool branches.
- **Verification:** assert every name in the generated prompt resolves in the registry (a test that *fails today* for `update_product`). Manual smoke: the WhatsApp price-update workflow (prompt lines 851–863) now completes instead of dead-ending.

### Phase 2 — Route both providers through `dispatch_action`
- Replace the Anthropic ladder (`streaming.rs` ~300–553) with a single `dispatch_action` call. Then replace the OpenAI ladder (~640–900) with the **same** call. Delete the duplicated `ENGINE_OPS` consts and the two `Registry::new()` blocks.
- Bring the **non-streaming** `run_tool_loop` (`ai_admin_commands.rs` 915–1070) onto `dispatch_action` too, so intents/engine ops finally work on the blocking path (they don't today).
- **Verification:** `cargo test`; parity test that a fixed `(name, input)` produces the same `StreamEvent` sequence on both provider paths.

### Phase 3 — Demote raw tools to private
- Remove the ~200 non-Action tools from `filtered_tool_definitions`'s model-facing output (keep them as internal functions). `filtered_tool_definitions` now returns only Action schemas (~25–30). Feature-toggle gating (lines 254–306) moves to filtering *Actions*.
- Collapse `MUTATION_TOOLS`, `is_mutation_intent`, and `LOW_RISK_MUTATIONS` into per-Action `kind()`/`risk()` metadata; delete the standalone lists once nothing references them.
- **Verification:** token-count check that the system prompt + tool payload shrank substantially; full regression of the read/mutation/bulk smoke suite.

**Rollback:** each phase is a separate change; reverting Phase _n_ restores Phase _n−1_ because the old name-based branches are only deleted in Phase 2–3, after the registry has demonstrably taken over.

---

## 5. Out of Scope (already delivered by the overhaul, or owned elsewhere)

This spec is **only** roadmap item 1.1 (action-layer unification). The following are explicitly **not** changed here:

- **SSE streaming** (OpenAI/Gemini) — `streaming.rs`, `openai_client.rs`, `provider.rs`. The Action layer reuses these paths; it does not touch the wire protocol.
- **Cost / usage tracking** — migrations 0016/0017, `record_usage`/`record_turn` (`streaming.rs` line 595, `ai_admin_commands.rs`). Untouched.
- **RBAC tiers** — `commands/rbac.rs`, `require_any_role`/`manager_or_owner` gates on the command surface. Actions inherit these at the command boundary; no change to the role model.
- **Suppliers / purchase orders / promotions** — their *tools* and *ops* already exist; this spec only changes how they're surfaced, not their logic.
- **The autonomy tier itself** — `Risk`, `mutation_risk`, `resolve_mutation_risk`, `auto_apply_mutation`, self-correction (`tools.rs` 143–248, `streaming.rs` 466–496, 1175+) are being completed by the concurrent autonomy agent. This spec **consumes** that work (calls `risk()`/`resolve_mutation_risk`) and must not re-implement or fork it. Coordinate so `LOW_RISK_MUTATIONS` is migrated to Action metadata jointly, not twice.
- **The `ai_run_execute` price-only executor bug** (§3.4) — flagged for the engine/autonomy owner; fixing it is adjacent, not part of the contract-unification deliverable.

---

## 6. Verification

How to prove the unification works and the drift is gone:

1. **Drift guard (the core invariant).** A unit test that builds the registry, generates the system prompt, and asserts: every action name in the prompt resolves in the registry, and every registered read/single-mutation Action appears in the prompt. This test **fails on today's code** (`update_product` is advertised but unresolvable) and passing it is the definition of done for 1.1.
2. **No-duplicate-name test.** Assert `registry.all_names()` is unique and that no name is simultaneously classified across two `ActionKind`s.
3. **Provider parity test.** For a fixed set of `(action_name, input)` cases covering one read, one low-risk mutation, one high-risk mutation, and one bulk run, assert the Anthropic and OpenAI dispatch paths emit the **same `StreamEvent` sequence** (`ToolStart`→…→`MutationApplied`/`MutationPending`/`RunPreview`). This proves the single `dispatch_action` replaced both ladders faithfully.
4. **Regression on existing engine tests.** `engine/ops.rs` (lines 818–843), `engine/selector.rs` (110–161), and `engine/batch.rs` (round-trip undo test, line 256) must stay green — bulk ops are wrapped, not rewritten.
5. **Surface-size assertion.** Test that `filtered_tool_definitions` returns ~25–30 schemas (not ~200) after Phase 3, with a token-budget check on the assembled prompt+tools payload.
6. **Manual smoke (the user-visible bug).** Run the WhatsApp price-update workflow end to end (system prompt lines 851–863): forward a price, model calls the `update_product` Action, preview/confirm fires, undo works. Verify it **completes** where it currently dead-ends.
7. **Build/lint gate.** `cargo build && cargo test` in `src-tauri/` after each phase. (Confirm the project's exact lint/test invocation from `package.json`/CI before relying on `npm run build && npm test` from `CLAUDE.md`, which is frontend-oriented.)

---

## 7. Recommended Cutover Order (summary)

**Scaffold the registry first (Phase 0, no behaviour change), then make the prompt honest (Phase 1) by registering exactly the Actions that fix today's dead-ends — `update_product` first, then the ~16 working intents, then the 7 bulk ops as `BulkRun` Actions — while leaving the old dispatch ladder live underneath.** Only once the prompt is generated from the registry and every advertised name resolves do you route both provider paths (and the non-streaming loop) through the single `dispatch_action` (Phase 2), then demote the ~200 raw tools to private and collapse the three mutation lists into per-Action metadata (Phase 3). This keeps every step additive and reversible, fixes the highest-impact reliability bug (the prompt advertising names that don't exist) on day one, and treats the in-flight autonomy tier and the SSE/cost/RBAC overhaul as fixed dependencies it consumes rather than touches.
