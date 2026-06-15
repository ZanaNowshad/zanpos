# ZanAI Insights Engine — Design Spec

**Date:** 2026-06-15
**Status:** draft
**Scope:** Three-phase AI intelligence layer for ZANPOS

## Overview

The ZanAI Insights Engine adds three stacked capabilities to the existing AI Back Office:

1. **Analytical Tools** — 8 new read-only AI tools for affinity analysis, RFM segmentation, forecasting, and margin intelligence
2. **Proactive Intelligence** — background detection loop that surfaces anomalies (stock-outs, refund spikes, cash discrepancies) without being asked
3. **AI-Assisted Workflows** — 5 new bulk operations with checkpoint/undo via the Run Engine, drafted by AI from analysis

All three phases are independent at the file level and can be built in any order. They connect through existing interfaces: the AI tool dispatch system, the RunPanel confirmation flow, and the `Operation` trait registry.

## Phase 1: Analytical Tools

Eight new read-only AI tools in `src-tauri/src/ai/tools_read_ext2.rs`. No schema changes, no new infrastructure.

### Tools

| # | Tool | SQL shape | Complexity | Input params |
|---|------|-----------|------------|-------------|
| 1 | `get_frequently_bought_together` | Self-join `sale_items` on `sale_id`, GROUP BY product pairs, ORDER BY frequency DESC | Complex | `product_id?`, `limit`, `period_days` |
| 2 | `get_bundle_suggestions` | Extends #1: `(unit_price_a + unit_price_b) - (cost_a + cost_b)` for margin-aware bundles | Medium | `limit`, `min_confidence` |
| 3 | `get_weekly_forecast` | 8-week trailing avg grouped by `strftime('%w', sold_at)`, projected 7 days with stddev bounds | Medium | `weeks_lookback` |
| 4 | `get_rfm_segmentation` | NTILE(5) on Recency, Frequency, Monetary per customer; label mapping (Champions, Loyal, At Risk, etc.) | Medium | (none) |
| 5 | `get_margin_trend` | Monthly `(SUM(revenue) - SUM(cost)) / SUM(revenue) * 100` | Simple | `months` |
| 6 | `get_restock_priority` | Weighted score: velocity × 0.3 + (1/days_remaining) × 0.3 + margin × 0.4 | Complex | `period_days`, `limit` |
| 7 | `get_dead_stock_value` | Extends existing `get_dead_stock` with `stock_qty * cost_minor AS value_at_risk` and estimated carrying cost | Simple | `days`, `limit` |
| 8 | `get_category_forecast` | Same as #3 grouped by `categories.name` | Medium | `weeks_lookback` |

### Integration (per tool)

1. `ToolDef` entry in `tools.rs` `all_tool_definitions()` — Analytics section (~line 987)
2. Dispatch arm in `tools_read_ext2.rs` `execute()` match (~line 43)
3. Implementation function in same file
4. UI meta in `toolCards.tsx` (Lucide icon, label, color)

### Performance

- Tool #1 includes mandatory `period_days` default of 30 to bound the self-join
- All queries use existing indexes; no new indexes needed in v1
- `COALESCE` on nullable `cost_minor` with a note returned when data is missing

### Files changed

| File | Change | Est. lines |
|------|--------|------------|
| `src-tauri/src/ai/tools_read_ext2.rs` | 8 new tool implementations | +800 |
| `src-tauri/src/ai/tools.rs` | 8 ToolDef entries | +80 |
| `src/officeai/toolCards.tsx` | 8 UI meta entries | +40 |

## Phase 2: Proactive Intelligence

New module `src-tauri/src/ai/proactive.rs`. Background detection loop runs every 5 minutes. No LLM in the detection path — every rule is a deterministic SQL query.

### Detection Rules (11)

