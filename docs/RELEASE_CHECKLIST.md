# ZANPOS Release Checklist — 2.0.1

Commit `e148185` · built 2026-09-08

Artifact: `ZANPOS_2.0.1_x64-setup.exe` · 219,723,242 bytes
SHA-256 `6f107a19e035bd8bfeaefdc7599f821f1d4b3e594fa26e703d66710de3351e3e`
(computed twice, identical)

Produced by `npm run ship` — **SHIP GATE PASSED, 0 failures, 0 warnings.**

> The previous revision of this file described 2.0.0 and is superseded. It
> recorded gates 6 and 7 as BLOCKED / NOT RUN behind a sidecar file lock, 44
> migrations, and installs as untested. All of those numbers had moved.

---

## Automated gates

| # | Check | Result | Evidence |
|---|---|---|---|
| 1 | Frontend types, lint, tests | PASS | ship gate 1 — `tsc` + `eslint --max-warnings 0` + vitest, at this commit |
| 2 | Rust `cargo check` | PASS | ship gate 2, 1m01s |
| 3 | Storefront types | PASS | ship gate 3 |
| 4 | File-size rule (500 lines) | PASS | ship gate 4 — no un-allowlisted file; 47 grandfathered entries unchanged |
| 5 | One version everywhere | PASS | ship gate 5 — 2.0.1 in all three manifests |
| 6 | Test modules `cfg`-gated | PASS | ship gate 6 — nothing test-only compiles into the release binary |
| 7 | Worker copies in sync | PASS | ship gate 7 — 7/7 routes in both `worker/index.ts` and `worker_embedded.js` |
| 8 | Updater signing key works | PASS | ship gate 8 — signs a throwaway file before the hour-long build, not after |
| 9 | Release build (fat LTO) | PASS | ship gate 9 — `release` profile, 17m38s, no release-only crash |
| 10 | NSIS bundle produced | PASS | 219,723,242 bytes |
| 11 | Updater signature emitted | PASS | `.sig` beside the installer, 416 bytes |
| 12 | **Updater signature verifies** | PASS | Ed25519 over BLAKE2b-512 against the `pubkey` in `tauri.conf.json`; key id `a0c490b084c5d428` matches on both sides |
| 13 | Rust test suite | PASS | `cargo nextest run --all-targets` at this commit — **1009 run, 1009 passed, 1 skipped**, 209.5s |
| 14 | Migration chain | PASS | 64 files, highest `0065_current_selling_price_view.sql`, embedded via `sqlx::migrate!` |
| 15 | Semgrep policy + contract validator | PASS | at this commit — self-test (production policy, 91 security mutations, 9 lexer edges), exception/authorization validator, 15/15 rule unit tests, and `semgrep scan --error`: **0 findings**, 15 rules, 265 files |
| 16 | Gitleaks | PASS | at this commit — 7 directories over 1041 tracked files (exported from `git archive HEAD`, matching CI's clean checkout): clean. The inverse assertion also holds: the planted fixtures still trigger detection, so "clean" is not a dead scanner. |
| 17 | Code signing (Authenticode) | **NOT SIGNED** | no `certificateThumbprint` and no `signCommand` in `tauri.conf.json`. Windows will show a SmartScreen warning on first install. |
| 18 | Install on a clean machine | NOT TESTED | no isolated environment available here |
| 19 | POS hardware smoke | NOT TESTED | needs the shop — see below |

Semgrep and Gitleaks were re-run locally at this commit (gates 15-16) rather than
cited, because CI has not run since `2117523`. Still attributed to that last
fully-green run, covering work this commit does not touch: frontend **503 tests /
70 files**, storefront unit **11 tests**, and storefront Playwright **102 tests**
against the *built* bundle at desktop, Pixel 5 and Arabic.

## CI is currently blocked — read before trusting a red run

GitHub Actions is refusing to start jobs on this account:

> The job was not started because recent account payments have failed or your
> spending limit needs to be increased.

It began as Windows-only (the `build` job) and is now total — every job,
including the cheap Ubuntu ones, fails in about a second with **zero steps** and
no runner assigned. That shape reads like a runner flake and it is not; the
reason appears only via
`gh api repos/<owner>/<repo>/check-runs/<job_id>/annotations`. Re-running just
reproduces it. Only the account owner can clear it.

Until then `npm run ship` is the verification of record, and for the Tauri build
it is the stronger one anyway: CI's "Tauri build check" is a **debug** build,
while ship does the real fat-LTO **release** build — the only one that can
surface the `0xc00000fd` class of release-only stack overflow.

## Runtime verification on the release binary

Performed against the built binary, not the dev server: launches without
`0xc00000fd`; database reports 80 tables, 64 migrations, `integrity_check: ok`;
WhatsApp sidecar healthy on `127.0.0.1:3131`, returning 401 with no token and
with a wrong one; LAN hub on `0.0.0.0:8923` with all 7 routes refusing
unauthenticated callers; kill and relaunch leaves data intact.

## Manual gates — none of these can be closed by code

| Gate | Owner | State |
|---|---|---|
| Restore drill on shop hardware | Operations | Bench drill **passed** — `backup-restore-ops.md` §8. On-hardware run and signed template still required. |
| Finance/tax adviser review | Adviser | Pack ready — `vat-receipt-review.md`. The VAT defect it found (§3) is **fixed**: a whole-bill discount was applied after line tax, overstating output VAT and over-refunding discounted lines. Still to ratify: the apportionment basis, and whether sales taken before the fix need correcting. |
| Card terminal PCI P2PE certificate | Operations | Pending |
| BitLocker on all POS hardware | IT | Pending |
| Authenticode code-signing certificate | Owner | Not configured (gate 17) |

### Shop smoke test

Printed by `npm run ship` on success. Step 3's second half — unplug the printer
mid-sale — is verified in code and needs only physical confirmation: the sale is
committed before printing is attempted, printing is dispatched fire-and-forget
so it cannot delay or fail the sale, the serial path times out after 15s, a
failure is recorded as `print_fail`, the rendered lines are queued to
`reprint_queue` so the receipt can be reproduced exactly, and the cashier is
told "Sale saved, but printing failed".

```
[ ] 1. Launch the release build (not dev) and log in with a PIN
[ ] 2. Complete one cash sale, scanner included
[ ] 3. Receipt prints on the store's own paper — then unplug the printer,
       sell again, confirm the sale still completes and the failure is visible
[ ] 4. Send one WhatsApp message from the app
[ ] 5. Ask ZanAI one question and get an answer
[ ] 6. Publish the storefront and load the public URL on a phone
[ ] 7. Settings → Send diagnostics returns a result
[ ] 8. Close and relaunch: no crash banner, no lost data
```

## Reproduce

```bash
npm run ship
```
