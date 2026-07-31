# ZANPOS Feature Proposals

Design specifications for five proposed features. **These are specs, not
implementations** — no existing code or schema is changed by anything in this
directory. Each spec is grounded in files and tables actually present in the
current tree (`feature/ai-tools-suppliers-wip`).

Conventions referenced throughout: money is **integer minor units** (1 BHD =
1000 fils; `i64` in Rust); IDs are ULIDs; multi-branch sync runs through the
Supabase outbox + LAN hub with a `sync_watermark` seed in
[`0001_initial.sql`](../../src-tauri/migrations/0001_initial.sql). See
[`AGENTS.md`](../../AGENTS.md) and [`README.md`](../../README.md) for the money
rules and architecture.

| # | Proposal | Effort | One-line summary |
|---|----------|--------|------------------|
| 01 | [NBR / GCC e-invoicing QR on receipts](01-nbr-einvoicing-qr.md) | S–M | Append a ZATCA-style base64 TLV QR (seller, TRN, timestamp, total, VAT) to the receipt that already prints TRN + VAT breakdown. |
| 02 | [Arabic / RTL UI + bilingual receipts](02-arabic-rtl-bilingual-receipts.md) | L | Add the i18n/RTL layer the app entirely lacks today, plus Arabic-capable thermal receipts. |
| 03 | [Purchasing loop: cost & margin](03-purchasing-loop-cost-margin.md) | L | Finish the half-built suppliers/PO loop (migration 0009), fix the broken receiving tool, add weighted-average cost and gross-margin reporting. |
| 04 | [Benefit / ECR card-terminal integration](04-benefit-ecr-card-terminal.md) | M–L | Connect a P2PE card terminal over ECR for amount push, approval capture, and EOD reconciliation — preserving PCI SAQ P2PE. |
| 05 | [Promotions / loyalty rules engine](05-promotions-loyalty-rules-engine.md) | L | A declarative rules engine (mix-and-match, happy-hour, %/amount) plus loyalty accrual and points redemption at the till. |

## Cross-cutting notes

- **No i18n/RTL exists today** (confirmed by search) — Proposal 02 is greenfield
  and several other specs (Arabic product/promotion names) depend on it.
- **Reports are revenue/VAT only** — confirmed in
  [`report_commands.rs`](../../src-tauri/src/commands/report_commands.rs); there is
  no COGS or margin anywhere. Proposal 03 introduces it.
- **The suppliers/PO tables exist but the workflow is broken** — the AI
  `receive_purchase_order` tool writes to a non-existent `products.stock_quantity`
  column and overwrites cost with last-cost. Proposal 03 documents and fixes this.
- **Money invariant** — every proposal keeps amounts as integer minor units and
  routes formatting/rounding through `domain::money`. Any QR/ECR/promotion amount
  is derived at the boundary, never stored as float.
- **Sync wiring is a recurring gap** — the 0009 purchasing tables and any new
  tables (promotions, loyalty ledger, cost history, terminal batches) must be
  added to `sync_watermark` and classified per
  [`sync-conflict-resolution.md`](../sync-conflict-resolution.md).
