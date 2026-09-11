# ZANPOS Production-Readiness Ledger

**Base:** `upgrade/zanpos-open-source` (`1f4738d`) + `83aa0d9` + `cb408ac`.
**Verified:** 2026-09-11, on Windows. Every row was re-read first-hand at the
cited line on *this* base.

## How this ledger was corrected

The audit that produced it originally ran against `main` (`48741d6`). `main` is
**163 commits behind** this branch, and re-checking each finding here dissolved most
of them: they were artefacts of a stale base, not defects in the shipping code.
The record of what was claimed and what survived is kept below, because "we found
seven P0s" and "five of them were already fixed" are different facts about this app.

A second correction is recorded in this pass: the D01 fix was believed verified by
SQL simulation, and one of its claims turned out to be false when the Rust actually
ran. See **D01-b**.

## Open

| ID | Sev | Area | Evidence | Finding | Status |
|---|---|---|---|---|---|
| D02 | P0 | Hub / transport | `hub/mod.rs:108,128`; `hub/rest.rs:19-22`; `hub/discovery.rs:24,60,68` | The hub binds `0.0.0.0` over **plaintext HTTP** and serves `POST /rest/v1/{table}`, writing sync tables including `users` and `audit_logs`. Authentication is strong — per-device pairing, constant-time digest comparison, `hub_unauthorized` audit events, legacy shared token deprecated — but the bearer token and every synced row still cross the LAN in cleartext, so anyone on the shop wifi can read them and replay. `discovery.rs` already advertises a `tls_fingerprint` field that nothing populates, so the intended shape is on record. | **OPEN** — needs its own design (rustls + self-signed cert minted at hub start + fingerprint pinned during pairing). Not a minimal fix. |
| OPS1 | P1 | CI / every gate | run 34543206477; base run 34189048662 at `1f4738d` fails identically | **GitHub Actions is billing-blocked on this repo.** Every job dies in 1-2s having run zero steps, the run page shows nothing, and the log endpoint 404s; the reason surfaces only through the annotations API. It is not a code signal — the pristine base commit fails the same way. The repo already diagnosed this (`f700f76`). Consequence: there is no automated gate on anything, so every claim of "tests pass" is a local run someone has to do by hand, on Windows. | **OPEN** — needs billing restored; nothing in the code can fix it |
| OPS2 | P1 | Build bootstrap | Tauri build script output, reproduced three times this session | **The Rust crate cannot be built from a clean checkout or a fresh worktree.** Three prerequisites are absent and each surfaces only as a bare `resource path X doesn't exist`, one at a time, after a full dependency compile: (1) `src-tauri/sidecar/whatsapp-sidecar/node.exe` — 91.7 MB, gitignored at `.gitignore:6`; (2) `storefront/dist` — needs `npm install && npm run build` in `storefront/`; (3) `src-tauri/sidecar/whatsapp-sidecar/node_modules` — its own `npm install`. With OPS1 blocking CI, nothing catches this, so a new machine hits three sequential dead ends with no guidance. | **OPEN** — a `scripts/bootstrap.mjs` plus a build-script message naming the remedy would close it (S) |
| B06 | P1 | Stock ledger | `sale_repo.rs` (`deduct_sale`, post-commit) | `stock_levels` is updated inside the sale transaction; `stock_movements` rows are written after commit, explicitly non-rolling-back. Levels and ledger can diverge, so stock is not reconstructible from history. Note `0056_loyalty_ledger.sql` applies exactly the convergent fix to loyalty and names `stock_movements` as the shape it copies — the same reasoning has not been applied back to the stock path itself. | **OPEN**, unchanged from the original audit |
| B02 | P1 | Loyalty write | `sale_repo.rs` (post-commit accrual) | Loyalty accrual still runs after commit (originally E05). Now logged rather than fixed. Largely superseded by the `loyalty_events` ledger — needs re-reading against it before it is worth acting on. | **NEEDS RE-READ** |
| F01 | P2 | Typography | `App.css:18-22` (Bricolage), DM Sans and JetBrains Mono blocks; `operator-ux.css:49,118,163,299` | **Four of the five declared font weights do not exist.** `@font-face` declares Bricolage Grotesque at 400/500/600/700/800 pointing at five filenames that are **byte-identical** (md5). DM Sans 300–800 are likewise one file under six names, and JetBrains Mono 400/500 under two. So every `font: 700` — and the `font: 750` at `operator-ux.css:299`, a weight no real face could serve — resolves to the 400 face and renders as **synthesised faux bold**: smeared, badly spaced, and muddy at arm's length on a counter touchscreen. The design system believes it has a weight scale; it has one weight and four aliases. | **OPEN** — ship the genuine weight files, or use the variable font (Bricolage Grotesque is variable) with a declared `font-weight` range. S |
| G01 | P2 | Startup / CSS | `dist/assets/index-*.css` = **467.71 kB** (76.66 kB gzip), measured from `npm run build` | The entry CSS is **larger than the entry JS** (199.25 kB) and is render-blocking, so it gates first paint on the till. JS chunking is done carefully — `backoffice`, `setup-wizard`, `vendor`, `OfficeAIPage`, `migration`, `jsbarcode`, `icons`, `tauri-api` are all split — and route-level CSS is split for setup-wizard, OfficeAIPage and backoffice, but the main sheet still carries the bulk. | **OPEN** — split the remaining route-specific rules out of the entry sheet. M |
| DOC1 | P1 | Stale specs | `docs/proposals/README.md:5,33-40` vs `src/i18n/locales/ar/`, `loyalty_repo.rs:39`, `report_commands.rs:48-49,60-61`, `migrations/0022_product_cost_history.sql`, `0023_sale_item_cost_snapshot.sql`, `0056_loyalty_ledger.sql` | **The feature proposals describe work that has since shipped, and would cause someone to rebuild it.** The README states it is grounded in `feature/ai-tools-suppliers-wip`, a different branch, and asserts on this base: "No i18n/RTL exists today (confirmed by search) — Proposal 02 is greenfield" (it exists, with `ar/` and `en/` locales and a `parity.test.ts` asserting every key has a real Arabic translation); Proposal 05 offers "points redemption at the till" (`loyalty_repo.rs:39` already implements `Redeem`, with insufficient-balance handling at `:215`); Proposal 03 says "Reports are revenue/VAT only — there is no COGS or margin anywhere" (`report_commands.rs` returns `gross_margin_minor` and `margin_basis_points`, backed by two cost migrations). | **OPEN** — re-ground the proposals on this branch and strike the parts already delivered, or the next person builds i18n, loyalty redemption and margin reporting a second time. S |
| E01 | P3 | Touch targets | `App.css:311` (54px), `:913` (52px), `:1117` (52px) vs `:1297` (56px, commented "T25: WCAG AA touch target for 15-inch touchscreen") | Three interactive minimum heights sit below the 56px the codebase sets as its own standard. The intent is clearly established elsewhere; these are drift, not oversight. | **OPEN** — S |

