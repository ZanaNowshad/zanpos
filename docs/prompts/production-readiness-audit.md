# ZANPOS — Total Production-Readiness Audit & Polish

> Paste this whole file as the opening prompt of a fresh Claude Code session on the
> ZANPOS repo. It is self-contained and resumable: if the session ends mid-way,
> paste it again — the agent re-reads `docs/audit/LEDGER.md` and continues.

---

## Your mission

Take ZANPOS from "feature-complete demo" to **production-ready retail software** that a
real shop in Bahrain runs eight hours a day, on one machine, offline, with money on the
line and an auditor able to ask questions two years later.

You will do this in two phases, in order, and you will not skip Phase 1:

1. **AUDIT** — a surgical, exhaustive, file-by-file sweep of the entire app that produces
   a written, ID'd, prioritised findings ledger.
2. **IMPLEMENT** — fix the findings in prioritised waves, each wave verified and committed
   separately, CI never left red.

Do not start editing code before the ledger exists. An unwritten audit is an audit you
will forget halfway through.

---

## The system you are auditing (measured, not assumed — reconcile these counts yourself)

Tauri v2 desktop POS for Bahrain retail. React 19 + TypeScript + Vite frontend, Rust +
sqlx + SQLite backend, local-first, syncs to Supabase.

| Surface | Size |
|---|---|
| Rust | ~37,700 LOC, 26 command modules, **172 `#[tauri::command]`**, 104 `#[test]` |
| TypeScript | ~21,000 LOC, 4 pages, **52 components**, 1 IPC binding file (`src/tauri/commands.ts`), 24 tests in 3 files |
| Database | 33 tables, 7 migrations, WAL, FK enforced |
| Frontend tests | **utilities only** — `money`, `adminChatClear`, `posProductFilters`. No component tests, node env, no jsdom |
| Hygiene signals | 88 `.unwrap()` and 75 `.expect()` outside tests; **98 `let _ =`** (silently discarded Results) |
| Largest files | `ai/tools.rs` 6230, `migration_commands.rs` 4169, `admin_commands.rs` 1940, `sale_repo.rs` 1224, `MigrationAgentPage.tsx` 1218, `PosPage.tsx` 1206 |
| i18n | **None.** No i18n library, no RTL, no Arabic — in a market where tax invoices are commonly expected in Arabic |

Domain entities in the schema: `products`, `product_prices`, `product_barcodes`,
`categories`, `customers`, `sales`, `sale_items`, `payments`, `refunds`, `refund_items`,
`shifts`, `cash_events`, `no_sale_events`, `users`, `roles`, `devices`, `branches`,
`stock_levels`, `stock_movements`, `held_carts`, `delivery_orders`, `ghost_barcodes`,
`unknown_barcodes`, `tax_rules`, `app_config`, `audit_logs`, `sync_watermark`,
`import_history`, `undo_records`, `ai_actions`, `ai_chat_history`, `ai_runs`,
`ai_run_undo_log`.

### Prior audits — re-verify, do not trust

Three gap docs exist from 2026-06-07. Significant UX and engine work landed **after** them,
so many entries may already be fixed and some may have regressed.

- `docs/superpowers/ENTERPRISE_GAPS.md` — E01–E15 (shift double-open race, silent audit
  failures, post-commit loyalty write, sync idempotency…)
- `docs/superpowers/UX_GAPS.md` — U01–U14 (empty states, payment Enter key, error copy,
  held-cart labels, offline messaging, printer feedback…)
- `docs/superpowers/VISUAL_GAPS.md` — V01–V12 (modal-out animation, hardcoded hex, emoji
  as icons, `transition: all`, missing skeletons…)

**For every single entry: open the cited file, read the current code, and mark it
`STILL OPEN`, `FIXED`, or `PARTIAL` with the evidence line.** Carry the open ones into the
new ledger with their original IDs so history is traceable. Do not re-litigate the ones
already marked "✓ Safe" unless the code changed.

---

## PHASE 1 — THE AUDIT

Read every file that matters. Not a sample — the whole surface. Where breadth beats depth,
fan out with `Explore` subagents (one per audit lens below), but you own the synthesis:
every finding must be one you can point at a line for.

### Lens A — The CRUD completeness matrix (this answers "what CRUD is missing where")

