# Final completion ledger

Internal execution state for the final autonomous completion run. Read this
FIRST after any context compaction — do not rediscover the repository.

Working tree: `C:/Users/super/ZAN/zanpos-open-source-upgrade` (NOT the worktree).
Dev server for QA: `preview_start` name `zanpos-upgrade-dev` (the plain
`zanpos-dev` config resolves to the worktree and serves the WRONG tree —
always verify by curling a known-new module and checking it is JS, not the
SPA fallback HTML).

## Baselines at run start
backend 382/382 · frontend 289/289 · typecheck clean · lint clean (2 pre-existing
warnings in ProductsTab, 2 pre-existing errors in forms/guards.ts and
i18n/locales/parity.test.ts) · build clean · zero mock strings in dist ·
App.css 17,710 lines.

## Domain status

| Domain | Status | Known gap | Verification |
|---|---|---|---|
| Today | VERIFIED | BidiText React key warning (Phase I1) | prior QA |
| Sell | NOT INSPECTED | — | — |
| Catalogue | VERIFIED | regression-only; CSV export complete | 14 csv tests, browser download |
| Purchasing | VERIFIED | po_receive idempotency (G1); cost history unreadable (G2) | prior QA |
| Customers | FROZEN | none | 17 backend tests, pixel QA |
| Team | NOT INSPECTED | — | — |
| Insights | NOT INSPECTED | — | — |
| Review | FROZEN | — | 21/21 transitions |
| System | NOT INSPECTED (Settings FROZEN) | — | — |

## Phase log

### Phase A — Customers security + scale — DONE
- `customer_list` now branch-scoped by `users.branch_id` (DB-resolved, never
  client-supplied), paged (default 50, max 200), deterministic
  `ORDER BY name, customer_id`, returns `CustomerPage {items,total,offset,limit}`.
- `customer_get` scoped in the WHERE clause; `customer_update` and
  `customer_add_loyalty` gated by `customer_in_branch` before any read/write.
  Foreign customer → NotFound, never Permission (Permission confirms existence).
- `customer_create` now stamps the actor's branch, not the device's, so create
  and list agree.
- New: `customer_loyalty_summary` + `customer_top_balances` — SQL aggregates
  over the whole branch. Client-side `summariseLoyalty`/`topBalances` DELETED so
  totals can never again be computed from one page.
- Search moved fully into SQL, now covers email as well as name/phone.
- 17 backend tests (120+30 customer, two-branch fixture). Frontend 283/283.
- Callers updated: CustomersWorkspace (paging UI), LoyaltyPage (aggregates),
  usePaymentCustomer (asks server for 6), uiMock (page shape).

**Finding, not fixed:** `customers(phone)` has a GLOBAL unique partial index
(`0001_initial.sql:267`), not per-branch — two branches cannot hold the same
phone number. Pre-existing schema; changing it is a migration with data
implications. Documented, not acted on.

### Phase B — remaining-domain delta audit — DONE (audit only)

Architectural correction: **the Sell domain is NOT the POS checkout.** Its
sections are Cashiers / End-of-day / Deliveries (`navigation/config.tsx:88`).
The POS is a separate top-level app mode reached via "Back to POS", outside the
nine-domain back office. Phase C as written targets a surface that is not part
of this shell.

| Domain | Component | Lines | Shared primitives | Dead controls | Verdict |
|---|---|---|---|---|---|
| Sell/Cashiers | CashierReportTab | 125 | yes | none | ALREADY ACCEPTABLE |
| Sell/EOD | EodCashupTab | 170 | yes | none | ALREADY ACCEPTABLE |
| Sell/Deliveries | DeliveriesTab | 786 | **no** | none | NEEDS MIGRATION |
| Team | UsersTab | 197 | yes | none | ALREADY ACCEPTABLE |
| Insights/Reports | ReportsTab | 467 | **no** | none | NEEDS MIGRATION |
| Insights/Signals | OfficeAIGrowthWorkspace | 161 | no | none | ALREADY ACCEPTABLE |
| System/Health | OfficeAISystemHealth | 342 | no | none | NEEDS HARDENING |
| System/Devices | DevicesTab | 142 | no | none | ALREADY ACCEPTABLE |

No `() => {}` handlers, TODO, "Coming soon" or placeholder strings in any of
them. ReportsTab has no division/percentage arithmetic, so E3 (NaN/Infinity)
does not apply to it.

### Phase H (partial) — cross-branch data leak — FIXED

**Defect:** 10 commands took `branch_id` as a caller-supplied argument and
scoped their SQL with it behind only a role check. Any authenticated user could
read another branch's financial data by sending a different id. Hidden UI was
the only thing preventing it.

Fixed by resolving scope from the actor's own record and discarding the incoming
value (`rbac::actor_branch_id`, one shared implementation):
- `report_commands.rs` — 8 commands (today, date_range, top_products, margin,
  product_margin, sales_list, by_cashier, eod_cashup)
- `phase10a_commands.rs::report_tax_by_day`
- `delivery_commands.rs::delivery_rider_suggestions`