## Fixed in this pass

| ID | Sev | Finding | Fix |
|---|---|---|---|
| D01 | P0 | `sync_v2/apply.rs` remapped an inbound user whose `role_id` was unknown locally to **the owner role**, to satisfy the FK. Unknown input widened privilege to maximum. | `b98b4f2` — parks an unprivileged placeholder under the same role_id (`rbac::UNSYNCED_ROLE_PREFIX`, matched by no allowlist), kept local so it never travels. **Now compiled and tested on Windows.** |
| D01-b | P1 | **The D01 fix's own heal was broken, and its own test caught it once actually run.** `b98b4f2` stamped the placeholder with `chrono::Utc::now()`. `apply_lww` overwrites only when the local `updated_at` is strictly older than the arriving one, and a real role carries whenever it was genuinely last edited — for roles, usually long past. The placeholder therefore outranked the real role and the row never healed: the user held nothing permanently, recoverable only by editing the database. The previous ledger's claim *"the placeholder self-heals — after the real roles row lands, the user resolves to it with no further action"* was **false**; it came from SQL simulation, and `cargo test` disproved it: `left: "unsynced:R-NOT-PULLED-YET"`, `right: "regional-manager"`. | `cb408ac` — stamp the placeholder at the epoch via a named `PARKED_ROLE_STAMP` carrying the reason. The epoch loses every freshness comparison, which is what a placeholder should do. Security property unchanged; the pre-existing heal test is the regression test (failed before, passes after). |
| D06 | P1 | `cashier_cannot_perform_manager_only_operations` called `manager_or_owner` identically three times with the command name only in the failure message — and could not distinguish a denied cashier from an absent one. | `b98b4f2` — replaced with a test that walks the allowlists themselves, asserting both the admits and the refusals. |

