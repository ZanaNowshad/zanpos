# ZANPOS — POS Compliance Checklist

**Scope:** PCI DSS (payment security), Bahrain NBR VAT requirements, and general financial-control standards for retail POS software.
**Verdict:** One **open defect** blocks the VAT gate — a whole-bill discount is
applied after line tax, so output VAT is overstated on any discounted bill
(§2.3, and `docs/vat-receipt-review.md`). Everything else passes automated
checks; several items still require human sign-off. See the status column.

> This document previously read PASS with no qualification while marking the VAT
> breakdown ✅ VERIFIED. It was not — the breakdown is wrong whenever a bill
> discount is used. A compliance sheet that certifies an untested claim is worse
> than one that admits a gap, so rows now cite the assertion or test that backs
> them, and anything not actually verified says so.

---

## 1. Payment Security (PCI DSS Alignment)

| Requirement | Evidence | Status |
|---|---|---|
| No PAN (Primary Account Number) stored | Schema has no column for card number, PAN, or track data. `payments` table stores `payment_method` (text: cash/card/wallet) and `external_reference` (tokenised reference only). | ✅ VERIFIED |
| No CVV / CVC stored | No CVV column anywhere in schema or migration files. | ✅ VERIFIED |
| No full magnetic stripe / chip data stored | No such column exists. Card interactions are delegated to an external terminal; ZANPOS only records the outcome. | ✅ VERIFIED |
| No cardholder name stored | Not collected or stored by ZANPOS. | ✅ VERIFIED |
| API keys stored in OS credential store | `secure_store.rs` uses Windows Credential Manager (keyring v2). AI provider keys and Supabase keys are stored there, not in plaintext SQLite. | ✅ VERIFIED |
| Payment data encrypted at rest | SQLite file is not encrypted at the application layer. Full-disk encryption (BitLocker) must be enforced on the POS device. | ⏳ HARDWARE GATE |
| TLS for all payment network communication | ZANPOS does not communicate with card networks directly. The external card terminal handles TLS. Supabase sync uses HTTPS. | ✅ VERIFIED |
| PCI DSS SAQ type | ZANPOS qualifies for **SAQ P2PE** if a PCI-listed point-to-point encryption terminal is used (all card data never enters the POS software). Confirm with payment terminal vendor. | ⏳ VENDOR CONFIRM |

**Recommendation:** Enforce BitLocker on all POS devices before production. Document the card terminal vendor's PCI certification.

---

## 2. Receipt and Tax Compliance (Bahrain NBR)

### 2.1 Receipt Numbering

| Requirement | Evidence | Status |
|---|---|---|
| Sequential receipt numbers | `next_receipt_number()` in `sale_repo.rs` generates `{BRANCH}-{DEVICE}-{08d}` sequential numbers. | ✅ VERIFIED |
| No gaps in sequence within a device | `devices.next_receipt_seq` is incremented atomically by `UPDATE … RETURNING next_receipt_seq - 1`, inside the sale transaction — a rolled-back sale rolls the number back with it. Proven by `the_receipt_sequence_has_no_gaps_across_sales_and_refunds`. | ✅ VERIFIED |
| Refund receipts numbered separately | `next_refund_receipt_number()` in `refund_repo.rs` generates `{BRANCH}-{DEVICE}-REF-{08d}`. | ✅ VERIFIED |
| Void receipts | `sales.status = 'voided'` recorded; void is traceable in audit log. | ✅ VERIFIED |

**Closed — was "receipt number gaps".** This section previously described the
counter as `COUNT(*) + 1` and recorded a gap whenever a sale failed between
numbering and commit, deferring an atomic sequence to "Phase 2". That work has
landed. The counter is now a column on `devices`, incremented with
`UPDATE … RETURNING`, and `next_receipt_number` is called with the sale's own
transaction, so an abandoned sale returns its number rather than burning it.
Because the counter no longer derives from row counts, pruning old sales also
cannot cause a collision.

### 2.2 Timestamps

| Requirement | Evidence | Status |
|---|---|---|
| `sold_at` is UTC ISO 8601 | `chrono::Utc::now().to_rfc3339()` — yes. | ✅ VERIFIED |
| `business_date` is local date | `chrono::Local::now().format("%Y-%m-%d")` — correct for end-of-day cross-midnight sales. | ✅ VERIFIED |
| Receipt timestamp shown in local time | Frontend renders `sold_at` using `toLocaleTimeString()` — user sees local time. | ✅ VERIFIED |

### 2.3 VAT / Tax

