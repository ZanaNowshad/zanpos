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
| D02 | P0 | Hub / transport | `hub/mod.rs:108,128`; `hub/rest.rs:19-22`; `hub/discovery.rs:24,60,68` | The hub binds `0.0.0.0` over **plaintext HTTP** and serves `POST /rest/v1/{table}`, writing sync tables including `users` and `audit_logs`. Authentication is strong — per-device pairing, constant-time digest comparison, `hub_unauthorized` audit events, legacy shared token deprecated — but the bearer token and every synced row still cross the LAN in cleartext, so anyone on the shop wifi can read them and replay. `discovery.rs` already advertises a `tls_fingerprint` field that nothing populates, so the intended shape is on record. | **DESIGNED, CODE OPEN** — [`docs/proposals/06-hub-tls-fingerprint-pinning.md`](../proposals/06-hub-tls-fingerprint-pinning.md) specifies cert minting and persistence, the rustls acceptor, populating the `tls_fingerprint` discovery field, pinning at pairing behind an operator-verified confirmation code, a three-state `hub_transport` compatibility path so no shop breaks on upgrade, the four dependencies with rejected alternatives, one additive migration, and a 12-case test plan. Sequenced so steps 1–2 land with no behaviour change. |
| OPS1 | P1 | CI / every gate | run 34543206477; base run 34189048662 at `1f4738d` fails identically | **GitHub Actions is billing-blocked on this repo.** Every job dies in 1-2s having run zero steps, the run page shows nothing, and the log endpoint 404s; the reason surfaces only through the annotations API. It is not a code signal — the pristine base commit fails the same way. The repo already diagnosed this (`f700f76`). Consequence: there is no automated gate on anything, so every claim of "tests pass" is a local run someone has to do by hand, on Windows. | **OPEN** — needs billing restored; nothing in the code can fix it |
| OPS2 | P1 | Build bootstrap | Tauri build script output, reproduced three times this session | **The Rust crate cannot be built from a clean checkout or a fresh worktree.** Three prerequisites are absent and each surfaces only as a bare `resource path X doesn't exist`, one at a time, after a full dependency compile: (1) `src-tauri/sidecar/whatsapp-sidecar/node.exe` — 91.7 MB, gitignored at `.gitignore:6`; (2) `storefront/dist` — needs `npm install && npm run build` in `storefront/`; (3) `src-tauri/sidecar/whatsapp-sidecar/node_modules` — its own `npm install`. With OPS1 blocking CI, nothing catches this, so a new machine hits three sequential dead ends with no guidance. | **OPEN** — a `scripts/bootstrap.mjs` plus a build-script message naming the remedy would close it (S) |
| F01 | P2 | Typography | `App.css:18-22` (Bricolage), DM Sans and JetBrains Mono blocks; `operator-ux.css:49,118,163,299` | **Four of the five declared font weights do not exist.** `@font-face` declares Bricolage Grotesque at 400/500/600/700/800 pointing at five filenames that are **byte-identical** (md5). DM Sans 300–800 are likewise one file under six names, and JetBrains Mono 400/500 under two. So every `font: 700` — and the `font: 750` at `operator-ux.css:299`, a weight no real face could serve — resolves to the 400 face and renders as **synthesised faux bold**: smeared, badly spaced, and muddy at arm's length on a counter touchscreen. The design system believes it has a weight scale; it has one weight and four aliases. **Measured, not inferred:** rendering `Total BHD 12.500` at 48px from the shipped file gives 355.33px at weight 400 and 364.67px at weight 700 — and a control that declares *only* a 400 face and asks for 700, forcing known synthesis, gives **exactly 364.67px too**. Blink synthesises from the face's real OS/2 weight class regardless of what the `@font-face` descriptor claims, so the declaration buys nothing. Every bold in the app is a 2.6% algorithmic smear of the regular cut. | **OPEN** — needs the genuine weight files, or the variable font (Bricolage Grotesque is variable) with a declared `font-weight` range. Deliberately *not* "fixed" by collapsing the five declarations into one: measurement shows that renders identically, so it would tidy the CSS while leaving the app with no real bold, and read as done. S once the files are to hand. |
| G01 | ~~P2~~ **P3** | Maintainability (was: startup) | `src/App.css` — 18,949 lines / 586 kB source; `dist/assets/index-*.css` 467.71 kB minified | **Severity corrected downward, and the finding reframed, after measuring.** It was raised as a startup cost — "render-blocking, gates first paint". That reasoning is imported from the web and does not hold here: Tauri serves the frontend from embedded local assets, so there is no transfer and gzip never applies. The CSS is already esbuild-minified (`vite.config.ts:39`), leaving a parse cost of tens of milliseconds, once per shift. Not a user-visible defect. What is real is the **maintainability** of a single 18,949-line stylesheet against the repo's own 500-line rule: it is ordered by development phase, not by concern, so till and back-office rules interleave (a payment-modal rule at `:3740` sits between inventory at `:3726` and the receipt editor at `:3775`). That interleaving is also why splitting it is not the mechanical M-effort job first estimated — every move risks a cascade-order change, with 145 components and no visual regression tests to catch one. | **OPEN at P3** — split by concern incrementally, newest sections first, not as one refactor |
| DOC1 | P1 | Stale specs | `docs/proposals/README.md:5,33-40` vs `src/i18n/locales/ar/`, `loyalty_repo.rs:39`, `report_commands.rs:48-49,60-61`, `migrations/0022_product_cost_history.sql`, `0023_sale_item_cost_snapshot.sql`, `0056_loyalty_ledger.sql` | **The feature proposals describe work that has since shipped, and would cause someone to rebuild it.** The README states it is grounded in `feature/ai-tools-suppliers-wip`, a different branch, and asserts on this base: "No i18n/RTL exists today (confirmed by search) — Proposal 02 is greenfield" (it exists, with `ar/` and `en/` locales and a `parity.test.ts` asserting every key has a real Arabic translation); Proposal 05 offers "points redemption at the till" (`loyalty_repo.rs:39` already implements `Redeem`, with insufficient-balance handling at `:215`); Proposal 03 says "Reports are revenue/VAT only — there is no COGS or margin anywhere" (`report_commands.rs` returns `gross_margin_minor` and `margin_basis_points`, backed by two cost migrations). | **OPEN** — re-ground the proposals on this branch and strike the parts already delivered, or the next person builds i18n, loyalty redemption and margin reporting a second time. S |
| CSS1 | P3 | Dead CSS | `App.css:903` (`.cart-line-wrap`), `:1108` (`.numpad-display`) | Both selectors have **zero references** in any `.ts`, `.tsx` or `.html` in `src/`, and no dynamic class construction builds either name (the only template-literal class names are `startup-`, `zp-caps-`, `zp-cap-`, `oa-pulse-`). They are dead rules. Found while checking E01 — two of its three "touch targets" turned out to style nothing at all. A sample of two says little on its own, but in an 18,949-line sheet it is worth a systematic pass. | **OPEN** — run a coverage pass over `App.css` and delete what nothing renders. M |