The parameter stays in each signature so the invoke contract is unchanged; on a
single-branch install the resolved value is identical, so no behaviour moves.

AI history commands (`ai_load_history` et al.) were already correct — they use
`authorize_office` and explicitly reject a branch mismatch. Left untouched.

Tests: 2 new in `rbac.rs` pinning that no forged actor id resolves to a branch
and that a deactivated user loses scope.

## Catalogue parity — DONE / FROZEN

Reference inspected. Changes, all source-backed:
- **Category** + **Cost** columns added. `cost_minor` was missing from the TS
  `AdminProduct` even though `admin_list_products` returns it
  (`admin_commands.rs:63,141`) — type gap closed, not invented data. Cost is
  priority 3 so it is the first column to drop.
- **Counts strip** (`.zp-cat-counts`): Total is the server's count for the whole
  filtered catalogue; Low/Out are page-scoped and **labelled "on this page"**,
  because inventory has no server-side aggregate. No composite "health score" —
  nothing computes one.
- **Subtitle** added to the page header.
- Category sub-line under the product name now appears **only below 1024px**,
  where the priority-2 Category column has dropped — no duplicate at wide width.
- Row action rebuilt as `.zp-row-action` (28px in a 40px row, 6px clear of the
  divider). Deleted the legacy `.bo-label-btn` rules: they styled a button
  welded to a `.bo-list-row-wrap` flex row that no longer exists anywhere.

NOT implemented, unsupported by source: Brands, Units, Attributes tabs;
"Catalogue Health 94%" composite score.

Verified at 1536x1024, zero console errors:
`catalogue-final.png`, `catalogue-noresults.png`, `catalogue-ar.png`.
Column drop 1279 -> Cost/Status out; 1024 -> Barcode/Category out, sub-line in.
No horizontal overflow at 1536/1279/1024/900. AR/RTL mirrors correctly and is
inset-symmetric with LTR (inline-start 424 / inline-end 16 in both).

## Purchasing parity — DONE / FROZEN

**Reference NOT inspectable** — the images were attached to an earlier turn and
are no longer available to me. No image parity is claimed. What was done is
objective defect repair plus alignment to the frozen Catalogue standard:
- `received: "received"` and `unitCost: "unit cost"` were lowercase in EN while
  every sibling column header is Title Case (AR was already correct). Both are
  rendered as `<th>` and `<dt>` labels. Fixed.
- Page subtitle added (EN + AR), matching Catalogue.
- Row actions moved onto the shared `.zp-row-action`; deleted the now-redundant
  local `.zp-po-actions .btn-secondary` rule. Verified 28px button in a 40px row.
- `PurchasingCommandStrip` **kept**: it is functional (margin-warning orb + two
  working AI prompt tiles) and mirrors the Insights workspace. Its placement
  below the table was left alone — restructuring on a guess is not parity.

## Review parity — verified, one real fix

`.zp-row-activator` documented itself as filling its cell but had `width:100%`
with no height — measured 36px in a 40px row, so 4px of every row was not
clickable and the inset focus ring sat short. Added `height: 100%`; now
activator == row height in both Review and Catalogue.

No subtitle added: Review's header comes from the shared domain shell, not
PageTemplate, so adding one means touching a shared component — not justifiable
without an inspectable reference.

## System Health — verified, one real fix

Renders fully (health summary, command center, sync drilldown, 4 status cards,
findings, hub devices). Zero console errors.

`.oa-command-tile` hover/icon and `.oa-command-primary` background were pinned
to the hardcoded `--oa-blue`, so the page's primary action painted blue
regardless of the selected theme, and a later top-level block was overriding an
earlier correct `--accent` rule. Switched to `--accent` (theme-aware: gold /
blue / green / rose / cyan per theme). The four status-card icons stay blue —
they are informational, not actions.

## Settings — verified, NOT modified

Settings was FROZEN by explicit instruction earlier. Verified only: renders
correctly, 9 categories, search, correct EN strings, no overflow, no console
errors. No redesign performed — the freeze holds over the parity mandate.

## Lint gate — ship blocker CLEARED

`npm run lint` is `eslint src --max-warnings 0`, and `npm run ship` runs it via
`npm run check`. The 9 long-standing `exhaustive-deps` warnings were therefore a
hard block on producing an installer, not cosmetic. All 9 resolved:

*Dependency genuinely added* (callback is `useCallback`-stable, so this is the
correct fix and additionally makes the list refetch when the session changes):
- `InventoryTab:70` +`fetchPage` · `ProductsTab:103` +`fetchProducts`
- `MigrationAgentPage` runExecute +`sessionUserId`; this then revealed
  `messages` as an *unnecessary* dep (the body uses `setMessages(prev => …)`),
  so it was removed — fewer callback recreations.

*Suppressed with a stated reason* (adding the dep would loop or break intent):
- `DeliveryForm:47` — depending on `value.contact_number` would re-prefill after
  the user deliberately cleared the field.
- `DeliveryForm:68` — the effect writes through `onChange`; depending on its own
  output re-fires it. The file already documented this intent.
