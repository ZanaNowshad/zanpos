# Proposal 04 — Benefit (Bahrain Debit Network) / ECR Card-Terminal Integration

**Status:** Draft proposal · **Effort:** M–L · **Owner sign-off required**

> Design only. No code or schema changes are made by this document.

## (a) Problem & why it matters for Bahrain/Gulf retail

Benefit is Bahrain's national debit/payment network; the vast majority of
in-store card payments in Bahrain run over Benefit-acquired terminals
(including BenefitPay wallet QR). Today ZANPOS treats card payment as a manual,
disconnected step: the cashier reads the amount off the screen, keys it into a
**standalone** card terminal, waits for approval, and then taps a button in
ZANPOS. Nothing connects the two. This causes the classic POS reconciliation
gap — the amount keyed into the terminal can differ from the sale, approvals
aren't captured, and end-of-day card totals must be reconciled by hand against
the terminal's own batch report.

Current state in the code:

- `payments.payment_method` is constrained to `('cash','card','wallet','other')`
  (CHECK in [`0001_initial.sql`](../../src-tauri/migrations/0001_initial.sql)).
  The only card-linked field is `external_reference` (free-text, "tokenised
  reference only"). There is **no terminal integration, no approval code capture,
  no batch/reconciliation**. A search for `ECR`, `terminal`, `P2PE`, `benefit`
  found references only in receipt cosmetics — `receiptLines.ts` relabels the
  `wallet` method as "BenefitPay" in the delivery section — and in compliance
  docs.
- [`docs/compliance-checklist.md`](../compliance-checklist.md) flags **PCI DSS
  SAQ P2PE** as a release gate (`⏳ VENDOR CONFIRM`): "ZANPOS qualifies for SAQ
  P2PE *if* a PCI-listed point-to-point encryption terminal is used (all card
  data never enters the POS software)." Two P0 ops items require confirming the
  terminal vendor's PCI certification. This proposal must **preserve** that
  property: PANs must never touch ZANPOS.

## (b) Proposed approach

Integrate via **ECR (Electronic Cash Register) protocol** to a P2PE-certified
Benefit terminal, so ZANPOS sends the amount and receives an approval result,
while card data stays entirely inside the terminal (preserving SAQ P2PE).

Flow:

1. At payment, instead of manual entry, ZANPOS sends an ECR "sale" request with
   the `amount_minor`-derived amount to the terminal over its supported transport
   (serial/USB or TCP on the LAN — to be confirmed with the acquirer's ECR spec).
2. The terminal performs the transaction with the card and returns an approval
   result: approval code, masked PAN (last 4 only), card scheme, terminal/batch
   id, RRN — **never** the full PAN or track data.
3. ZANPOS records the result against the `card`/`wallet` payment and prints the
   merchant/customer copy lines on the existing thermal receipt.
4. **Reconciliation:** ZANPOS can request the terminal's batch totals at close
   and compare them to its own `card`/`wallet` sums from the shift, surfacing any
   variance in the EOD/Z report — the natural home, since `report_eod_cashup_inner`
   in [`report_commands.rs`](../../src-tauri/src/commands/report_commands.rs)
   already aggregates per-shift card totals.

The terminal I/O belongs in **Rust**, alongside the existing serial/printer
code. The printer path in
[`thermal_commands.rs`](../../src-tauri/src/commands/thermal_commands.rs) already
demonstrates the pattern: synchronous device I/O on a `spawn_blocking` thread,
COM-port handling, and Windows specifics. The ECR client mirrors that.

## (c) Data-model changes (referencing existing schema)

- `payments` — add nullable, **non-sensitive** result fields:
  `approval_code TEXT`, `card_scheme TEXT`, `masked_pan TEXT` (last-4 only),
  `terminal_id TEXT`, `batch_id TEXT`, `rrn TEXT`, `ecr_status TEXT`. These extend
  the existing `external_reference` rather than replace it. **Schema-level
  guarantee:** no column for full PAN, CVV, or track data — consistent with the
  PCI verification already recorded in the compliance checklist.
- New table `card_terminals`: `terminal_id`, `branch_id`, `device_id`, `label`,
  `transport` (`serial`/`tcp`), `address` (COM port or host:port), `is_active`,
  timestamps — following the `devices`-table shape in 0001.
- New table `terminal_batches` (for reconciliation): `batch_id`, `terminal_id`,
  `shift_id`, `closed_at`, `terminal_total_minor`, `pos_card_total_minor`,
  `variance_minor`, `status`. Lets the Z-report show terminal-vs-POS variance.
- `app_config` — `card_terminal_enabled`, default `0`, following the existing
  `thermal_printer_enabled` config-key convention.
- Money stays integer minor units throughout; ECR amount fields are derived from
  `amount_minor` at the boundary.

## (d) Backend command surface (Tauri commands)

- `terminal_list` / `terminal_set_config(label, transport, address)` /
  `terminal_test` — parallels the `thermal_*` config/test commands.
- `terminal_sale(amount_minor, sale_ref) -> EcrResult` — initiates a card sale on
  the terminal and returns the approval result (no PAN).
- `terminal_void(...)` / `terminal_refund(amount_minor, original_ref)` — for the
  refund path that already exists in `refund_commands.rs`.
- `terminal_batch_close(shift_id) -> BatchReconciliation` — pulls terminal totals
  and computes variance against POS card/wallet sums.
- The existing payment recording (`pos_commands.rs` finalize path) extended to
  persist the ECR result fields onto the `payments` row.

## (e) Frontend touchpoints

- [`PaymentModal.tsx`](../../src/components/PaymentModal.tsx) — when card/wallet is
  chosen and a terminal is configured, replace manual confirm with "Send to
  terminal", show waiting/approved/declined states, and capture the result.
- New `settings/CardTerminalTab.tsx` (mirrors `settings/PrinterTab.tsx`).
- [`EodCashupTab.tsx`](../../src/components/EodCashupTab.tsx) /
  `XReportModal.tsx` / `ZReportModal.tsx` — show terminal-batch reconciliation and
  variance.
- [`receiptLines.ts`](../../src/utils/receiptLines.ts) — print approval
  code / masked PAN / scheme on the card payment line.
- `src/tauri/commands.ts`, `src/types.ts` — wrappers and `EcrResult` type.

## (f) Effort estimate

**M–L.** The ZANPOS-side data model, commands, and UI are M. The unknown is the
acquirer's ECR protocol: if Benefit/the vendor provides a documented serial/TCP
ECR spec or an SDK, integration is moderate; a proprietary or undocumented
protocol pushes this to L and adds dependency on vendor cooperation.

## (g) Risks & open questions for the product owner

- **Biggest open question:** *Which exact terminal model / acquirer ECR protocol
  are we targeting, and can the vendor supply ECR documentation or an SDK plus a
  test terminal?* Everything downstream (transport, message format, certification
  path) depends on this; it cannot be reverse-engineered safely.
- **PCI scope must be preserved.** The integration must keep ZANPOS out of card
  data scope (SAQ P2PE). Confirm the chosen terminal is on the PCI P2PE listed
  solutions register — this is already a P0 ops sign-off in the compliance
  checklist. Misdesign here expands PCI scope dramatically.
- Reconciliation assumes the terminal exposes batch totals via ECR; some terminals
  only print them. Confirm programmatic batch retrieval is available.
- Offline behaviour: if the LAN/terminal link drops mid-sale, the result-capture
  flow must reconcile against the terminal's own record to avoid double-charging
  or phantom approvals — define the recovery path.
