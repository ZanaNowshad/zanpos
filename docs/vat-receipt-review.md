# ZANPOS — VAT & Receipt Review Pack

Prepared for the finance/tax adviser review listed as a P0 release gate.

The gate asks for a review of the receipt VAT breakdown and TRN. That review needs
something concrete to look at, so this records exactly what the till computes and
what it prints, and raises the questions an adviser has to answer. Everything
below is a statement about **this codebase**, verified against the source. Nothing
here is tax advice, and the legal questions in §5 are deliberately left open.

Currency is Bahraini dinar: 3 decimals, minor unit = fils, so 1.900 BHD = 1900
fils. All amounts in this document are fils unless written as BHD.

---

## 1. Where VAT is computed

Server-side, in `sale_repo::finalize_sale`
(`src-tauri/src/db/repositories/sale_repo.rs`). The comment there is explicit
about why: the frontend's tax figures are ignored and everything is recomputed
from first principles, because a compromised client could otherwise post
arbitrary `tax_amount_minor` values.

Two rate modes, both integer arithmetic, both rounding half-up:

| Mode | Formula | Rate = 1000 bp (10%) |
|---|---|---|
| Exclusive | `(amount × bp + 5000) / 10000` | VAT added on top |
| Inclusive | `(amount × bp + (10000+bp)/2) / (10000+bp)` | VAT extracted from the price |

`domain/money.rs::calc_tax_exclusive` holds the first. Rates are stored in basis
points on the tax rule; the seeded Bahrain rule is `VAT 10%` = 1000 bp, with a
`Zero Rate` rule at 0 bp.

**Rounding is per line, then summed** — `server_line_taxes.iter().sum()` — not
computed once on the invoice total. The two can differ by a fils or two on a
multi-line basket. This is a choice, and it is the first thing for the adviser to
ratify.

---

## 2. What the receipt prints

From `src/utils/receiptLines.ts`:

```
<store name>              if design.show_store_name
<address>                 if design.show_address
<phone>                   if design.show_phone
TRN: <tax_number>         if design.show_tax_number
CR No: <cr_number>        if design.show_tax_number
------------------------
Receipt: #<receipt_number>
<date/time, Asia/Bahrain, en-BH>
Cashier: <name>
------------------------
<item> x<qty>                     <line total>
------------------------
Discount                        - <discount>     if > 0
Subtotal (excl. VAT)              <net − tax>    if tax > 0
VAT                               <tax>          if tax > 0
TOTAL                             <net>
```

Three properties worth the adviser's attention:

- **The VAT rate is never printed** — only the word `VAT` and an amount. A reader
  cannot tell 10% from zero-rated from a mixed basket.
- **No per-rate breakdown.** A basket mixing 10% and zero-rated items prints one
  combined VAT figure, with no split of taxable versus zero-rated consideration.
- **TRN and CR share one toggle.** `design.show_tax_number` gates both (lines
  86–87). Switching it off removes the TRN from every receipt, and nothing warns
  that this may change the document's status.

---

## 3. Finding — now fixed: a whole-bill discount overstated VAT

> **Status: fixed in code.** The description below is kept because the adviser
> needs to know what the till did before, and because one decision inside the fix
> is still theirs to confirm — see "What was changed" at the end of this section.
> Sales taken before this change carry the old figures.

**Line-level discounts are handled correctly.** Tax is computed on the discounted
line, and the invariant test asserts it in those words — `tax, 180, "10% of
1.800, not of 2.000"` (`db/invariants/lifecycle/discounts.rs`).

**Bill-level discounts are not.** `finalize_sale` computes line tax first and
subtracts the whole-bill discount afterwards:

```rust
let line_total = discounted + if line.tax_inclusive { 0 } else { tax_amount };
…
let server_net = (server_post_line - cart.bill_discount_minor).max(0);
```

So `tax_total_minor` is VAT on the *pre-discount* amount, while the customer pays
the discounted total. Worked through with the existing test's own numbers
(2 × 1.000 BHD, 10% exclusive, 0.300 bill discount):

| | fils |
|---|---|
| Line subtotal | 2000 |
| VAT at 10%, computed before the discount | 200 |
| Post-line total | 2200 |
| Bill discount | −300 |
| **Net — what the customer pays** | **1900** |
| Stored `tax_total_minor` | **200** |

The receipt then renders `Subtotal (excl. VAT)` as `net − tax`:

```
Subtotal (excl. VAT)   1.700
VAT                    0.200
TOTAL                  1.900
```

200 / 1700 is **11.76%**, not 10%. The receipt's own three lines are inconsistent
with the rate the store charges.