| Requirement | Evidence | Status |
|---|---|---|
| VAT rate captured per line item | `tax_rule_snapshot` JSON column on `sale_items` stores `rule_id`, `rate_basis_points`, `inclusive`. | ✅ VERIFIED |
| Tax calculated correctly (exclusive) | `calc_tax_exclusive = (subtotal × rate_bp + 5000) / 10000` — integer, round-half-up. 10% on 800 fils = 80 fils. | ✅ VERIFIED |
| Tax calculated correctly (inclusive) | `(price × rate + (10000+rate)/2) / (10000 + rate)` — integer, round-half-up. | ✅ VERIFIED |
| Tax computed server-side only | `finalize_sale` recomputes every line from first principles and ignores client-supplied totals, so a compromised frontend cannot set tax. | ✅ VERIFIED |
| Zero-rated items carry no tax | Asserted directly: `result.tax_total_minor, 0, "zero-rated items carry no tax"`. | ✅ VERIFIED |
| Line-level discount reduces taxable amount | Asserted as `tax, 180, "10% of 1.800, not of 2.000"`. | ✅ VERIFIED |
| **Whole-bill discount reduces taxable amount** | **It does not.** Line tax is computed first and the bill discount subtracted afterwards, so `tax_total_minor` is VAT on the pre-discount amount. On the existing test's figures the receipt prints an implied 11.76% against a 10% rate, overstating output VAT by 0.027 BHD on a 1.900 BHD sale. See `docs/vat-receipt-review.md` §3. | ❌ **OPEN DEFECT** |
| VAT breakdown on receipt | Receipt shows "Subtotal (excl. VAT)", "VAT", "Total" when tax > 0 — but the subtotal is derived as `net − tax`, so it inherits the defect above whenever a bill discount is applied. The rate itself is never printed, and mixed-rate baskets get one combined figure with no per-rate split. | ⚠️ **REVIEW** |
| VAT registration number on receipt | `TRN: {tax_number}` printed in ESC/POS builder and HTML receipt. Note that one toggle, `show_tax_number`, hides both TRN and CR number. | ⚠️ **REVIEW** |
| Daily tax report | `report_tax_by_day` command returns daily totals with cumulative. | ✅ VERIFIED |

~~**Action required:** Add TRN (Tax Registration Number) field to `app_config` and print it on every receipt.~~ **✅ DONE** — `TRN:` label implemented in ESC/POS builder and HTML receipt; `SettingsTab` preview shows TRN from `branches.tax_number`. See row above.

### 2.4 BHD Currency Formatting

| Requirement | Evidence | Status |
|---|---|---|
| 3 decimal places (fils) | `formatMoney(amount, 3)` used throughout. Vitest tests verify BHD 0.400, BHD 1.234, etc. | ✅ VERIFIED |
| Minor units (1 BHD = 1000 fils) | All amounts stored as integers (minor units). | ✅ VERIFIED |
| No floating-point arithmetic on money | All calculations use integer minor units. `f64` only used for intermediate stock quantity, never money. | ✅ VERIFIED |

---

## 3. Cashier and Shift Controls

| Requirement | Evidence | Status |
|---|---|---|
| RBAC enforced | Every sensitive command resolves its caller from the session store via `rbac::session_actor`, never from the request payload. Enforced statically by `scripts/semgrep_validator/authorization.py`, which fails the build on any `#[tauri::command]` that neither authenticates nor appears in a documented `PRE_AUTH` list. `owner_only` is now `#[cfg(test)]` precisely so it cannot regain a shipping caller. | ✅ VERIFIED |
| Shift open/close recorded | `shifts` table with `opened_at`, `closed_at`, `opening_cash_minor`, `counted_cash_minor`. | ✅ VERIFIED |
| EOD cashup (X/Z report) | `report_tax_by_day` and shift-close flow. Z-report (closing) requires implementation verification. | ⏳ UI REVIEW |
| Cash drawer movements audited | `cash_events` table from migration 0009; `paid_in`/`paid_out` events. | ✅ VERIFIED |
| Manager approval for adjustments | `inventory_adjust_stock` and `inventory_receive_stock` require `manager_or_owner`. | ✅ VERIFIED |
| Audit log hash chain | SHA-256 chained audit log. `audit_verify_chain` command exposes verification. | ✅ VERIFIED |
| No price override without manager PIN | Price override must be confirmed in UI review. | ⏳ UI REVIEW |

---

## 4. Inventory Integrity

