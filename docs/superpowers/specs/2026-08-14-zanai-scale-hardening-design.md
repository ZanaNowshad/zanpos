# ZanAI Scale Hardening Design

## Objective

Keep ZanAI responsive, truthful, and safe when a store has a large catalogue, long operating history, or several tool calls in one turn. The upgrade must preserve the shared POS/OfficeAI conversation, cashier read-only policy, manager confirmations, undo support, and offline-first behavior.

## Current State

The runtime already has bounded conversation history, provider prompt caching, role-filtered tool definitions, three-way bounded parallelism for independent reads, cursor-based bulk mutation batches, persistent action/task ledgers, and FTS5 product search. Individual read tools usually limit row counts, but the streaming loops do not enforce a provider-independent budget across the combined tool results in a turn. The UI context is bounded, while display-message updates are bounded only on some completion paths rather than through one invariant.

## Architecture

### 1. Provider-independent result budgeting

Add a small `ToolResultBudget` unit in the AI runtime. Every successful read or intent result is passed through it before being appended to an Anthropic or OpenAI conversation. It enforces both a per-result character limit and a cumulative per-turn character limit using Unicode-safe character boundaries.

Truncated output ends with a machine- and model-readable JSON notice containing `truncated`, `reason`, and `next_action`. The notice tells ZanAI to narrow the query, aggregate, or request the next page. Errors, mutation previews, action records, and undo records are not truncated by this unit.

The limits are operator-configurable through `app_config`, clamped to safe ranges, and default to 24,000 characters per result and 64,000 characters per turn. This is large enough for useful analysis while preventing several broad queries from consuming the entire context window.

### 2. Long-session UI invariant

Create a pure `appendBoundedMessages` helper and route every append through it. Keep the newest 100 rendered messages while preserving any currently actionable message. Provider history remains separately bounded at 40 role messages. Loaded history is also display-bounded. This prevents a full-day POS session from creating an ever-growing React tree.

### 3. Large-data access behavior

Retain FTS5 search and cursor-based batch execution. Do not add speculative duplicate indexes. Tool-result truncation is explicit rather than silent, so the model can use existing filters, aggregations, and pagination instead of attempting to ingest an entire catalogue. Exact writes continue to be executed by deterministic Rust operations rather than generated from truncated text.

### 4. Diagnostics

Emit a structured tracing event whenever budgeting truncates a result. It records the tool name, original character count, emitted character count, and reason, without logging the result content. Existing turn usage logging remains authoritative for latency and token counts.

## Failure Handling

- Invalid configuration falls back to defaults; values are clamped.
- Unicode is truncated by characters, never raw bytes.
- Once a turn budget is exhausted, later successful reads receive a compact continuation notice rather than an empty result.
- Authorization and tool execution errors remain intact and are not disguised as truncation.
- Pending actions are preserved when the display list is compacted.

## Verification

- Rust unit tests prove per-result limits, cumulative limits, Unicode safety, exhaustion behavior, and configuration clamps.
- TypeScript unit tests prove display bounds, newest-message ordering, and preservation of actionable messages.
- Run `npm run check`, `npm run build`, isolated `cargo test --lib`, and `cargo clippy --all-targets -- -D warnings`.
- A manual Tauri smoke test remains necessary for final WebView2 interaction evidence if the live app cannot be controlled automatically.

## Non-goals

- No cloud vector database or mandatory online retrieval dependency.
- No automatic execution of additional mutation classes.
- No weakening of cashier or branch authorization.
- No claim that every possible future dataset size can be loaded into one model turn; the system must page and aggregate instead.
