# ZanAI Scale Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bound ZanAI model context and browser memory so large catalogues and long sessions remain responsive and truthful.

**Architecture:** A provider-independent Rust budget limits each tool result and the cumulative tool output per turn before either provider receives it. A pure TypeScript helper establishes one display-message retention invariant for both POS and OfficeAI renderers.

**Tech Stack:** Rust 2021, Tokio, sqlx/SQLite, React 19, TypeScript, Vitest, Tauri v2

**Spec:** `docs/superpowers/specs/2026-08-14-zanai-scale-hardening-design.md`

## Global Constraints

- Preserve the shared POS/OfficeAI conversation and all existing role, confirmation, undo, and offline behavior.
- Money remains integer minor units; this change does not transform monetary values.
- External input is never fetched directly by the frontend.
- Default limits are 24,000 characters per tool result, 64,000 characters across tool results in one turn, 40 provider-history messages, and 100 rendered messages.
- Truncation must be Unicode-safe and explicit to the model.

---

### Task 1: Tool-result budget unit

**Files:**
- Create: `src-tauri/src/ai/result_budget.rs`
- Modify: `src-tauri/src/ai/mod.rs`

**Interfaces:**
- Produces: `ToolResultBudget::new(per_result_chars, per_turn_chars)` and `ToolResultBudget::apply(tool_name, content) -> BudgetedToolResult`.
- Produces: `BudgetedToolResult { content, original_chars, emitted_chars, truncated, reason }`.

- [ ] **Step 1: Write failing Rust tests** for unchanged small results, Unicode-safe per-result truncation, cumulative exhaustion, and a non-empty continuation notice.
- [ ] **Step 2: Run** `cargo test --lib ai::result_budget::tests -- --nocapture` and confirm unresolved module/type failures.
- [ ] **Step 3: Implement** character-count budgeting with a JSON truncation notice and no result-content logging.
- [ ] **Step 4: Re-run** the focused test and confirm all result-budget tests pass.

### Task 2: Runtime configuration

**Files:**
- Modify: `src-tauri/src/ai/config.rs`
- Modify: `src-tauri/src/commands/ai_admin_commands.rs`

**Interfaces:**
- Produces: `AiParams.tool_result_max_chars: usize` and `AiParams.turn_tool_results_max_chars: usize`.
- Consumes: `ToolResultBudget::new` limits in Task 3.

- [ ] **Step 1: Add failing configuration tests** proving defaults, invalid-value fallback, and clamps of 4,000..100,000 per result and 8,000..250,000 per turn.
- [ ] **Step 2: Run** `cargo test --lib ai::config::tests -- --nocapture` and confirm failures identify missing fields/config parsing.
- [ ] **Step 3: Add** the fields, defaults, `app_config` keys, parsing, and admin diagnostics exposure.
- [ ] **Step 4: Re-run** the focused test and confirm it passes.

### Task 3: Apply budgets to both providers

**Files:**
- Modify: `src-tauri/src/ai/streaming.rs`

**Interfaces:**
- Consumes: `ToolResultBudget` and the two `AiParams` limits.
- Preserves: authorization errors, mutation previews, pending actions, and undo data.

- [ ] **Step 1: Add failing streaming unit tests** proving both Anthropic and OpenAI result adapters call the same budget and preserve error results.
- [ ] **Step 2: Run** `cargo test --lib ai::streaming::tests -- --nocapture` and confirm the new assertions fail.
- [ ] **Step 3: Route** successful plain-read and intent results through one per-turn budget in each provider loop; emit metadata-only `tracing::warn!` on truncation.
- [ ] **Step 4: Re-run** focused streaming tests and result-budget tests.

### Task 4: Bound rendered chat state

**Files:**
- Create: `src/zanai/messageRetention.ts`
- Create: `src/__tests__/zanAiMessageRetention.test.ts`
- Modify: `src/officeai/useChatController.ts`

**Interfaces:**
- Produces: `appendBoundedMessages(current, additions, maxMessages?)` and `boundLoadedMessages(messages, maxMessages?)`.
- Preserves: messages with `pendingAction` or `pendingBatchActions`, plus the newest messages.

- [ ] **Step 1: Write failing Vitest tests** for a 100-message bound, stable newest ordering, actionable-message preservation, and no mutation of inputs.
- [ ] **Step 2: Run** `npm test -- src/__tests__/zanAiMessageRetention.test.ts` and confirm the missing module failure.
- [ ] **Step 3: Implement** the pure helper, then replace all append and initial-load paths in `useChatController` with the helper.
- [ ] **Step 4: Re-run** the focused test and the shared-runtime/widget tests.

### Task 5: Full verification

**Files:**
- Modify only files required to fix regressions caused by Tasks 1-4.

**Interfaces:**
- Consumes all prior tasks.
- Produces fresh verification evidence.

- [ ] **Step 1: Run** `npm run check`.
- [ ] **Step 2: Run** `npm run build`.
- [ ] **Step 3: Run** isolated `cargo test --lib` using `C:\Users\super\AppData\Local\Temp\zanpos-codex-target` as `CARGO_TARGET_DIR`.
- [ ] **Step 4: Run** isolated `cargo clippy --all-targets -- -D warnings` with the same target directory.
- [ ] **Step 5: Run** `git diff --check`, inspect the scoped diff, and report any pre-existing formatter/worktree limitations separately.