| Rule | SQL condition | Severity |
|------|--------------|----------|
| Stock-out | `stock_quantity = 0 WHERE track_inventory = 1` | Critical |
| Low stock | `stock_quantity <= reorder_point AND stock_quantity > 0` | Warning |
| Refund spike | `COUNT(*) > 2x 7-day rolling average` | Critical |
| Cash discrepancy | `ABS(expected - actual) > threshold (500 fils default)` | Critical |
| Sales drop | `< 50% of same-day-last-week revenue` | Warning |
| Overstock | `> 90 days supply at current daily velocity` | Info |
| Receipt gap | `receipt_number gap > 1` | Warning |
| Shift too long | `open shift > 12 hours` | Warning |
| Sync stuck | `consecutive_failure_count >= 3` | Critical |
| High discounts | `discount_rate > 20% per cashier per shift` | Warning |
| Negative margin | `cost_minor > selling_price` | Warning |

### Runtime

```
lib.rs setup():
  tokio::spawn(proactive::run_detection_loop(app_handle, pool))
    loop { sleep 5min; run all rules; persist alerts; emit events }
```

Watermarks per rule in `proactive_watermark` table prevent re-firing the same row-level anomaly. Crash recovery replays from last persisted cursor.

### Alert flow

1. Detection loop runs rule → finds anomaly → INSERT into `proactive_alerts`
2. `app_handle.emit("proactive-alerts", alerts)` pushes to frontend
3. Frontend listeners in KPI sidebar + `ReminderPopup` for critical alerts
4. Pull fallback via `admin_get_alerts` and `admin_dismiss_alert` commands

### Data model

```sql
CREATE TABLE proactive_alerts (
    alert_id      TEXT PRIMARY KEY,
    branch_id     TEXT NOT NULL,
    alert_type    TEXT NOT NULL,
    severity      TEXT NOT NULL CHECK (severity IN ('info','warning','critical')),
    title         TEXT NOT NULL,
    description   TEXT NOT NULL,
    detail_json   TEXT,
    detected_at   TEXT NOT NULL,
    dismissed_at  TEXT,
    dismissed_by_user_id TEXT,
    created_at    TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_alerts_branch ON proactive_alerts(branch_id, detected_at DESC);
CREATE INDEX idx_alerts_undismissed ON proactive_alerts(branch_id) WHERE dismissed_at IS NULL;

CREATE TABLE proactive_watermark (
    rule_name    TEXT PRIMARY KEY,
    last_checked TEXT NOT NULL
);
```

### UI surface

- KPI sidebar: alerts section with count badge + 3 most recent non-dismissed alerts
- Critical alerts: reuse existing `ReminderPopup` component
- New `"insights"` tab in OfficeAIPage: full alert history table with severity badges, dismiss action, time-series for repeat anomalies

### RBAC

- Read alerts: any authenticated user
- Dismiss alerts: manager/owner

### Files changed

| File | Change | Est. lines |
|------|--------|------------|
| `src-tauri/src/ai/proactive.rs` | New — detection engine + rules + loop | +350 |
| `src-tauri/src/ai/mod.rs` | `pub mod proactive;` | +1 |
| `src-tauri/src/db/repositories/proactive_repo.rs` | New — CRUD for alerts + watermarks | +80 |
| `src-tauri/src/db/repositories/mod.rs` | `pub mod proactive_repo;` | +1 |
| `src-tauri/src/domain/ai_admin.rs` | `ProactiveAlert` struct + StreamEvent variant | +15 |
| `src-tauri/src/commands/ai_admin_commands.rs` | `admin_get_alerts`, `admin_dismiss_alert` | +50 |
| `src-tauri/src/lib.rs` | Register commands, spawn background task | +20 |
| `src/tauri/commands.ts` | Frontend bindings | +15 |
| `src/officeai/officeAiTypes.ts` | `ProactiveAlert` type, `"insights"` tab variant | +15 |
| `src/officeai/KpiSidebar.tsx` | Alerts section | +40 |
| `src/officeai/OfficeAIPage.tsx` | Alert event listener, Insights tab | +15 |
| `src/types.ts` | `ProactiveAlert` TS type | +8 |
| `src-tauri/migrations/0010_proactive_alerts.sql` | New migration | +20 |

## Phase 3: AI-Assisted Workflows

Five new `Operation` trait implementations in `ops.rs`. Three new `Selector` variants in `selector.rs`. All flow through the existing RunPanel → preview → confirm → execute → undo pipeline.

### New Operations

