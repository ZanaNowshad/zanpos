# Proposal 05 — Promotions / Loyalty Rules Engine

**Status:** Draft proposal · **Effort:** L · **Owner sign-off required**

> Design only. No code or schema changes are made by this document.

## (a) Problem & why it matters for Bahrain/Gulf retail

Gulf retail is heavily promotion-driven — buy-one-get-one, mix-and-match bundles,
weekend/Ramadan price windows, "happy hour" pricing, and loyalty-point redemption
at the till are table stakes for grocery, F&B, and convenience formats. ZANPOS
can today apply only **manual, ad-hoc discounts**: a line discount or a bill
discount entered by the cashier, each requiring a reason (see
[`cart.rs`](../../src-tauri/src/domain/cart.rs) — `line_discount_minor`,
`bill_discount_minor`, and their `_reason` fields). There is no concept of an
automatic, rule-based promotion.

For loyalty: `customers.loyalty_points` is an `INTEGER` column in
[`0001_initial.sql`](../../src-tauri/migrations/0001_initial.sql), and customers
are linked to sales via `sales.customer_id`. But a search across `src-tauri/src`
shows **points are stored, not earned or redeemed by any rule** — there is no
accrual logic on sale finalize and no redemption-at-checkout path. So loyalty is
a number that sits still.

## (b) Proposed approach

Introduce a **declarative rules engine** evaluated against the cart at checkout,
plus point accrual/redemption. The engine is deliberately data-driven (rules in
tables, not code) so promotions can be created without redeploying.

**Rule model.** Each promotion has a *condition* (what triggers it) and an
*effect* (what it does), an optional *time window*, and a priority. Supported
rule types for v1:

- **Percentage / fixed-amount off** a product, category, or whole bill.
- **Mix-and-match / multi-buy** — e.g. "any 3 from category X for 1.000 BHD",
  "buy 2 get 1 free".
- **Time-windowed pricing ("happy hour")** — a price or discount active only
  within a day-of-week + time-of-day window (evaluated in `Asia/Bahrain`, the
  branch timezone already stored on `branches.timezone`).
- **Points redemption** — convert loyalty points to a bill discount at a
  configured points→fils rate.

**Evaluation.** A pure Rust evaluator runs over the `Cart` (`domain/cart.rs`)
after lines are built and produces a set of *applied discounts* expressed in the
existing integer-minor-unit discount fields. Critically, the engine should emit
its result **into the same `line_discount_minor` / `bill_discount_minor`
channels the cart already validates and the receipt already prints**, with a
machine `reason` (the promotion id/name) instead of a hand-typed reason. This
keeps tax math, `Cart::validate()`, EOD discount totals, and
`receiptLines.ts`'s existing "Discount" line all working unchanged. Determinism
and rounding follow `domain::money` (integer, half-up); the engine must be
order-independent and idempotent for a given cart + clock.

**Points.** On finalize (`sale_repo::finalize_sale`), accrue points per an
accrual rule (e.g. N points per BHD of net) onto `customers.loyalty_points`, and
record redemptions as a ledger entry so balances are auditable rather than just
mutated — mirroring the append-only philosophy of `stock_movements` and
`audit_logs`.

## (c) Data-model changes (referencing existing schema)

- New `promotions` table: `promotion_id`, `name`, `name_ar` (optional, ties to
  Proposal 02), `rule_type`, `scope` (`product`/`category`/`bill`),
  `target_id` (product/category id, nullable), `effect_json` (parameters: percent
  bp, fixed minor, buy-qty/get-qty, bundle price minor), `priority`,
  `stackable` (INTEGER), `active_from`/`active_to`, `days_of_week`,
  `start_time`/`end_time`, `branch_id` (nullable = all), `is_active`, standard
  timestamps + `sync_status`/`sync_attempts` (so promotions sync like catalog —
  LWW, like `products`/`tax_rules`).
- New `loyalty_ledger` table (append-only): `ledger_id`, `customer_id`, `sale_id`
  (nullable), `points_delta`, `reason` (`earn`/`redeem`/`adjust`),
  `balance_after`, `created_by_user_id`, `created_at`, `sync_status`. Keep
  `customers.loyalty_points` as the cached running balance for fast reads.
- `app_config` — `loyalty_earn_rate`, `loyalty_redeem_rate_minor`,
  `loyalty_enabled` (default `0`), following the existing `flag_*` config-key
  convention.
- `sale_items` / `sales` — optionally record `applied_promotion_id` per discount
  for reporting; can also be captured in the audit `reason`. No change to money
  representation.
- `sync_watermark` — add `promotions` and `loyalty_ledger` rows so multi-branch
  picks them up (the watermark seed in 0001 must be extended).

## (d) Backend command surface (Tauri commands)

- `promotion_list` / `promotion_upsert` / `promotion_set_active` /
  `promotion_delete` — manager/owner gated (pricing authority).
- `cart_apply_promotions(cart) -> AppliedPromotions` — runs the evaluator and
  returns the discounts to display before payment (the cart already has
  `pos_apply_line_discount` / bill-discount commands in `pos_commands.rs` to
  parallel).
- `loyalty_preview_redeem(customer_id, points)` and the finalize path extended to
  accrue/redeem and write `loyalty_ledger`.
- `loyalty_balance(customer_id)` / `loyalty_history(customer_id)`.

## (e) Frontend touchpoints

- [`CartPanel.tsx`](../../src/components/CartPanel.tsx) — show auto-applied
  promotion lines distinctly from manual discounts.
- [`PaymentModal.tsx`](../../src/components/PaymentModal.tsx) — "redeem points"
  control showing balance and the resulting discount.
- New `PromotionsTab.tsx` (rule builder) under `src/components/`, plus loyalty
  settings in `settings/`.
- [`CustomersTab.tsx`](../../src/components/CustomersTab.tsx) — points balance and
  ledger history.
- [`receiptLines.ts`](../../src/utils/receiptLines.ts) — itemise promotions and
  show points earned/redeemed and new balance.
- `src/tauri/commands.ts`, `src/types.ts` — wrappers and types.

## (f) Effort estimate

**L.** A correct, deterministic, integer-money rules evaluator with mix-and-match
and time windows is non-trivial, and it touches the hot checkout path plus a
rule-builder UI and a loyalty ledger. Phase suggestion: (1) simple %/amount +
points redemption, (2) mix-and-match/multi-buy, (3) time-windowed pricing.

## (g) Risks & open questions for the product owner

- **Biggest open question:** *What are the stacking and conflict rules when
  multiple promotions match one cart — best-discount-wins, explicit priority,
  stack-all, or one-per-line?* This governs the evaluator's core algorithm and is
  the most common source of "the till charged the wrong price" disputes.
- Interaction with VAT: discounts must reduce the taxable base correctly. The
  cart already recomputes tax after discounts in `cart.rs::recalculate`, so
  promotions feeding the same discount fields inherit that — confirm this matches
  NBR expectations for promotional pricing.
- Loyalty economics (earn rate, redemption value, expiry) are a business policy
  decision, not a technical one — needs owner input, including whether points
  expire.
- Performance: promotion evaluation runs on every cart change; the engine must
  stay fast on large carts (the cart validation path already caps at 1M units /
  line). Keep evaluation O(lines × applicable-rules).