Build a real matrix. For **each of the 33 entities**, check every cell across all three
layers — Rust command → TS binding in `src/tauri/commands.ts` → a UI surface a human can
actually reach:

| Dimension | The question |
|---|---|
| Create | Can it be created? With validation on required fields, uniqueness, and money bounds? |
| Read / Detail | Can a single record be inspected in full? |
| List | Paginated? Sorted? Or does it `SELECT *` and render 10,000 rows into the DOM? |
| Search / Filter | By the fields a shopkeeper would actually search by? |
| Update | Every field that should be editable, editable? Any field silently uneditable after create? |
| Delete / Archive | Hard delete, soft delete, or nothing? Is hard delete FK-safe? Should it be archive instead (it almost always should for anything a sale references)? |
| Bulk | Bulk edit/import/export where volume demands it? |
| Undo | Reversible where reversal matters? |
| Audit | Does the mutation write an `audit_logs` entry with the correct actor? |
| Sync | Does the mutation mark the row for sync? Does the conflict strategy match the entity class? |
| RBAC | Is the role gate enforced **server-side in Rust**, not merely hidden in the UI? |
| Offline | Does it work with no network, and reconcile after? |

Flag every gap as its own finding. Pay special attention to the asymmetries: a Rust command
with no TS binding (dead backend), a TS binding with no UI (unreachable feature), a UI
control with no RBAC check behind it (privilege hole), a create with no corresponding edit
or delete (data roach motel).

### Lens B — Data integrity & money correctness (highest severity class)

- Every money path is `i64` minor units. Hunt any `f64`/`number` arithmetic touching money,
  any `parseFloat`, any `toFixed`, any rounding that isn't half-up on integers. The one
  sanctioned float is `apply_price` percent math in `ai/engine/mod.rs` — verify it stays
  the only one and that it rounds then clamps.
- VAT: basis points, integer math, correct inclusive/exclusive handling, correct rounding
  at the line vs. total level. Verify the receipt's Subtotal + VAT = Total exactly, for
  discounts, mixed tax rates, and refunds — including partial refunds of discounted lines.
- Every multi-statement write is inside a transaction. Hunt post-commit writes (the E05
  loyalty-points class of bug) across all repositories.
- Concurrency: every SELECT-then-INSERT that should be a DB constraint (E01 shift race is
  the archetype — look for its siblings in receipt numbering, stock levels, held carts,
  cash events, device registration).
- Sequences and idempotency: receipt numbers, refund receipt numbers, sync outbox keys.
- Audit chain: unbroken, verifiable, and **never silently skipped**. Every one of the 98
  `let _ =` sites gets a verdict: legitimately best-effort *with logging*, or a swallowed
  failure that must propagate.
- Stock: can it go negative? Should it? Is that a policy flag? Are movements ledgered such
  that current stock is reconstructible from history?

### Lens C — Failure modes & resilience

Every `.unwrap()` and `.expect()` outside tests (163 of them) gets triaged: provably
infallible, or a panic that takes down a cashier's till mid-sale. In a POS, a panic during
`finalize_sale` is money lost and a customer standing there.

Then walk the failure matrix: printer unplugged mid-receipt; DB locked; disk full;
Supabase unreachable for three days; WhatsApp sidecar missing or dead; clock skew across
devices; power loss between payment capture and sale commit; corrupted WAL; migration
version mismatch on an older DB (`app_stderr.txt` records exactly this panic —
`Migration(VersionMismatch(23))` at `lib.rs:48` — a hard startup crash with no operator
guidance is a P0 in shipped software).

For each: does it fail loudly, safely, and recoverably, and does the person standing at the
till know what to do next?

### Lens D — Security & compliance

- RBAC enforced server-side on all 172 commands. Enumerate any unguarded mutation.
- PIN handling, lockout, session/idle timeout, manager-override token lifetime and replay.
- Secrets: keyring only, never SQLite, never logs. Verify no key material reaches
  `tracing` output or the AI chat history.
- The AI back office writes to the live DB — confirm every mutation path is confirmation-
  gated, audited with `actor_type = "ai"`, undoable, and RBAC-checked at execution time,
  not just at proposal time. Prompt-injectable content (product names, customer notes,
  imported CSVs) must not be able to escalate into an unconfirmed write.