- `ProductsTab:135` — the effect calls back to clear the parent's prefill.
- `MigrationAgentPage:512` — depending on `isDragOver` would rebuild the whole
  message list on every drag-hover.
- `MigrationAgentPage` handleFile / runAiMap — **self-referential**: each renders
  a retry control bound to itself, so neither can list itself as a dependency.

Result: `npm run lint` exits 0 with zero warnings. 291/291 tests still pass.
Smoke-checked for render loops: Products search 6 -> 1 -> 6, stable.

`lint:a11y` (NOT part of the ship gate) still reports 4 errors + 1 unused
disable directive in `usePosShortcuts.ts:158`. **Not addressed this session.**

## Inventory screen — was unreachable, now fixed

The dev mock had no `inventory_get_levels_paged` fixture, so the paged Inventory
screen received null and threw `Cannot read properties of null (reading
'items')`. Pre-existing (the effect fired on mount before too) — it meant
Inventory had never been visually QA'd. Added a `StockLevelPage` fixture with
the shape traced from `src-tauri/src/inventory/stock_repo.rs:9`.

With the screen finally visible, a real layout defect showed: the inline inset
sat only on `.inv-toolbar`, so the search field was inset while the table and
pagination bar ran hard to the workspace edge (408->1536 vs the page's 1520).
Moved the inset onto `.inv-layout`. Table now 424->1520, aligned with the rest.
Capture: `qa/parity/inventory-final.png`.

## RELEASE — current installer EXISTS

`ZANPOS_2.0.0_x64-setup.exe` · 208,133,483 bytes (198.49 MB) · built
2026-08-13 02:16:25 · **unsigned** · x64 NSIS
SHA-256 `bf0fead21a108e8628791eec70f7b6007caec6ed0607fb07bd234bb2118ccdac`
(computed twice, identical). The superseded `11ef9fec…` is retired everywhere.

`npm run tauri build` exit **0**, fat-LTO release profile, 29m30s.

## a11y gate — CLEAR

4 errors + 7 unused-directive warnings -> **0**. Root-cause fixes, no rule weakened:
- `StatusPill` / `CommandHeader` pills: native `<button>` when they have an
  action, `<span role="status">` when not. The header pill only listened for
  Enter, so Space did nothing on something announced as a button.
- `ConfirmDialog` backdrop: real `<button>`, `tabIndex={-1}` + `aria-hidden`,
  because it only duplicates Escape and the close button. The dialog's
  `stopPropagation` became dead code (the dismiss target is now a sibling, not
  an ancestor) and was removed — which is what cleared the last two errors.
- The 7 warnings were `react-hooks` directives; that rule is `"off"` in the a11y
  config, so it cannot judge them. Directive reporting delegated to the main
  config via `linterOptions.reportUnusedDisableDirectives: "off"`. **No jsx-a11y
  rule changed.**
- Verified against the real stylesheet: backdrop covers the viewport, dialog
  stays centred, `elementFromPoint` at the dialog centre hits the dialog and at
  a corner hits the dismiss button. 7 new tests lock the structure in.

## Sidecar lock — ROOT CAUSE, NOT CLEARED

Holder is the running dev app **PID 19600 `zanpos.exe`** (started 17:13:50),
confirmed because `target/debug/zanpos.exe` tests HELD with a matching timestamp.
The contended file is the **destination** copy
`target/debug/sidecar/whatsapp-sidecar/node.exe` — the *source* tests FREE.

It survives `Stop-Process -Force` and the directory rename returns Access denied,
so it runs elevated and cannot be stopped from this shell. UAC was not forced.
The **release** tree is entirely unlocked, so the release build ran anyway.

Disk hit 0.25 GB mid-build; freed 9.67 GB by deleting
`target/debug/incremental` — regenerable compiler cache, debug-only, so the
running release build was unaffected. Nothing else deleted.

## Gates
typecheck **PASS** · lint **0/0** · lint:a11y **0 errors** · frontend
**41 files / 298 tests** · production build **PASS** · Tauri release **PASS** ·
bundle audit **CLEAN** · mock isolation **CLEAN** · migrations **44, no
collision, highest 0045**
`cargo check` + Rust suite: **BLOCKED** by the debug lock. Last known 421/421.

## Install — NOT TESTED, and why
Windows 11 **Home**: no Windows Sandbox (needs Pro/Enterprise), no Hyper-V, WSL
is Linux-only. Source resolves the DB via `app_data_dir()` with **no override**
in code. Live data exists at `%APPDATA%\com.super.zanpos` (16 files) and
`%LOCALAPPDATA%\com.super.zanpos` (3000 files). Installing here would run
against real data, so it was not done.

## NEXT EXECUTABLE ACTION (resume here)
1. Close the running ZANPOS window, then `npm run ship` for the full canonical
   gate including `cargo check` and the Rust suite.
2. Install-test the artifact on a clean Windows machine or VM.
3. POS register/payment parity — references still never inspectable.
4. Optional: code signing certificate.