| Operation ID | Description | Selector | Undo strategy |
|---|---|---|---|
| `bulk.stock_variance_fix` | Write correction stock_movements for stock-take discrepancies | `variance_threshold` (new) | Reverse movement rows |
| `bulk.promotion_apply` | Create time-bounded promotional prices (`price_type='promotional'`) | `category_subtree` (existing) | Set `effective_to` on promo rows |
| `bulk.supplier_price_sync` | Update cost + selling price from supplier data | `supplier_id` (new) | Restore previous cost_minor + price |
| `bulk.product_archive` | Set `is_active=0` on dead products | `text` + `active` (existing) | Flip `is_active` back |
| `bulk.reorder_point_update` | Batch update `reorder_point` values | `below_reorder` (new) | Restore previous reorder_point |

### New Selector Variants

In `selector.rs`, added to the existing `Selector` struct:

```rust
pub variance_threshold: Option<i64>,  // |expected - counted| >= this
pub supplier_id: Option<String>,      // filter by supplier
pub below_reorder: Option<bool>,      // only products below reorder point
```

### Flow

1. User asks AI a question ("Which products should I archive?")
2. AI analyzes via Phase 1 tools (`get_dead_stock_value`, etc.)
3. AI calls engine tool with selector + operation params
4. `runs::create_run` creates `ai_runs` record
5. `StreamEvent::RunPreview` sent to chat controller
6. RunPanel shows description + product count
7. User clicks Execute → `aiRunExecute` command
8. Batch engine loops over matched products in a transaction
9. Undo snapshots written to `ai_run_undo_log` before each mutation
10. Transaction commits, run marked `completed`

### Undo Design

Each operation writes undo records during `commit_batch` before mutation. The undo function reads these records and replays in reverse. Undo is triggered by `undo_run` in `batch.rs` or a dedicated per-operation undo path. If undo fails, the run is marked `undo_failed` and surfaced to admin.

### Migration

Only `bulk.supplier_price_sync` potentially needs `products.supplier_id` if the suppliers migration (0009) is applied. Other four operations work on existing schema.

### Files changed

| File | Change | Est. lines |
|------|--------|------------|
| `src-tauri/src/ai/engine/ops.rs` | 5 new structs implementing Operation trait | +400 |
| `src-tauri/src/ai/engine/selector.rs` | 3 new Selector variants + SQL compilation | +100 |
| `src-tauri/src/ai/engine/batch.rs` | Undo functions per new operation | +200 |
| `src-tauri/src/ai/streaming.rs` | Add 5 ops to ENGINE_OPS constant | +10 |
| `src-tauri/src/ai/engine/mod.rs` | Registry registration | +15 |

## Error Handling

- **Phase 1**: NULL `cost_minor` handled via COALESCE with a note returned when data is missing (existing pattern)
- **Phase 2**: SQLite locked during timer tick → skip cycle, log warning. Crash recovery replays from last watermark — never double-fires
- **Phase 3**: Validation runs before preview. Invalid selectors/params return structured errors. Undo failure marks run as `undo_failed`. Undo snapshots written before mutation in same transaction
- All new commands follow existing RBAC: read tools = any authenticated, alerts = any authenticated, dismiss = manager/owner, bulk ops = manager/owner

## Testing

### Rust (cargo test --lib)

- Phase 1: 1 test per tool with in-memory SQLite seeded with known data
- Phase 2: 1 test per detection rule — trigger and non-trigger cases
- Phase 3: 1 test per Operation — validate, preview, commit_batch, undo

### Frontend (Vitest)

- KPI sidebar alert rendering with mock data
- RunPanel description rendering per operation type

## Build Order

| Phase | Effort | Risk | Ships |
|-------|--------|------|-------|
| 1 — Analytical Tools | ~920 lines | Low | 8 new read tools, zero new infrastructure |
| 2 — Proactive Intelligence | ~630 lines | Medium | New `proactive.rs` module + background timer + Tauri events |
| 3 — AI-Assisted Workflows | ~725 lines | Low | Extends existing Operation trait + RunPanel |

Phase 1 recommended first (zero-risk, gives AI richer context for Phases 2-3).