- CSP, sidecar token, IPC allowlist.
- Bahrain NBR VAT invoice requirements against `docs/compliance-checklist.md`: TRN, the
  simplified vs. full tax invoice distinction, sequential numbering, retention period, and
  **whether Arabic-language invoice content is required** — currently there is no i18n at
  all, which is a compliance question, not just a UX one.
- PCI: confirm no PAN/track data anywhere, including logs and backups.

### Lens E — UX, at the counter, under pressure

Judge every flow by: *a queue of six customers, a cashier trained for twenty minutes, a
15-inch touchscreen, one hand holding a scanner.*

Walk each end-to-end: first launch → setup wizard → login → open shift → scan → discount →
hold/resume → split payment → print → refund → no-sale → safe drop → X report → close shift
→ EOD cashup. Then back office: products, categories, customers, inventory, stock take,
deliveries, users, devices, reports, settings, migration/import, AI office.

Count the taps in each. Flag every dead end, every state with no next action, every error
message written for a developer instead of a shopkeeper, every destructive action with no
undo, every long operation with no progress, every empty state that teaches nothing, every
keyboard path that forces a mouse. Verify focus management and Escape/Enter behaviour in
all modals, and touch targets ≥ 56px.

### Lens F — Visual & interaction system

Design-token discipline (hardcoded hex, ad-hoc transition durations, emoji standing in for
icons), light/dark parity, loading skeletons vs. layout shift, modal enter *and* exit,
consistent motion language, typography scale, focus-visible rings, contrast ratios at
WCAG AA, and behaviour at the real window sizes this app runs at.

### Lens G — Performance at real data volumes

Assume 20,000 products, 500 sales/day, three years of history. Find: unindexed queries in
hot paths, `SELECT *` into unbounded lists, N+1 IPC round-trips, unvirtualised grids,
re-render storms in `PosPage.tsx`/`CartPanel.tsx`, blocking work on the UI thread, bundle
size and lazy-chunk correctness, scan-to-cart latency, and startup time. Measure where you
can; state the method when you do.

### Lens H — Standard practice this app is missing

Two checklists. For each item, decide **present / partial / absent**, and if absent decide
**needed / not needed for v1**, with a one-line reason. Do not implement blindly.

