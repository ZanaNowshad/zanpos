# ZANPOS Production-Readiness Ledger

**Base:** `upgrade/zanpos-open-source` (`1f4738d`). **Verified:** 2026-09-10.
Every row was re-read first-hand at the cited line on *this* base.

## How this ledger was corrected

The audit that produced it originally ran against `main` (`48741d6`). `main` is
**163 commits behind** this branch, and re-checking each finding here dissolved most
of them: they were artefacts of a stale base, not defects in the shipping code.
The record of what was claimed and what survived is kept below, because "we found
seven P0s" and "five of them were already fixed" are different facts about this app.

## Open

| ID | Sev | Area | Evidence | Finding | Status |
|---|---|---|---|---|---|
| D02 | P0 | Hub / transport | `hub/mod.rs:108,128`; `hub/rest.rs:19-22` | The hub binds `0.0.0.0` over **plaintext HTTP** and serves `POST /rest/v1/{table}`, writing sync tables including `users` and `audit_logs`. Authentication is now strong — per-device pairing, constant-time digest comparison, `hub_unauthorized` audit events, legacy shared token deprecated — but the bearer token and every synced row still cross the LAN in cleartext, so anyone on the shop wifi can read them and replay. `discovery.rs:24,60,68` already advertises a `tls_fingerprint` field that nothing populates, so the intended shape is on record. | **OPEN** — needs its own design (rustls + self-signed cert minted at hub start + fingerprint pinned during pairing). Not a minimal fix. |
| B06 | P1 | Stock ledger | `sale_repo.rs` (`deduct_sale`, post-commit) | `stock_levels` is updated inside the sale transaction; `stock_movements` rows are written after commit, explicitly non-rolling-back. Levels and ledger can diverge, so stock is not reconstructible from history. Note `0056_loyalty_ledger.sql` applies exactly the convergent fix to loyalty and names `stock_movements` as the shape it copies — the same reasoning has not been applied back to the stock path itself. | **OPEN**, unchanged from the original audit |
| B02 | P1 | Loyalty write | `sale_repo.rs` (post-commit accrual) | Loyalty accrual still runs after commit (originally E05). Now logged rather than fixed. Largely superseded by the `loyalty_events` ledger — needs re-reading against it before it is worth acting on. | **NEEDS RE-READ** |

## Fixed in this pass

| ID | Sev | Finding | Fix |
|---|---|---|---|
| D01 | P0 | `sync_v2/apply.rs` remapped an inbound user whose `role_id` was unknown locally to **the owner role**, to satisfy the FK. Unknown input widened privilege to maximum. | `b98b4f2` — parks an unprivileged placeholder under the same role_id (`rbac::UNSYNCED_ROLE_PREFIX`, matched by no allowlist), kept local so it never travels, healed by LWW when the real role syncs. Five assertions cover it. |
| D06 | P1 | `cashier_cannot_perform_manager_only_operations` called `manager_or_owner` identically three times with the command name only in the failure message — and could not distinguish a denied cashier from an absent one. | `b98b4f2` — replaced with a test that walks the allowlists themselves, asserting both the admits and the refusals. |

## Dissolved on re-check — fixed before this work began

These were real on `main` and are **already fixed on this branch**. Recorded so they
are not re-reported by the next audit that starts from the wrong branch.

| ID | Was | Resolution on this base |
|---|---|---|
| C01 | P0 — `init_db` failure deleted `zanpos.db`, `-wal`, `-shm`, `-journal` and retried, so a migration checksum change destroyed the shop's data | Nuke-and-retry path is gone |
| C02 | P0 — any failed read-only probe (a transient AV or backup lock) was read as "not ours" and deleted the database | Probe-and-delete path is gone |
| D03 | P0 — applying a synced `users` row deactivated every local user sharing the username, unlogged | Now LWW-guarded; a stale row cannot deactivate a live account, and the loser yields its unique name. Regression tests present (`U-LIVE`/`U-STALE`) |
| D04 | P0 — no session: `require_role` asked "does user X hold role R?", never "is the caller X?", and `actor_user_id` was an ordinary IPC parameter | `auth_session::SessionStore` + `session_actor` resolve the caller from a session token this process issued at login; the old signature is retained only for authorization |
| B01 | P0 — retention pruned sales at 90 days and `audit_logs` at 30, destroying tax records and breaking the hash chain | Reworked behind a `PRUNABLE_SALES` guard; the code comments record the same defect |
| A05 | P1 — loyalty points accrued with nothing to spend them | `0056_loyalty_ledger.sql` adds `loyalty_events` (append-only, signed, `earn`/`redeem`/`adjust`/`opening`) with `loyalty_repo.rs` and `db/invariants/loyalty.rs` |
| A06 | P1 — loyalty synced as `MAX(local, incoming)`, so a redemption would be reverted | Same migration. Its comment diagnoses the `MAX` merge precisely and moves the total to a derived sum; `customers.loyalty_points` stays a local cache and stops travelling |
| A03 | P2 — `hub_join` had no RBAC gate | Overstated when raised: `hub_commands.rs:236-247` rejects the call once `setup_complete = 1`, so it is reachable only on an unprovisioned terminal |

## Corrections to the original brief

| Claim | Truth |
|---|---|
| "163 `.unwrap()`/`.expect()` outside tests" | **11.** A `#[cfg(test)]`-aware scan finds 7 `unwrap` + 4 `expect`. The 88/75 figures came from `grep -v test`, which only drops lines containing the word. |
| "Frontend tests are utilities only — 24 tests in 3 files, no component tests, no jsdom" | **70 files, 503 tests, all passing.** 20 are `.tsx` component tests. |
| "No i18n, no RTL, no Arabic" | `src/i18n/` exists with `locales/`, `modalStrings.ar.ts`, and a test asserting *every* key has a real Arabic translation. |
| "Baseline is red — 2 eslint warnings" | **Green on this base.** `npm run type-check`, `npm run lint --max-warnings 0` and `npm test` all exit 0. |
| "7 migrations" | **65.** |

## Verification constraint — read before trusting any Rust result

**The Rust crate cannot be built or tested on Linux.** `sha2`, `hmac`, `hex`, `base64`,
`csv`, `mysql`, `tiberius`, `calamine`, `zip` and `keyring` all sit under
`[target.'cfg(windows)'.dependencies]` (`Cargo.toml:92-123`), so a Linux `cargo test`
fails with ~155 unresolved-crate errors before running a single test. This is why CI
runs the Rust job on `windows-latest`.

Consequences: `cargo fmt --all -- --check` passes locally and proves the files parse,
but **no Rust change made in a Linux session can be locally verified** — the Windows CI
job is the only gate. Do not report a Rust change as tested without a CI run behind it.

Frontend gates do run locally and were green on this base before and after the change.

## Coverage

Complete on this base: Lens D (security), Lens A (IPC/CRUD, partial).
Partial: Lens B, Lens C.
**Not started: Lens E (UX), Lens F (visual), Lens G (performance).** Lens H is largely
answered by `docs/audit/pos-ai-deepscan-gap-report.md` and `docs/proposals/01-05`
already on this branch — NBR e-invoicing QR, Arabic/RTL bilingual receipts, the
purchasing loop with cost & margin, Benefit ECR card terminal, and a promotions and
loyalty rules engine.
