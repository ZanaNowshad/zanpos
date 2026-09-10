# ZANPOS Production-Readiness Ledger

**Base audited:** `claude/zanpos-discussion-rpnb0k` @ `31fd2e1` (= `main` @ `48741d6` + docs).
**Verified:** 2026-09-10. Every row below was re-read first-hand at the cited line.

> **Read this first.** This audit was run against `main`. `origin/upgrade/zanpos-open-source`
> is **163 commits ahead of `main`, 0 behind** (clean fast-forward), and already fixes several
> P0s below. The `Upgrade br.` column records each finding's status *there*. Do not implement
> against `main` without deciding the branch question — see `## Base-branch problem`.

## Status legend

`CONFIRMED` re-read and reproduced · `CORRECTED` claim was wrong or overstated ·
`UNVERIFIED` could not be checked in this environment · `FIXED-ON-UPGRADE` already solved
on `upgrade/zanpos-open-source`.

## Findings

| ID | Sev | Area | Evidence (on `main`) | Finding | Upgrade br. |
|---|---|---|---|---|---|
| C01 | P0 | Startup / data loss | `lib.rs:109-144` | `init_db` failure is caught by **deleting `zanpos.db`, `-wal`, `-shm`, `-journal` and retrying**. A migration checksum change — which `app_stderr.txt` records as `Migration(VersionMismatch(23))` — therefore destroys the shop's sales, tax records and audit chain on update. `0001_initial.sql` has been edited after shipping (`2cbdae5`, `33115ff`), so the trigger is real, not hypothetical. | **FIXED** — nuke-and-retry path is gone |
| C02 | P0 | Startup / data loss | `lib.rs:87-106` | The "is this DB ours?" probe treats **any** failed read-only connect as `false` (`else { false // can't even open — assume corrupt }`) and deletes the database. An antivirus or backup process holding a transient lock is sufficient. | **FIXED** — probe/delete path is gone |
| D01 | P0 | Sync / privilege escalation | `sync_v2/apply.rs:160-181` | An inbound synced `users` row whose `role_id` is not in the local `roles` table is **remapped to the owner role** (`01JROLES000000000000000001`). Unknown input widens privilege to maximum; the `tracing::warn!` states it explicitly. Fail-open. | **STILL OPEN** (`:357-370`) |
| D02 | P0 | Hub / network | `hub/mod.rs:53,73`, `hub/rest.rs:18` | Hub binds **`0.0.0.0`** over **plaintext HTTP** and exposes `POST /rest/v1/{table}` writing any of the sync tables — including `users` and `audit_logs` — authorised by a bearer token crossing the same cleartext LAN. Table name is allowlisted (`rest.rs:92`), so no SQL injection. | **PARTIAL** — auth hardened (per-device pairing, constant-time digest compare, `hub_unauthorized` audit events, legacy shared token deprecated) but **still plaintext on `0.0.0.0`** |
| D03 | P0 | Sync / lockout | `sync_v2/apply.rs:122-136` | Applying a synced `users` row runs `UPDATE users SET is_active = 0, deleted_at = ? WHERE username = ? AND user_id <> ?` — deactivating every local user sharing that username, as an unlogged side effect (`let _ =`). Remote owner lockout. | **FIXED** — statement removed, regression tests added (`U-LIVE`/`U-STALE`/`shared-name`) |
| D04 | P0 | AuthN / session | `commands/rbac.rs:11-39` | There is no session. `require_role` answers *"does user X hold role R?"*, never *"is the caller X?"* — `actor_user_id` is an ordinary IPC parameter. The doc comment ("the frontend cannot forge a role") is true of the role and silent about the identity. `auth_list_users` hands out user IDs pre-login. | **FIXED** — `session_actor` + session-token functions added; old fn retained for compatibility |
| B01 | P0 | Retention / compliance | `sync_v2/worker.rs:800-856` | Defaults prune **sales at 90 days** and **`audit_logs` at 30 days**. Refunds and reprints become impossible past the window, and a tamper-evident hash chain that self-destructs monthly is not tamper-evident. Both deletes *are* gated on `sync_status = 'synced'` (old E12 fix is in place). Bahrain NBR retention is materially longer. | **REWORKED** — `PRUNABLE_SALES` guard; comment records the same defect |
| B06 | P1 | Stock ledger | `sale_repo.rs:576-594` | `stock_levels` is updated **inside** the sale transaction; `stock_movements` rows are written **after commit** ("failures don't roll back sale"). Levels and ledger can diverge, so current stock is not reconstructible from history. The `tracing::error!` says so. | not re-checked |
| B02 | P1 | Loyalty | `sale_repo.rs:560-575` | Loyalty accrual still runs post-commit (old **E05**). Now logged (`T06`) rather than fixed — a crash between commit and update silently loses the customer's points. | not re-checked |
| A05 | P1 | Loyalty / half-feature | `ai/tools.rs:852,4341,6015`; no redemption path | Points accrue (`add_loyalty_points`, sale accrual) and are displayed, but **nothing spends them**. There is no redemption command, binding, or UI. | **STILL OPEN** |
| A06 | P1 | Loyalty / sync | `sync_v2/apply.rs:338-340` | **New finding.** Loyalty merges as `MAX(customers.loyalty_points, excluded.loyalty_points)`. Any redemption — a *decrease* — would be silently reverted by the next sync. The missing half of A05 cannot be built until this changes. | **STILL OPEN** (`:844`) |
| D06 | P1 | Test integrity | `commands/rbac.rs:233-246` | `cashier_cannot_perform_manager_only_operations` loops over `["shift_close","cash_event_create","pos_void_sale"]` but calls `manager_or_owner(&pool, cashier_id)` identically each pass — the label appears only in the failure message. It asserts one thing three times and exercises none of the three commands. Devalues the 104-test headline. | **STILL OPEN** (`:294-299`) |