*Engineering standard practice:* structured logging with rotation and a user-reachable log
export; crash reporting; a real backup schedule and a **tested** restore drill; DB integrity
check on startup; migration rollback strategy; graceful shutdown; health/diagnostics screen;
feature flags; versioned IPC contract; component-level frontend tests (there are none —
jsdom isn't even configured); E2E smoke test of the sale path; code signing and notarised
installer; updater rollout with rollback; release checklist; runbook for the five most
likely support calls; data export for the shop's accountant.

*Retail POS domain features:* price overrides with manager approval; promotions and mix-and-
match rules; weighted/scale barcodes (price-embedded EAN-13); unit of measure; cost price,
margin and profit reporting; supplier records, purchase orders and goods receipt; stock
transfers between branches; batch/expiry/serial tracking; layaway and quotations; store
credit and gift cards; returns without a receipt; tax-exempt customers; blind cashup; cash
drawer denomination counting; Z-report locking; receipt reprint and email/PDF delivery;
customer display; loyalty redemption (points are earned — can they be spent?); staff
commission; barcode label templates; stock-take variance approval; supplier returns;
multi-till reconciliation.

### Ledger format — write this before you touch code

Create `docs/audit/LEDGER.md`. One row per finding, and this file is the single source of
truth for the whole engagement:

```
| ID | Lens | Area | Severity | Evidence (file:line) | Finding | Fix | Effort | Status |
```

- **ID**: `A01`, `B01`… Carry forward existing IDs (`E01`, `U03`, `V07`) where the finding
  is the same one.
- **Severity**: `P0` loses money, corrupts data, breaks compliance, or crashes the till ·
  `P1` blocks a real workflow or hides a failure · `P2` friction and polish · `P3` nice to have.
- **Evidence**: a real `file.rs:123` you have read. No finding without one.
- **Fix**: the specific change, not "improve error handling".
- **Effort**: `S` < 1h · `M` a few hours · `L` a day+ · `XL` needs its own design doc.
- **Status**: `OPEN` → `IN PROGRESS` → `DONE (commit sha)` → or `DEFERRED (reason)`.

Also write `docs/audit/CRUD-MATRIX.md` (Lens A's grid, entity × dimension, one cell per
verdict) and `docs/audit/MISSING-PRACTICE.md` (Lens H's two checklists with verdicts).

Then post a summary to me: counts by severity, the ten findings that scare you most, and
your proposed wave plan. **Wait for my go-ahead before Phase 2.**

---

## PHASE 2 — THE IMPLEMENTATION

Work in waves. One wave = one focused branch of work = one or more commits = one verified
green checkpoint. Update the ledger row status as you go — the ledger and the code move
together, in the same commit.

**Wave order is not negotiable.** Correctness before comfort:

0. **Baseline** — get `npm run check` and the Rust suite green *before* changing anything,
   so you know what you broke. Delete the four zero-byte junk files at repo root (`10)`,
   `bool`, `c.toUpperCase())`, `null`) and decide whether the 16 `qa-*.png` screenshots
   belong in git.
1. **P0 data integrity & money** — Lens B, plus the startup-crash class from Lens C.
2. **P0/P1 security & audit completeness** — Lens D.
3. **P1 failure handling & observability** — the rest of Lens C: unwraps, swallowed
   Results, logging, operator-legible errors.
4. **P1 CRUD gaps** — Lens A, highest-traffic entities first.
5. **P1/P2 UX flows** — Lens E, counted by taps saved.
6. **P2 visual system** — Lens F, tokens first so later fixes inherit them.
7. **Missing features** — Lens H, only the ones we agreed are v1-necessary, each as its own
   design-then-build.
8. **Release engineering** — tests, CI, signing, updater, runbooks, docs.

### Rules of engagement

- **Verify before you claim.** After every wave: `npm run check` (type-check → lint →
  test), and from `src-tauri/`: `cargo fmt --all -- --check`, `cargo clippy --all-targets
  -- -D warnings`, `cargo test --lib`. Paste the real output. If something fails, say so —
  never report a wave green that isn't.
- **Every P0/P1 fix ships with a test that fails before it and passes after.** A concurrency
  fix needs a concurrency test. A money fix needs a money test. Frontend logic fixes need
  the jsdom setup that doesn't exist yet — that's Wave 8 work you may pull forward.
- **Migrations are append-only.** New numbered file (`0008_…`), never edit a shipped one.
  Every migration must be safe on a populated production DB, and you state what happens to
  existing rows.
- **Money invariants are load-bearing.** `i64` minor units, integer math, half-up rounding,
  no floats. If a fix seems to need a float, you have the wrong fix.
- **Don't break the engine.** `src-tauri/src/ai/engine/` (selectors, batching, run ledger,
  undo) is recent, deliberate, and tested. Extend it; don't rewrite it.
- **No new dependencies** without naming the alternative you rejected and why. This app
  ships with seven runtime deps. Keep it that way unless the case is overwhelming.
- **Scope discipline.** Fix findings from the ledger. If you discover something new, add it
  as a ledger row — don't silently expand a wave into a rewrite. Refactor only where a
  finding requires it.
- **Commit per wave**, message format `fix(area): W<n> — <what changed>`, with the ledger
  IDs closed listed in the body. Push to the session's designated branch.
- **Report honestly.** If a fix is partial, mark it `PARTIAL` and say what's left. If a
  finding turns out to be a non-issue on closer reading, mark it `INVALID` with the reason.
  A short accurate report beats a long optimistic one.
- **Escalate the genuinely ambiguous.** Policy questions (should stock go negative? are
  Arabic receipts required? hard delete or archive?) are the owner's call, not yours — batch
  them and ask, then keep working on everything that doesn't depend on the answer.

### Done means

- Every P0 is `DONE` with a test and a commit sha in the ledger.
- Every P1 is `DONE` or `DEFERRED` with a written reason I've seen.
- `npm run check` green, `cargo clippy -D warnings` green, `cargo test --lib` green,
  full Tauri build succeeds.
- `docs/audit/LEDGER.md` reflects reality, and `README.md` / `AGENTS.md` reflect the app as
  it now is.
- A closing report: what changed, what's left, what I should decide next, and an honest
  answer to "would you run a shop on this tomorrow?"

Begin with Phase 1. Reconcile the counts in the table above against the real repo first —
if they've drifted, say so, and audit what's actually there.
