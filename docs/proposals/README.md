# ZANPOS Feature Proposals

Design specifications. **These are specs, not implementations** — nothing in this
directory changes code or schema by itself.

> **Re-grounded 2026-09-11 against `upgrade/zanpos-open-source` (`9951d3c`).**
> These specs were written against `feature/ai-tools-suppliers-wip`, and parts of
> three of them have since shipped. The summaries below were describing work that
> already exists, which would have sent the next reader to rebuild it. Each row
> now says what landed and what did not. The specs themselves are left as
> written — deciding what is still wanted from them is the owner's call, not a
> documentation edit.

Conventions referenced throughout: money is **integer minor units** (1 BHD =
1000 fils; `i64` in Rust); IDs are ULIDs; multi-branch sync runs through the
Supabase outbox + LAN hub. See [`AGENTS.md`](../../AGENTS.md) and
[`README.md`](../../README.md) for the money rules and architecture.

| # | Proposal | Effort | Status on this base |
|---|----------|--------|---------------------|
| 01 | [NBR / GCC e-invoicing QR on receipts](01-nbr-einvoicing-qr.md) | S–M | **Open.** Append a ZATCA-style base64 TLV QR (seller, TRN, timestamp, total, VAT) to the receipt that already prints TRN + VAT breakdown. |
| 02 | [Arabic / RTL UI + bilingual receipts](02-arabic-rtl-bilingual-receipts.md) | L | **Half delivered.** The i18n layer exists — `src/i18n/` with `locales/ar/` and `locales/en/`, and `locales/parity.test.ts` asserting every key has a real Arabic translation. **Not** delivered: Arabic-capable thermal receipts (no Arabic handling in `thermal_commands.rs` or `receipt_pdf.rs`) and a systematic RTL layout pass. Scope the spec to the receipt half. |
| 03 | [Purchasing loop: cost & margin](03-purchasing-loop-cost-margin.md) | L | **Largely delivered.** `0009_suppliers_and_purchases.sql`, `0022_product_cost_history.sql` and `0023_sale_item_cost_snapshot.sql` are in; `report_commands.rs` returns `gross_margin_minor` and `margin_basis_points`; weighted-average cost lives in `purchasing_commands.rs`; `receive_purchase_order` increments stock per line. The `products.stock_quantity` write this spec was written to fix is **gone** — the only remaining mentions are a tool description and a comment at `tools_read_ext3.rs:416` noting the column does not exist. Re-read before assuming anything here is outstanding. |
| 04 | [Benefit / ECR card-terminal integration](04-benefit-ecr-card-terminal.md) | M–L | **Open.** Connect a P2PE card terminal over ECR for amount push, approval capture, and EOD reconciliation — preserving PCI SAQ P2PE. |
| 05 | [Promotions / loyalty rules engine](05-promotions-loyalty-rules-engine.md) | L | **Half delivered.** Loyalty accrual *and redemption* exist: `0056_loyalty_ledger.sql` adds an append-only signed `loyalty_events` ledger, and `loyalty_repo.rs` implements `Earn`, `Redeem`, `Adjust` and `Opening` with insufficient-balance handling. **Not** delivered: the declarative promotions engine (mix-and-match, happy-hour, %/amount) — there is no promotions migration. Scope the spec to promotions. |
| 06 | [Hub TLS with fingerprint pinning](06-hub-tls-fingerprint-pinning.md) | M–L | **Open, and the last P0.** The LAN hub binds `0.0.0.0` over plaintext HTTP, so the per-device tokens `pairing.rs` is careful to digest cross the shop WiFi in the clear along with every synced row. Closes ledger finding D02. |

## Cross-cutting notes

- **i18n exists** — `src/i18n/` with `ar`/`en` locales and a parity test. The
  earlier note here said it did not, which was true of the branch these specs
  were written against and false of this one. Arabic *receipts* remain absent.
- **Margin reporting exists** — `report_commands.rs` returns gross margin and
  margin basis points, backed by `0022`/`0023`. The earlier claim that reports
  were "revenue/VAT only, no COGS or margin anywhere" no longer holds.
- **Loyalty redemption exists** — points can be spent, not only earned, and the
  `loyalty_events` ledger is the source of truth (`customers.loyalty_points` is a
  local cache that no longer travels over sync).
- **Money invariant** — every proposal keeps amounts as integer minor units and
  routes formatting/rounding through `domain::money`. Any QR/ECR/promotion amount
  is derived at the boundary, never stored as float.
- **Sync wiring is a recurring gap** — any new table (promotions, terminal
  batches) must be added to `SYNC_TABLES` in `sync_v2/apply.rs` and classified per
  [`sync-conflict-resolution.md`](../sync-conflict-resolution.md). Note that
  `apply.rs` has a test asserting every synced table has an apply handler, so a
  table added to the list without one fails the suite rather than failing quietly.
- **Verification is manual** — GitHub Actions is billing-blocked on this repo
  (ledger OPS1), and the Rust crate is Windows-only, so every gate is a local run
  on Windows. Run `npm run bootstrap` first; a clean checkout cannot build
  otherwise.
