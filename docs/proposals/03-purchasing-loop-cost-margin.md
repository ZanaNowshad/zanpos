# Proposal 03 — Close the Purchasing Loop: Reorder → Draft PO → Receiving → Weighted-Average Cost → Gross-Margin Reporting

**Status:** Draft proposal · **Effort:** L · **Owner sign-off required**

> Design only. No code or schema changes are made by this document.

## (a) Problem & why it matters for Bahrain/Gulf retail

A retailer cannot manage a business on revenue alone — they need to know what
stock costs, when to reorder, and what margin each product earns. ZANPOS today
can sell and report **revenue and VAT**, but it cannot answer "what did I make on
this?" Migration [`0009_suppliers_and_purchases.sql`](../../src-tauri/migrations/0009_suppliers_and_purchases.sql)
laid the foundation — `suppliers`, `purchase_orders`, `purchase_order_lines`
exist — and some AI tools were started, but **the loop is broken and incomplete**.

Concrete findings from the current tree:

1. **Reports have zero margin/COGS.** A search across `src-tauri/src` for
   `gross_margin`, `margin`, `cogs`, `weighted`, `average_cost` found no
   reporting use. [`report_commands.rs`](../../src-tauri/src/commands/report_commands.rs)
   exposes revenue, discount, tax, cash/card, refunds — never cost.
2. **Cost basis is naïve.** `products.cost_minor` exists (nullable) but is a
   single static figure. There is no weighted-average cost and no cost history.
3. **The receiving tool is buggy.** The AI tool `receive_purchase_order` in
   [`tools_write_ext3.rs`](../../src-tauri/src/ai/tools_write_ext3.rs) runs
   `UPDATE products SET stock_quantity = COALESCE(stock_quantity,0) + ? ...` — but
   **`products` has no `stock_quantity` column.** Real stock lives in
   `stock_levels.quantity_on_hand` (a `TEXT` decimal) per
   [`0001_initial.sql`](../../src-tauri/migrations/0001_initial.sql). The same
   phantom column appears in read tools (e.g. `low_stock_with_velocity` in
   `tools_read_ext2.rs`). So receiving today either errors or silently no-ops on
   real stock, and it **overwrites `cost_minor` with the last unit cost** rather
   than computing a weighted average. It also writes **no `stock_movements` row**,
   breaking the audit/movement trail that sales and stock-takes maintain.
4. **Purchasing data does not sync.** `suppliers`, `purchase_orders`, and
   `purchase_order_lines` are absent from the `sync_watermark` seed list in
   `0001_initial.sql`, so multi-branch sync ignores them entirely.
5. **No UI.** There is no purchasing/suppliers tab among the components in
   `src/components/` (`InventoryTab.tsx`, `ProductsTab.tsx`, etc. exist; no
   PO/receiving screen).

## (b) Proposed approach

Build the loop end-to-end and put cost on a correct integer-minor-unit footing.

1. **Reorder signal.** Reuse the existing reorder fields — `products.reorder_point`
   and `stock_levels.quantity_on_hand` — which `get_low_stock` /
   `low_stock_with_velocity` already (attempt to) read. Fix those reads to use
   `stock_levels`, then surface a "needs reorder" list.
2. **AI-suggested draft PO.** `create_purchase_order` already inserts a `draft`
   PO + lines (`tools_write_ext2.rs`). Keep it, but feed it from the reorder list
   and the product's `default_supplier_id` (column already on `products`). The AI
   proposes quantities (reorder gap, optionally velocity-adjusted) for human
   confirmation — consistent with the existing AI mutation-confirmation model.
3. **Receiving flow (fix + complete).** Rewrite receiving to, per line received:
   (a) update `stock_levels.quantity_on_hand` via the existing inventory movement
   path in [`inventory/movements.rs`](../../src-tauri/src/inventory/movements.rs)
   so a `stock_movements` row is written with `movement_type = 'purchase'` and
   `reference_type = 'purchase_order'`, `reference_id = po_id`; (b) recompute
   **weighted-average cost** in integer arithmetic; (c) update PO line
   `received_qty` and roll PO status (`partial`/`received`) as the current code
   already does.
4. **Weighted-average cost.** New `new_avg = (qty_on_hand * old_cost +
   recv_qty * unit_cost) / (qty_on_hand + recv_qty)`, computed with i128
   intermediates and half-up rounding to match `domain::money`. Store the result
   in a cost-basis field (below) — never via floating point.
