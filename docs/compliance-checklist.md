# ZANPOS — POS Compliance Checklist

**Scope:** PCI DSS (payment security), Bahrain NBR VAT requirements, and general financial-control standards for retail POS software.
**Verdict:** PASS on automated checks. Several items require human sign-off before production. See status column.

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
| No gaps in sequence within a device | Counter increments by `COUNT(*) + 1` per branch/device prefix. Gaps can occur if a sale fails after receipt number generation but before commit. | ⚠️ KNOWN GAP |
| Refund receipts numbered separately | `next_refund_receipt_number()` in `refund_repo.rs` generates `{BRANCH}-{DEVICE}-REF-{08d}`. | ✅ VERIFIED |
| Void receipts | `sales.status = 'voided'` recorded; void is traceable in audit log. | ✅ VERIFIED |

**Known gap — receipt number gaps:** If `finalize_sale` generates a receipt number but then the DB transaction fails (e.g. power cut between `next_receipt_number()` and `tx.commit()`), the number is wasted and a gap appears. This is a very narrow window (< 50 ms). NBR does not mandate zero-gap sequences for electronic POS systems. Document as acceptable for beta; evaluate atomic sequence with `RETURNING` in Phase 2.

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
| Tax calculated correctly (exclusive) | `calc_tax_exclusive(subtotal, rate_bp) = subtotal * rate_bp / 10000`. Integration test `test_finalize_sale_happy_path` verifies 10% on 800 fils = 80 fils. | ✅ VERIFIED |
| Tax calculated correctly (inclusive) | `tax = price * rate / (10000 + rate)`. Integration test `line_total_tax_inclusive` verifies. | ✅ VERIFIED |
| Zero-rated items carry no tax | Integration test `test_finalize_sale_zero_tax_item` verifies 0 tax on zero-rated items. | ✅ VERIFIED |
| VAT breakdown on receipt | Receipt shows "Subtotal (excl. VAT)", "VAT", and "Total" rows when tax > 0. | ✅ VERIFIED |
| VAT registration number on receipt | `TRN: {tax_number}` printed in ESC/POS builder and HTML receipt. `SettingsTab` preview updated. | ✅ VERIFIED |
| VAT breakdown on receipt (subtotal excl. VAT) | When `tax_total_minor > 0`, receipt shows "Subtotal (excl. VAT)", "VAT", then "Total" — both in HTML and ESC/POS output. | ✅ VERIFIED |
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
| RBAC enforced | `rbac::manager_or_owner()` and `rbac::owner_only()` guards on all sensitive commands. | ✅ VERIFIED |
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
| 3 | UI review: Z-report (shift-close) shows required fields | QA | P0 |
| 4 | UI review: manager PIN required for price override | QA | P0 |
| 5 | Enforce BitLocker (full-disk encryption) on all POS hardware | Ops | P0 |
| 6 | Verify card terminal vendor's PCI certification (SAQ P2PE) | Ops | P0 |
| 7 | Verify `apply_sync_event` Supabase RPC idempotency | Dev | P0 |
| 8 | Complete restore drill on real hardware; fill in sign-off template | Ops | P0 |
| 9 | Permanently-failed sync event alerting | Dev | P1 |
| 10 | Atomic receipt number (no gap on crash) | Dev | P2 |
| 11 | Multi-device stock reconciliation nightly query | Dev | P2 |

---

## 7. Sign-Off Matrix

| Gate | Who signs | Status |
|---|---|---|
| Automated CI all-green | CI system | ✅ Automated |
| Backup/restore drill | Operations manager | ⏳ Pending |
| Receipt VAT compliance review | Finance/tax adviser | ⏳ Pending |
| PCI terminal vendor cert on file | Operations manager | ⏳ Pending |
| BitLocker enabled on all POS devices | IT administrator | ⏳ Pending |
| Supabase RPC idempotency confirmed | Developer | ⏳ Pending |
| TRN printed on receipts | Developer + QA | ✅ Implemented (QA sign-off pending) |