## Dissolved on re-check — fixed before this work began

These were real on `main` and are **already fixed on this branch**. Recorded so they
are not re-reported by the next audit that starts from the wrong branch.

| ID | Was | Resolution on this base |
|---|---|---|
| C01 | P0 — `init_db` failure deleted `zanpos.db`, `-wal`, `-shm`, `-journal` and retried, so a migration checksum change destroyed the shop's data | Nuke-and-retry path is gone |
| C02 | P0 — any failed read-only probe (a transient AV or backup lock) was read as "not ours" and deleted the database | Probe-and-delete path is gone |
| D03 | P0 — applying a synced `users` row deactivated every local user sharing the username, unlogged | Now LWW-guarded; a stale row cannot deactivate a live account, and the loser yields its unique name. Regression tests present (`U-LIVE`/`U-STALE`) |
| D04 | P0 — no session: `require_role` asked "does user X hold role R?", never "is the caller X?", and `actor_user_id` was an ordinary IPC parameter | `auth_session::SessionStore` + `session_actor` resolve the caller from a session token this process issued at login; the old signature is retained only for authorization. Visible in the TS bindings, which now take `sessionToken: SessionToken` |
| B01 | P0 — retention pruned sales at 90 days and `audit_logs` at 30, destroying tax records and breaking the hash chain | Reworked behind a `PRUNABLE_SALES` guard; the code comments record the same defect |
| A05 | P1 — loyalty points accrued with nothing to spend them | `0056_loyalty_ledger.sql` adds `loyalty_events` (append-only, signed, `earn`/`redeem`/`adjust`/`opening`) with `loyalty_repo.rs` and `db/invariants/loyalty.rs` |
| A06 | P1 — loyalty synced as `MAX(local, incoming)`, so a redemption would be reverted | Same migration. Its comment diagnoses the `MAX` merge precisely and moves the total to a derived sum; `customers.loyalty_points` stays a local cache and stops travelling |
| A03 | P2 — `hub_join` had no RBAC gate | Overstated when raised: `hub_commands.rs:236-247` rejects the call once `setup_complete = 1`, so it is reachable only on an unprovisioned terminal |
| G02 | P2 — "~600 kB of duplicated font data ships in the bundle" | **Wrong, and retracted.** Vite is content-addressed: 19 source `.woff2` collapse to **9** in `dist` (measured). The duplication costs nothing at runtime. The real defect is that those duplicates are declared as distinct weights — see **F01**, which is a typography bug, not a bundle one. |

## Corrections to the original brief

