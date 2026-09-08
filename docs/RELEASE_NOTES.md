# ZANPOS — Release Notes

# 2.0.1

**Artifact:** `ZANPOS_2.0.1_x64-setup.exe`
**SHA-256:** `6f107a19e035bd8bfeaefdc7599f821f1d4b3e594fa26e703d66710de3351e3e`
**Built:** 2026-09-08 from commit `e148185` · x64 NSIS · unsigned (Authenticode);
updater signature present and verified

Supersedes an earlier 2.0.1 of the **same filename**,
`21216702d8a1ffee92a0bd5a015d7098974d4b98004801aa7dd1cb5957a1ec41`, which
predates the VAT fix below and must not be distributed. Check the hash, not the
name.

## Read this first if you file VAT

**How much VAT the till records on a discounted bill has changed.** Take this to
whoever prepares your returns before installing.

When a discount was applied to a whole bill, VAT was calculated on the total
*before* the discount came off. The customer paid the discounted price, but the
sale recorded tax on the higher figure. On a 2.000 basket at 10% with 0.300 off,
the customer paid 1.900 and the receipt read:

```
Subtotal (excl. VAT)   1.700
VAT                    0.200
TOTAL                  1.900
```

0.200 on 1.700 is 11.76%, not 10%. VAT is now charged on what the customer
actually pays, so that sale records 0.173 and the receipt adds up at the rate on
the shelf. Discounts applied to a single line were always correct; only
whole-bill discounts were affected.

Two consequences worth raising with your adviser:

- **Past sales keep their old figures.** Nothing is rewritten. If whole-bill
  discounts were used in a period you have already filed, the output VAT declared
  was too high, and whether that needs correcting is their call.
- **Where the discount lands on a mixed basket.** When one basket contains both
  standard-rated and zero-rated goods, the discount is spread across every line
  in proportion to its value. That is the conventional treatment; your adviser
  may direct otherwise. `docs/vat-receipt-review.md` is written for them.

**Refunds are also corrected.** A refund pays back the line's recorded total, and
because the bill discount never reached the lines, refunding an item from a
discounted sale returned more than the customer had paid for it.

## Also in this release

- Backup guidance corrected. The scheduled task in the operations runbook copied
  the database file directly, which silently loses every sale still held in the
  write-ahead log — in testing it lost 25 of 25. It now takes a proper snapshot
  and checks it before copying. The application's own backup command was always
  correct; only the documented manual procedure was wrong. See
  `docs/backup-restore-ops.md`.
- Compliance and release documentation rewritten against the code rather than
  against earlier documentation, after several entries were found asserting
  things that were no longer true.

## Known limitations

Unchanged from 2.0.0 below, except that the install path remains untested and
Windows SmartScreen will still warn, as no code-signing certificate is
configured.

---

# 2.0.0

**Artifact:** `ZANPOS_2.0.0_x64-setup.exe`
**SHA-256:** `bf0fead21a108e8628791eec70f7b6007caec6ed0607fb07bd234bb2118ccdac`
**Built:** 2026-08-13 02:16:25 · unsigned · x64 NSIS

Supersedes the earlier build `11ef9fecfec274209a20b804a1dc28a2b9e9c0051c0edc5dec8d3142650a6dd2`, which must not be used.

## What changed in this build

### Accessibility
Clickable status pills in the command header and `StatusPill` were spans with
hand-rolled key handlers; the header's only listened for Enter, so Space — which
every user expects on something announced as a button — did nothing. Both now
render a native `<button>` when they have an action and a plain
`<span role="status">` when they do not.

The confirm dialog dismissed on backdrop click through a handler on a
presentational `<div>`, reachable by mouse only. The backdrop is now a real
button kept out of the tab order and the accessibility tree, since it only
duplicates Escape and the close button. The dialog's `stopPropagation` became
dead code in the process and was removed.

`lint:a11y` now passes with zero errors.

### Catalogue
Category and Cost columns added. `cost_minor` was missing from the TypeScript
`AdminProduct` even though `admin_list_products` already returned it. A counts
strip reports the server's total for the filtered catalogue, with low/out-of-stock
figures explicitly labelled "on this page" because inventory has no server-side
aggregate.

### Inventory
The screen was unreachable: the dev mock had no `inventory_get_levels_paged`
fixture, so it threw on `.items` and had never been visually reviewed. With it
visible, the page inset turned out to sit only on the toolbar, leaving the table
and pagination running hard into the workspace edge. Both fixed.

### Purchasing
Two EN column headers were lowercase while every sibling was Title Case. Row
actions moved onto the shared row-action style.

### System Health
The primary action tile was pinned to a hardcoded blue through a rule overriding
an earlier correct one, so it ignored the selected theme. It now follows the
theme accent.

### Correctness
Nine `react-hooks/exhaustive-deps` warnings resolved — four by adding a genuinely
missing dependency (which then exposed an unnecessary one), five suppressed with
stated reasons where adding the dependency would loop or break intent, including
two self-referential callbacks that render retry controls bound to themselves.

## Known limitations

- **Unsigned.** SmartScreen will warn.
- **Install not tested** — no isolated environment on this machine. See INSTALL.md.
- **Rust test suite not re-run in this session** — a running elevated dev process
  holds `src-tauri/target/debug`, which `cargo test` must write to. Last known
  result 421/421.
- POS register and payment-dialog visual parity remain uncompared; the reference
  images were never available in an inspectable form.
