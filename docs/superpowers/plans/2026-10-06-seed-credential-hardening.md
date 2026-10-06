# Seed Credential Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ensure the shipped well-known seed users can never remain usable with PIN `0000` after application startup, without disabling accounts whose PINs were legitimately changed.

**Architecture:** Preserve the existing startup authentication hardening path. Extend `auth_repo::rehash_plain_pins` so it first inspects only the two known seed user IDs, disables any active seed user whose stored credential still verifies as `0000` (PLAIN or Argon2id), then continues the existing legacy-PLAIN rehash. This keeps authority in Rust and avoids a migration that cannot safely recognize already-hashed default PINs.

**Tech Stack:** Rust 1.82+, sqlx 0.8, SQLite, Argon2id, Tauri 2.

**Spec:** Master Autonomous Software Engineering System Prompt supplied in the active engineering session; repository authority `AGENTS.md`.

## Global Constraints

- Repository: `ZanaNowshad/zanpos`.
- Baseline: `main` at `c227cb379490f17b6d0668393cbed900d0a2df3c`.
- Work branch: `agent/harden-seed-cashier`.
- Preserve valid existing setup/auth architecture and database compatibility.
- No plaintext production credentials.
- Server-side authentication remains authoritative.
- Do not add a migration solely to recognize Argon2id-hashed default PINs; SQLite cannot safely infer the original PIN from those hashes.

## Review Focus

- Fresh install: active seeded cashier with `PLAIN:0000` must be disabled before login.
- Existing install: active seeded cashier whose PIN was already Argon2id-rehashed from `0000` must be disabled.
- Legitimate account: same seed user ID with a non-default changed PIN must remain active.
- Owner setup: seeded owner with a changed PIN must not be disabled.
- Legacy non-seed `PLAIN:` users must still be rehashed as before.

---

### Task 1: Pin the vulnerability with regression tests

**Files:**
- Create: `src-tauri/tests/seed_credentials.rs`

**Interfaces:**
- Consumes: `zanpos_lib::db::repositories::auth_repo::{hash_pin, rehash_plain_pins}` and the embedded migration set.
- Produces: behavioral contract for startup credential hardening.

- [ ] **Step 1: Write failing tests**

Add integration tests covering:
1. a migrated fresh database where `cashier1` is active with `PLAIN:0000`; after `rehash_plain_pins`, the user must be inactive;
2. an active seed cashier whose `pin_hash` is `hash_pin("0000")`; after hardening it must be inactive;
3. an active seed cashier whose `pin_hash` is `hash_pin("482619")`; after hardening it must remain active;
4. an arbitrary non-seed `PLAIN:2468` user is still converted to Argon2id and verifies with `2468`.

- [ ] **Step 2: Run the test and verify RED**

Run: `cargo test --test seed_credentials -- --nocapture`
Expected: at least the default-seed deactivation assertion fails against baseline behavior.

### Task 2: Harden the existing startup credential path

**Files:**
- Modify: `src-tauri/src/db/repositories/auth_repo.rs`
- Modify: `src-tauri/src/commands/rbac.rs` only to correct the stale test comment that incorrectly attributes seed-user deactivation to migration `0030`.

**Interfaces:**
- Consumes: existing `verify_pin`, `rehash_plain_pins`, users table.
- Produces: `rehash_plain_pins(pool)` additionally guarantees that known seed identities still using PIN `0000` are inactive before login.

- [ ] **Step 1: Implement the minimal fix**

Inside `rehash_plain_pins`, before legacy rehashing:
- query only `01JUSER000000000000ADMIN1` and `01JUSER000000000000CASH01` when active;
- if the stored hash is `PLAIN:0000`, treat it as the known default directly because runtime `verify_pin` intentionally rejects `PLAIN:` hashes;
- otherwise use `verify_pin(stored_hash, "0000")` for Argon2id rows;
- set `is_active = 0`, clear lockout counters, mark `updated_at`, and set `sync_status='pending'` for matching default-credential users;
- do not modify seed accounts whose PIN no longer verifies as `0000`.

Then run the existing legacy `PLAIN:` rehash exactly as before for all remaining rows.

- [ ] **Step 2: Run targeted tests and verify GREEN**

Run: `cargo test --test seed_credentials -- --nocapture`
Expected: PASS.

- [ ] **Step 3: Run auth/security regression tests**

Run: `cargo test --lib auth_repo auth_session rbac -- --nocapture`
Expected: PASS, or record exact command incompatibility and run the applicable filtered tests separately.

### Task 3: Whole-repository verification through the existing CI gate

**Files:**
- No production changes unless verification identifies a regression.

**Interfaces:**
- Consumes: branch implementation from Tasks 1–2.
- Produces: release evidence for this hardening change.

- [ ] **Step 1: Open a draft PR targeting `main`**

Purpose: trigger the repository's existing PR CI without merging or publishing a release.

- [ ] **Step 2: Verify mandatory checks**

Required relevant gates: Rust checks/tests, frontend, storefront, Semgrep, Gitleaks, Tauri build/release gate dependencies as configured.

- [ ] **Step 3: Diagnose any failure from the first genuine failing step**

Do not weaken tests or security gates. Distinguish repository-wide/pre-existing failures from regressions introduced by this branch.

- [ ] **Step 4: Record final status**

Only classify this hardening as verified when the regression tests and applicable repository gates are green. Hardware UAT is not required for this authentication-only change.