## Fixed in this pass

| ID | Sev | Finding | Fix |
|---|---|---|---|
| D01 | P0 | `sync_v2/apply.rs` remapped an inbound user whose `role_id` was unknown locally to **the owner role**, to satisfy the FK. Unknown input widened privilege to maximum. | `b98b4f2` — parks an unprivileged placeholder under the same role_id (`rbac::UNSYNCED_ROLE_PREFIX`, matched by no allowlist), kept local so it never travels. **Now compiled and tested on Windows.** |
| D01-b | P1 | **The D01 fix's own heal was broken, and its own test caught it once actually run.** `b98b4f2` stamped the placeholder with `chrono::Utc::now()`. `apply_lww` overwrites only when the local `updated_at` is strictly older than the arriving one, and a real role carries whenever it was genuinely last edited — for roles, usually long past. The placeholder therefore outranked the real role and the row never healed: the user held nothing permanently, recoverable only by editing the database. The previous ledger's claim *"the placeholder self-heals — after the real roles row lands, the user resolves to it with no further action"* was **false**; it came from SQL simulation, and `cargo test` disproved it: `left: "unsynced:R-NOT-PULLED-YET"`, `right: "regional-manager"`. | `cb408ac` — stamp the placeholder at the epoch via a named `PARKED_ROLE_STAMP` carrying the reason. The epoch loses every freshness comparison, which is what a placeholder should do. Security property unchanged; the pre-existing heal test is the regression test (failed before, passes after). |
| E01 | P3 | Three interactive minimum heights were reported below the 56px floor the codebase sets for itself (`App.css:1297`, "T25: WCAG AA touch target for 15-inch touchscreen"). On checking, **only one was real**: `.pos-sidebar-item` at 54px is a genuine `<button>` — Quick Sale, Reports, Admin, each with an `aria-label` (`components/pos/PosSidebar.tsx:44,49,54`). The other two style nothing (see CSS1). | Raised `.pos-sidebar-item` to 56px with a comment pointing at the rule it now honours. |
| OPS2 | P1 | The Rust crate could not be built from a clean clone or a fresh worktree: three of the four resources `tauri.conf.json:35-40` bundles are absent, and each failed the build with a bare `resource path X doesn't exist`, one at a time, after a full dependency compile. CI worked around it and said so (`ci.yml:129`: node.exe "is gitignored and no script in the repo fetched it"); the README told developers to run `npm install`, which is the path that fails. | `scripts/bootstrap.mjs` + `npm run bootstrap`, idempotent, verifying each step produced what it claimed. Invokes `npm-cli.js` under the current Node rather than the `npm.cmd` shim — `execFileSync` on a `.cmd` fails with EINVAL since the CVE-2024-27980 mitigation, and `shell: true` is itself deprecated (DEP0190). Both surfaced only from testing the repair path; the idempotent path passes without touching either. README setup corrected. |
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
| B06 | P1 — `stock_levels` moved inside the sale transaction while `stock_movements` were written after commit, so the shelf count could drop with nothing in the ledger to explain it | **Already fixed on this base, and the ledger was stale in carrying it forward.** `movements::record_sale_movements_tx` now takes `&mut Transaction` and is called at `sale_repo.rs:1051` with `?`, between `pool.begin()` at `:644` and `tx.commit()` at `:1061` — so a movement that cannot be written rolls the sale back. Its doc comment (`movements.rs:258-274`) diagnoses the old post-commit shape in the same terms this finding used. A per-line idempotence guard at `:310-319` additionally protects sales whose movements were written by an older build. |
| B02 | P1 — loyalty accrual ran after commit (originally E05), logged rather than fixed | **Already fixed on this base.** `loyalty_repo::record_tx(&mut tx, …)` is called at `sale_repo.rs:1019`, inside the transaction, with the comment at `:1008` stating it. The void path reverses what a sale earned via `points_awarded_for_sale` / `points_reversed_for_sale` and an `Adjust` event (`:87-105`), and `test_finalize_sale_rolls_back_when_loyalty_update_fails` (`:1779`) covers the rollback. |
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