Treating the 1.900 actually paid as VAT-inclusive consideration gives taxable
1.727 and VAT 0.173. On this sale the till therefore **overstates output VAT by
0.027 BHD — about 15.6% more VAT than the consideration supports.**

The direction is conservative for the store (it declares more VAT than the
discounted price implies, rather than less), but the figure is wrong either way,
and a VAT-registered customer reclaiming input VAT from this receipt would
reclaim an amount the consideration does not support.

This is reachable in production, not theoretical: `pos_commands.rs` exposes a
bill-discount command requiring a reason and manager approval, and the POS UI
offers it (`PosCartModals.tsx`, `PosTotalsPanel.tsx`). The existing test
`a_bill_discount_reduces_what_is_owed` asserts `net` and `discount` but does not
assert `tax` — the behaviour is deliberate and simply was never assessed for tax
correctness.

### A second defect, same root cause

A refund pays back `sale_items.line_total_minor`. Because the bill discount never
reached the lines, refunding a line from a discounted bill returned the
**undiscounted** amount — the store refunded more than it took.

### What was changed

`domain::money::apportion_bill_discount` now spreads the bill discount across the
lines and re-extracts the VAT inside each from what remains. Both defects close
together, and three properties hold that did not before:

- VAT is charged on the consideration. The worked example above now stores 0.173.
- `SUM(sale_items.line_total_minor)` equals `sales.net_total_minor`, so a refund
  returns what was actually paid for the line.
- **What the customer pays is unchanged.** These line totals are VAT-inclusive,
  so their sum is still post-line minus the discount. Only the split moved — no
  payment, tender or change calculation is affected.

Rounding uses largest-remainder so the apportioned parts sum to the discount
exactly; flooring alone would have quietly charged the customer the shortfall.
A zero-rated line takes its share of the discount and still carries no tax.

**The one decision still open for the adviser.** The discount is apportioned pro
rata across *every* line, including zero-rated ones. The alternative — charging
the whole discount against standard-rated lines — changes how much VAT the store
reclaims on a mixed basket. Pro rata is the conventional basis and is what the
code assumes; it is isolated in that one function and is a small change if you
direct otherwise. Question 1 in §5 is the place to record the ruling.

Covered by `a_bill_discount_reduces_what_is_owed`, which asserts the stored tax,
the per-line totals and their sum, plus five unit tests on the apportionment
itself.

---

## 4. What is already sound

- Tax cannot be set by the client; it is recomputed server-side on every sale.
- Integer fils throughout — no floating point anywhere in the money path.
- Line-level discounts reduce the taxable amount correctly.
- Zero-rated items carry no tax (`result.tax_total_minor, 0, "zero-rated items
  carry no tax"`).
- Receipt timestamps are rendered in `Asia/Bahrain`, not the machine's zone.
- Reprints are watermarked `DUPLICATE — NOT ORIGINAL`.
- Receipt numbers are gapless per device, proven by
  `the_receipt_sequence_has_no_gaps_across_sales_and_refunds`.

---

## 5. Questions for the adviser

1. **Bill-discount apportionment (§3).** The defect is fixed; the basis needs
   ratifying. It apportions pro rata across every line, zero-rated included —
   confirm that, or direct the discount against standard-rated lines only. Also
   confirm whether sales taken *before* the fix need correcting, since those
   carry VAT computed on the pre-discount amount.
2. **Per-line rounding (§1).** Is line-level round-half-up, summed, acceptable, or
   must VAT be computed on the invoice total?
3. **Rate on the receipt (§2).** Must the VAT rate, and a per-rate split for mixed
   baskets, appear on a retail receipt?
4. **Document title.** Receipts carry no "Tax Invoice" heading and no Arabic. Is a
   title, or Arabic text, required for the documents this till issues?
5. **TRN visibility (§2).** Should `show_tax_number` be allowed to hide the TRN at
   all, or should it be forced on once a TRN is configured?
6. **Simplified vs full tax invoice.** Is there a value threshold above which this
   receipt is insufficient and a full tax invoice is required?

---

## 6. Sign-off

```
ZANPOS VAT & Receipt Review
---------------------------
Reviewed by:       _______________   (name, firm)
Date:              _______________

§3 bill-discount apportionment basis agreed: _______________________________
Corrective action needed for past sales:     [ ] no   [ ] yes — ____________

Question 2 (rounding)      resolved: [ ] as-built  [ ] change required
Question 3 (rate shown)    resolved: [ ] as-built  [ ] change required
Question 4 (title/Arabic)  resolved: [ ] as-built  [ ] change required
Question 5 (TRN toggle)    resolved: [ ] as-built  [ ] change required
Question 6 (threshold)     resolved: [ ] as-built  [ ] change required

Receipt approved for production issue:  [ ] yes  [ ] no
Sign-off:          _______________
```