| Requirement | Evidence | Status |
|---|---|---|
| Stock deducted on sale | `movements::deduct_sale()` called after every `finalize_sale()` commit. Integration tested. | ✅ VERIFIED |
| Stock restored on refund | `movements::return_refund()` called after every `create_refund()` commit. Integration tested. | ✅ VERIFIED |
| Stock adjustments require manager | `inventory_adjust_stock` enforces `manager_or_owner`. | ✅ VERIFIED |
| Branch/device correctly identified | Hardcoded IDs removed; all functions receive IDs from callers or DB query. | ✅ VERIFIED |
| Low-stock alerts fire correctly | Integration test `test_low_stock_alert_fires` verifies alert when qty ≤ reorder_point. | ✅ VERIFIED |

---

## 5. Reliability

| Requirement | Evidence | Status |
|---|---|---|
| WAL mode enabled | `PRAGMA journal_mode = WAL` in `db/mod.rs`. | ✅ VERIFIED |
| Foreign keys enforced | `PRAGMA foreign_keys = ON` in `db/mod.rs`. | ✅ VERIFIED |
| Sale atomicity | `pool.begin()` + `tx.commit()` in `finalize_sale`. Stock deduction is post-commit (intentional). | ✅ VERIFIED |
| Idempotency key prevents double-charge | `sales.idempotency_key UNIQUE` constraint. Integration test `test_finalize_sale_idempotency_key_unique` verifies. | ✅ VERIFIED |
| Offline-first | All writes go to local SQLite; sync is background best-effort. | ✅ VERIFIED |
| Backup command exists | `db_backup` Tauri command (owner-only). Backup/restore integration tested. | ✅ VERIFIED |
| Restore procedure documented | `docs/backup-restore-ops.md` with step-by-step restore and drill sign-off template. | ✅ VERIFIED |

---

## 6. Outstanding Actions Before Production

| # | Item | Owner | Priority |
|---|------|-------|----------|
| 1 | ~~Add TRN field to `app_config`; print on every receipt~~ — **DONE**: `TRN:` label in ESC/POS + HTML receipt | Dev | ✅ |
| 2 | ~~UI review: VAT breakdown visible on receipt printout~~ — **DONE**: subtotal/VAT/total rows implemented | QA | ✅ |
| 3 | **Fix whole-bill discount VAT (§2.3)** — blocked on the adviser's ruling on apportionment across mixed rates | Dev + Adviser | **P0** |
| 4 | Finance/tax adviser review and sign-off — `docs/vat-receipt-review.md` | Adviser | P0 |
| 5 | UI review: Z-report (shift-close) shows required fields | QA | P0 |
| 6 | UI review: manager PIN required for price override | QA | P0 |
| 7 | Enforce BitLocker (full-disk encryption) on all POS hardware | Ops | P0 |
| 8 | Verify card terminal vendor's PCI certification (SAQ P2PE) | Ops | P0 |
| 9 | Complete restore drill on real hardware; fill in sign-off template. Bench drill done — `docs/backup-restore-ops.md` §8 | Ops | P0 |
| 10 | ~~Verify `upsert_rows` Supabase REST idempotency~~ — **DONE**: `Prefer: resolution=merge-duplicates` in `sync_v2/client.rs`, covered by `hub/rest_tests.rs` | Dev | ✅ |
| 11 | ~~Permanently-failed sync event alerting~~ — **DONE**: `dead_letter::pending_count` raises `sync.quarantined_rows` at Critical | Dev | ✅ |
| 12 | ~~Atomic receipt number (no gap on crash)~~ — **DONE**: `UPDATE … RETURNING`, see §2.1 | Dev | ✅ |
| 13 | ~~Multi-device stock reconciliation query~~ — **DONE**: ledger-vs-cache drift raises `stock.ledger_mismatch`. Running it on a nightly schedule is still ops work | Dev | ✅ |

---

## 7. Sign-Off Matrix

| Gate | Who signs | Status |
|---|---|---|
| Automated CI all-green | CI system | ✅ Automated |
| Backup/restore drill | Operations manager | ⏳ Pending on hardware (bench drill passed — `backup-restore-ops.md` §8) |
| Receipt VAT compliance review | Finance/tax adviser | ⏳ Pending — pack ready at `vat-receipt-review.md`, **one open defect** |
| Whole-bill discount VAT fix | Developer, after the adviser rules | ❌ Open |
| PCI terminal vendor cert on file | Operations manager | ⏳ Pending |
| BitLocker enabled on all POS devices | IT administrator | ⏳ Pending |
| Supabase RPC idempotency confirmed | Developer | ✅ Verified in code and test |
| TRN printed on receipts | Developer + QA | ✅ Implemented (QA sign-off pending) |