### Gate run on Windows, 2026-09-11

Run twice. First at `cb408ac` (the D01 work on `1f4738d`), then again after rebasing
onto the six commits that existed only in a local working copy — `cb7191a` through
`c8fa615` — so the published branch is verified as a whole rather than in halves.

| Command | At `cb408ac` (4 commits) | At `3d05658` (11 commits) |
|---|---|---|
| `cargo fmt --all -- --check` | exit 0 | **exit 1 → fixed in `3d05658`, then exit 0** |
| `cargo clippy --all-targets -- -D warnings` | exit 0, zero warnings | exit 0, zero warnings |
| `cargo test --lib` | 964 passed; 0 failed; 1 ignored | **970 passed; 0 failed; 1 ignored** |
| `npm run check` | exit 0 — 70 files, 503 tests | exit 0 — **72 files, 510 tests** |
| `npm run build` | exit 0 | — |

Two things this run established that no previous one could:

1. **The Rust compiles** — the open question the previous ledger flagged. Running it is
   also what exposed **D01-b**, which SQL simulation had reported as proven.
2. **`cargo fmt --check` was red on the unpublished local commits** (`intent_engine.rs`,
   `intent_engine/tests.rs`, `errors/mod.rs`). Because of OPS1 nothing would have caught
   that before it reached `main`. Fixed in `3d05658` as its own commit rather than by
   amending someone else's.

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