5. **Gross-margin reporting.** Add COGS = `Σ(quantity_sold × cost_basis_at_sale)`
   and margin = revenue − COGS to the report layer. Cost basis must be **snapshotted
   at sale time** (like `tax_rule_snapshot` already is on `sale_items`) so that
   later cost changes don't retroactively rewrite historical margin.

## (c) Data-model changes (referencing existing schema)

- `sale_items` — add `cost_minor_snapshot INTEGER NOT NULL DEFAULT 0`. Mirrors the
  existing `tax_rule_snapshot` / `unit_price_minor` snapshot pattern on the same
  table. Captured from the product's weighted-average cost at finalize time. This
  is the linchpin for correct historical margin.
- `products` — keep `cost_minor` as the *current weighted-average* cost (correct
  its update semantics; do **not** add the phantom `stock_quantity`). Optionally
  add `cost_method TEXT NOT NULL DEFAULT 'wac'` for future FIFO support.
- New table `product_cost_history` (append-only, like `stock_movements`):
  `cost_history_id`, `product_id`, `branch_id`, `old_cost_minor`,
  `new_cost_minor`, `recv_qty`, `source` (`purchase`/`manual`), `po_id`,
  `created_at`. Gives an auditable cost trail.
- `stock_movements` — no schema change; introduce `movement_type = 'purchase'` as
  a new value (the column is free-text today).
- `sync_watermark` — **add rows** for `suppliers`, `purchase_orders`,
  `purchase_order_lines` (and `product_cost_history`) so multi-branch sync
  includes purchasing. Note the LWW-vs-device-authoritative classification per
  [`docs/sync-conflict-resolution.md`](../sync-conflict-resolution.md) must be
  decided (POs are arguably append/device-authoritative like sales).
- Add `created_at`/`updated_at` and `sync_status`/`sync_attempts` columns to the
  0009 tables to match the sync convention used by every synced table in 0001
  (the 0009 tables currently lack `sync_status`).

## (d) Backend command surface (Tauri commands)

Purchasing is currently AI-tool-only; promote it to first-class commands so a UI
can drive it without the AI:

- `supplier_list` / `supplier_upsert` / `supplier_delete`.
- `po_list(status?)` / `po_get(po_id)` / `po_create(supplier_id, lines)` /
  `po_update_status` / `po_cancel`.
- `po_receive(po_id, lines[])` — the corrected receiving command (stock movement +
  WAC + cost-history + PO status), shared by the AI tool and the UI.
- `report_margin(branch_id, from_date, to_date)` and
  `report_product_margin(...)` — new report commands returning
  revenue/COGS/gross-margin, alongside the existing `report_date_range` and
  `report_top_products`.
- `suggest_reorder_po(supplier_id?)` — builds a draft PO from the reorder list.

## (e) Frontend touchpoints

- New `SuppliersTab.tsx` and `PurchaseOrdersTab.tsx` (+ a receiving modal) under
  `src/components/`, registered in `SettingsTab.tsx` / back-office nav.
- `ReportsTab.tsx` and `CashierReportTab.tsx` — add a margin/COGS column or
  panel (guard behind manager/owner, since cost is sensitive).
- `ProductFormModal.tsx` — show current weighted-average `cost_minor` (read-only
  or manager-editable) and `default_supplier_id`.
- `InventoryTab.tsx` — reorder list with a "create draft PO" action.
- `src/tauri/commands.ts` and `src/types.ts` — wrappers + types for all the above.

## (f) Effort estimate

**L.** This is the largest of the five: a correctness fix (phantom column, WAC,
movement rows), new reporting math, schema additions including sync wiring, and
two-to-three new UI screens. The pieces are independently testable.

## (g) Risks & open questions for the product owner

- **Biggest open question:** *Is weighted-average cost the agreed costing method,
  and do we backfill `cost_minor` / margin for historical sales (which lack a
  cost snapshot), or does margin reporting start "from today forward"?* Without a
  snapshot, past sales have no defensible COGS.
- The existing buggy `receive_purchase_order` may already be in use by AI
  sessions — confirm whether any production DB has bad/partial PO state to
  migrate or clean up.
- Sync classification for purchasing tables (LWW vs append-only) must be settled
  before multi-branch rollout to avoid receiving the same PO twice across devices.
- Margin visibility is sensitive (reveals supplier pricing); confirm it is
  manager/owner-only, consistent with how Z-reports are gated in
  `report_commands.rs`.