## Corrections to the original brief

| Claim | Verdict |
|---|---|
| "163 `.unwrap()`/`.expect()` outside tests" | **CORRECTED → 11.** A `#[cfg(test)]`-aware scan finds **7 `unwrap` + 4 `expect`**: `lib.rs:45,142,594`, `ai/tools.rs:1331`, `ai/engine/batch.rs:98`, `sync_v2/client.rs:31`, `sync_v2/apply.rs:227`, `migration_commands.rs:3473`, `setup_commands.rs:228,284,285`. The 88/75 figures came from `grep -v test`, which only drops lines containing the word. Triage is an S, not an L. |
| "A03 — `hub_join` has no RBAC gate, one IPC call from compromise" | **CORRECTED / overstated.** True that no `rbac::` call guards it, but `hub_commands.rs:236-247` rejects the call outright once `setup_complete = 1`. Reachable only on an unprovisioned terminal. Downgrade to P2. |
| "Baseline is red — 2 eslint warnings" | **UNVERIFIED here.** This container has no usable `node_modules`; eslint cannot load `@eslint/js` and `tsc` reports missing `@tauri-apps/api` types. Neither confirms nor refutes the claim — must be re-run where deps install. |
| "172 commands / 172 registered / 170 bound, zero phantom bindings" | Not re-derived. Plausible and cheap to re-check on the chosen base. |
| "62 components, not 52" | Not re-derived. |

## Base-branch problem

`origin/upgrade/zanpos-open-source` — **163 commits ahead of `main`, 0 behind, `main` is a clean
ancestor**. Beyond the fixes tabulated above it carries: a VAT defect fix (`e148185` — VAT was
charged on the pre-discount total), release-gate CI, Playwright storefront E2E with Linux
baselines, nextest, a backup runbook correction, a corrected compliance sheet, clippy/fmt/MSRV
fixes — plus `docs/audit/pos-ai-deepscan-gap-report.md` and five proposals that already cover
much of Lens H: NBR e-invoicing QR, Arabic/RTL bilingual receipts, the purchasing loop with
cost & margin, Benefit ECR card-terminal integration, and a promotions/loyalty rules engine.

Implementing this ledger against `main` would re-solve C01, C02, D03, D04 and B01, and land
those fixes on the losing side of a 163-commit merge.

## Coverage

Complete: Lens D (security), Lens A (IPC/CRUD, partial). Partial: Lens B (money/integrity),
Lens C (resilience). **Not started: Lens E (UX), Lens F (visual), Lens G (performance),
Lens H (missing practice — though the upgrade branch's proposals cover much of it).**

An earlier attempt to fan out nine parallel lens agents died on the usage limit; the lenses
above were then run inline. Everything recorded here has a line number that was actually read.