| Claim | Truth |
|---|---|
| "163 `.unwrap()`/`.expect()` outside tests" | **11.** A `#[cfg(test)]`-aware scan finds 7 `unwrap` + 4 `expect`. The 88/75 figures came from `grep -v test`, which only drops lines containing the word. |
| "Frontend tests are utilities only — 24 tests in 3 files, no component tests, no jsdom" | **70 files, 503 tests, all passing.** 20 are `.tsx` component tests. |
| "No i18n, no RTL, no Arabic" | `src/i18n/` exists with `locales/`, `modalStrings.ar.ts`, and a test asserting *every* key has a real Arabic translation. |
| "Baseline is red — 2 eslint warnings" | **Green on this base.** `npm run type-check`, `npm run lint --max-warnings 0` and `npm test` all exit 0. |
| "7 migrations" | **64** `.sql` files under `src-tauri/migrations/` (an earlier revision of this ledger said 65). |

## Verification — what is actually proven

**The Rust crate cannot be built or tested on Linux.** `sha2`, `hmac`, `hex`, `base64`,
`csv`, `mysql`, `tiberius`, `calamine`, `zip` and `keyring` all sit under
`[target.'cfg(windows)'.dependencies]` (`Cargo.toml:92-123`), so a Linux `cargo test`
fails with ~155 unresolved-crate errors before running a single test.

**And CI cannot stand in for it** — see OPS1.

### Gate run on Windows, 2026-09-11, at `cb408ac`

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --all-targets -- -D warnings` | exit 0, zero warnings (6m16s) |
| `cargo test --lib` | **964 passed; 0 failed; 1 ignored** |
| `npm run check` | exit 0 — 70 files, **503 tests** passed |
| `npm run build` | exit 0 |

The Rust now compiles — the open question the previous ledger flagged. Running it is
also what exposed **D01-b**, which SQL simulation had reported as proven.

## Coverage

Complete on this base: Lens D (security), Lens A (IPC/CRUD, partial),
**Lens G (performance)**, **Lens F (visual)** and **Lens E (UX)** — first pass.
Partial: Lens B, Lens C. Lens H is largely answered by
`docs/audit/pos-ai-deepscan-gap-report.md` and `docs/proposals/01-05` already on this
branch — NBR e-invoicing QR, Arabic/RTL bilingual receipts, the purchasing loop with
cost & margin, Benefit ECR card terminal, and a promotions and loyalty rules engine.

### E/F/G — verified solid, do not re-report

The old `main`-based U01–U14 and V01–V12 lists are obsolete here. What was checked:

| Property | Evidence |
|---|---|
| Scan-to-cart is queued, not raced — a scan buffer drains serially so fast scanning cannot interleave or drop items | `hooks/usePosBarcode.ts:33-49` |
| Scan feedback is synchronous, before the IPC await, so perceived latency is independent of backend time | `usePosBarcode.ts:53` (`flashSuccess`) |
| Focus returns to the barcode input after the drain and after every modal close | `usePosBarcode.ts:47`; `pages/usePosPageState.tsx:265` |
| An unknown barcode is non-blocking: flashes, flags the manager, and tells the cashier "Keep selling." | `usePosBarcode.ts:41-43` |
| Payment is double-submit guarded | `PaymentModal.tsx:347,354` (`disabled={loading}`) |
| Payment is fully keyboard-driven — Enter to confirm, dialpad, F1/F2/F3 method select, Backspace/Delete | `usePaymentKeyboard.ts:72-77`; `PaymentModal.tsx:225-228` keeps the Enter handler off a stale closure |
| `ProductGrid` is memoised, renders a 12-card skeleton while loading, and has a teaching empty state | `ProductGrid.tsx:26,30,42-50` |
| Offline state surfaces the sync worker's actual last error, not a bare "Offline" | `SyncChip.tsx:102-104` |
| Design tokens are real: 401 CSS custom properties; only 3 `transition: all`; 103 `focus-visible` rules | tree scan |
| No N+1 IPC — no `invoke` inside a loop or `.map()` anywhere in `src/` | tree scan |
| Product listing is paginated (default 50) and JS is route-chunked; the till entry chunk is 199 kB / 56.9 kB gzip | `commands.ts:257-262`; `npm run build` output |
