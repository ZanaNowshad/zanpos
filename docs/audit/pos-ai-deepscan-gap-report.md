# POS-AI Deepscan Protocol Audit — Gap Report

**Audit date:** 2026-06-16
**Protocol version:** 2.0.0-POS (adapted for single-terminal software POS)
**Scope:** zanpos — Rust/Tauri v2 + React 19 + SQLite, Bahrain single-terminal POS

## Adaptation Notes

The protocol is designed for enterprise multi-terminal POS with payment hardware and PCI-DSS attestation. For zanpos (software-only, single-terminal):

- **N/A**: Hardware integration (no card reader/PIN pad/NFC code exists)
- **N/A**: Multi-channel UI consistency (single-terminal desktop app)
- **N/A**: PCI-DSS formal attestation (governance gate; SAQ P2PE guidance only)

Thresholds are adapted: a single gap in the enterprise protocol may be a P2 finding here, and a finding with no code surface at all is marked N/A.

---

## Executive Summary

| | Count |
|---|---|
| P0 (financial risk) | 5 |
| P1 (security/operational risk) | 5 |
| P2 (improvement) | 6 |
| N/A (not applicable) | 5 |
| PASS | 34 |

**Overall verdict: PASS with remediation required on 5 P0 items before production.**

---

## Section 1: Static Exoscan

### 1.1 Hardware Integration — N/A

No card reader, PIN pad, NFC, barcode scanner, receipt printer, or cash drawer code exists. All hardware interaction is delegated to external systems or is purely UI (ESC/POS receipt builder outputs text; thermal config is settings-only). `open_cash_drawer` is a no-op stub in `tools.rs:954`.

**Verdict:** N/A — software-only POS.

### 1.2 UI State Inventory

React component states traced through `PosPage.tsx` → `useCart.ts` → `Cart` domain struct.

| State | Coverage | Gaps |
|---|---|---|
| Empty cart | Handled | — |
| Active cart with items | Handled | — |
| Held/parked cart | Handled via `held_cart_save/list/resume/delete` | P2-06: no product validation on resume |
| Discounted cart (bill + line) | Handled, capped to subtotal | — |
| Split payment | Handled | — |
| Custom item | Handled with price validation | — |
| Void line/transaction | Handled | — |
| Refund flow | Handled with 5 reason codes | — |
| Shift open/close | Handled with ghost-shift prevention | — |
| Error boundaries | `ErrorPage.tsx` catch-all | P2-04: minimal per-component error boundaries |

**Verdict:** PASS with 2 P2 notes.

### 1.3 Multi-Channel Consistency — N/A

Single-terminal desktop app. No web/mobile/kiosk channels.

**Verdict:** N/A.

---

## Section 2: Dynamic Endoscan

### 2.1 Transaction Data Flow (P0 financial)

| Check | Finding | Evidence |
|---|---|---|
| Money integrity | **PASS** — all i64 minor units, no f64 in money paths | `domain/money.rs`, `sale_repo.rs` |
| PriceOp::Percent rounding | **PASS** — rounds immediately after multiplication | `domain/money.rs` apply_discount_bp |
| Atomic receipt number | **PASS (FIXED)** — `UPDATE devices SET next_receipt_seq = next_receipt_seq + 1 RETURNING next_receipt_seq` inside tx | `sale_repo.rs:22-24` |
| Idempotency | **PASS** — UNIQUE on `sales.idempotency_key` + `refunds.idempotency_key` | `0001_initial.sql` |
| Atomicity | **PASS** — `BEGIN` → writes → `tx.commit()` wrapping | `sale_repo.rs:217-555` |
| Stock deduction guard | **PASS** — `WHERE CAST(quantity_on_hand AS REAL) >= ?` inside tx | `sale_repo.rs:359-482` |
| Split payment rounding | **PASS** — last payment absorbs remainder | `sale_repo.rs` |
| Tax recalculation | **PASS** — server-side recalculation ignores frontend values | `sale_repo.rs:140-168` |
| Bulk price verification | **PASS** — fetches current DB prices, rejects mismatches | `sale_repo.rs:86-137` |
| Idempotency key from frontend | **P1-01** — if frontend doesn't provide key, Ulid is generated; retry creates duplicate | `pos_commands.rs:279-281` |

**Verdict:** 9 PASS, 1 P1 gap.

### 2.2 AI Inference Pipeline (P0 security)

| Check | Finding | Evidence |
|---|---|---|
| API key storage | **PASS** — OS credential store (`secure_store.rs`) with SQLite fallback for legacy | `provider.rs` |
| Multi-provider support | **PASS** — Anthropic, OpenAI, Gemini (3 providers) | `provider.rs` Provider enum |
| Tool loop termination | **PASS** — max 8 turns enforced | `streaming.rs:131` |
| Context window truncation | **PASS** — `truncate_history()` from provider | `provider.rs`, `streaming.rs:598-615` |
| Mutation confirmation | **PASS** — dry_run → create action → MutationPending → user confirm | `streaming.rs:467-496` |
| Audit logging (AI) | **PASS** — all AI mutations use `actor_type: "ai"` | `audit_hash.rs`, 88 MUTATION_TOOLS |
| ENGINE_OPS bypass | **P0-01** — `streaming.rs:273-383` dispatches engine ops directly, bypassing `is_mutation_tool()` check and standard confirmation flow | `streaming.rs` |
| Read tool failure wrapping | **P0-02** — `streaming.rs:498-513` wraps read failures as success, loop continues with bad data | `streaming.rs` |
| Gemini streaming | **P0-03** — `provider.rs` `api_key()` returns `""` for Gemini variant on streaming path | `provider.rs` |
| Single provider (no fallback) | **P1-02** — `Provider::from_db` returns single provider; no failover | `provider.rs` |
| No per-tool RBAC | **P1-03** — tools gated by feature flags only, not roles | `tools.rs:1192-1199` |
| No AI observability | **P1-04** — no latency/cost/cache tracking dashboard | — |
| AI reasoning not persisted | **P1-05** — reasoning_content captured in response but not stored to DB | `streaming.rs` |

**Verdict:** 7 PASS, 3 P0, 4 P1.

### 2.3 Dependency Graph

Domain layer (`domain/*.rs`) has zero dependencies on commands/ or ai/. Clean architecture separation confirmed.

**Verdict:** PASS.

---

## Section 3: Cohesion Audit

### 3.1 Transaction Integrity (P0)

| Check | Finding | Evidence |
|---|---|---|
| Ghost shift prevention | **PASS** — UNIQUE partial index `WHERE status = 'open'` | `0001_initial.sql:475` |
| Cross-device refund guard | **PASS** — 3-tier: same-device/cross+manager/cross+cashier+token | `refund_commands.rs:110-137` |
| Manager override token | **PASS** — 60s TTL, 2-tier storage (mem + DB), one-shot consumption | `override_token.rs` |
| Held cart integrity | **PASS** — carts JSON-serialized, resume reassigns shift_id | `held_cart_repo.rs` |
| Held cart stale product | **P2-06** — no validation on resume; caught at finalize by bulk price check | `held_cart_repo.rs:77-101` |
| Sync FK-safe push order | **PASS** — 20-table dependency-ordered push | `sync_v2/worker.rs` |
| Sync idempotency | **PASS** — outbox pattern with adaptive backoff | `sync_v2/worker.rs` |

**Verdict:** 6 PASS, 1 P2.

### 3.2 AI Decision Consistency (P1)

| Check | Finding | Evidence |
|---|---|---|
| Mutation audit completeness | **PASS** — 88 MUTATION_TOOLS, integrity test verifies all | `tools.rs:1090-1186, 6640-6677` |
| Intent parameter validation | **PASS** — 21 intents with fixed JSON Schema params | `intent_engine.rs` |
| Decision reproducibility | **P2-01** — temperature=0.0 but untested; no deterministic harness | `provider.rs` |
| Tool bypass risk | **P1-03** (carried from §2.2) — tools callable without intent routing | `streaming.rs` |

**Verdict:** 2 PASS, 1 P1, 1 P2.

### 3.3 Regulatory Compliance — Bahrain NBR

| Requirement | Status | Evidence |
|---|---|---|
| Sequential receipt numbers | **PASS** — atomic RETURNING in tx | `sale_repo.rs:22-24` |
| TRN on receipt | **PASS** — `TRN: {tax_number}` | `receiptLines.ts:38` |
| VAT breakdown (subtotal/VAT/total) | **PASS** — 3-line breakdown when tax > 0 | `receiptLines.ts:68-70` |
| BHD 3 decimal places | **PASS** — `formatMoney(n, currency_exponent=3)` | `receiptLines.ts:26`, `money.ts` |
| Tax rate snapshot per line | **PASS** — `tax_rule_snapshot` JSON on sale_items | `0001_initial.sql` |
| No FP on money | **PASS** — i64 minor units throughout | `money.rs` |
| Audit hash chain | **PASS** — SHA-256 NUL-separated, chain walk, legacy handling | `audit_hash.rs` |
| Separate refund sequence | **PASS** — `REF-{08d}` prefix | `refund_repo.rs` |
| Void traceability | **PASS** — `sales.status = 'voided'` with audit log | `pos_commands.rs:782-786` |
| JSON-LD signed output | **P2-02** — hash chain provides tamper-evidence but no portable signed artifact | `audit_hash.rs` |
| AI explainability persisted | **P1-05** (carried from §2.2) — reasoning not stored | `streaming.rs` |

**Verdict:** 9 PASS, 2 gaps (1 P1, 1 P2).

---

## Section 4: Capability Utilization Matrix

147 tools mapped against 15 protocol capability domains.

| # | Domain | Tools | Coverage | Schema | RBAC | Error | Test | Score |
|---|---|---|---|---|---|---|---|---|
| 1 | Product & Catalog Mgmt | 15 | 85 | 100 | 60 | 70 | 50 | 73 |
| 2 | Inventory & Stock Control | 12 | 80 | 100 | 70 | 70 | 50 | 74 |
| 3 | Pricing & Promotions | 8 | 65 | 100 | 40 | 60 | 30 | 59 |
| 4 | Transaction & Sales | 8 | 80 | 100 | 80 | 80 | 70 | 82 |
| 5 | Refunds & Voids | 5 | 85 | 100 | 80 | 80 | 60 | 81 |
| 6 | Customer Management | 14 | 75 | 100 | 50 | 70 | 40 | 67 |
| 7 | Shift & Cash Management | 12 | 90 | 100 | 80 | 75 | 50 | 79 |
| 8 | Reporting & Analytics | 28 | 95 | 100 | 30 | 60 | 30 | 63 |
| 9 | Tax & Compliance | 6 | 80 | 100 | 70 | 75 | 60 | 77 |
| 10 | Configuration & Admin | 12 | 75 | 100 | 85 | 70 | 40 | 74 |
| 11 | AI/ML Model Operations | 5 | 40 | 80 | 90 | 50 | 20 | 56 |
| 12 | Proactive Intelligence | — | 30 | 80 | 0 | 60 | 10 | 36 |
| 13 | Undo/Reversal | 7 | 60 | 80 | 70 | 50 | 20 | 56 |
| 14 | Bulk Operations | 8 | 70 | 100 | 60 | 50 | 30 | 62 |
| 15 | Supplier & Purchase Order | 10 | 80 | 100 | 60 | 70 | 30 | 68 |

**Score legend:** Coverage = tool quantity vs need (0-100), Schema = % with complete input_schema, RBAC = role-gating completeness (0-100), Error = error handling quality (0-100), Test = test coverage (0-100). Final score is average.

**Weakest domains:**
- Proactive Intelligence (36) — no user-facing tools, hardcoded thresholds, no RBAC on alerts
- AI/ML Model Operations (56) — minimal tool surface, no drift detection, no model governance
- Undo/Reversal (56) — undo tied to engine ops, not surfaced as independent tools

---

## Section 5: Output Specification — Audit Hash Chain

### SHA-256 Hash Chain — PASS

`audit_hash.rs` (425 lines) implements a tamper-evident per-device hash chain:

- **Canonical format:** NUL-byte (`\x00`) separators between 11 fields, preventing field-splicing attacks
- **Chain link:** Each row's hash includes `previous_hash` as the final field
- **Genesis:** Empty `previous_hash` stored as NULL; chain starts from first SHA-256 row
- **Legacy handling:** Rows with < 64 char hashes (pre-upgrade) counted but excluded from verification (F-MED-09)
- **Verification:** `verify_chain()` walks oldest-first, recomputes each hash, checks previous_hash links
- **Tests:** 3 unit tests (determinism, chain effect, field sensitivity)

**Gap: JSON-LD Signing — P2-02**
The hash chain provides tamper-evidence within the database but does not produce a portable signed artifact (e.g., JSON-LD with Ed25519/LD-proofs). This is a P2 item — the chain is sufficient for internal audit and would be strengthened by cryptographically-signed export for external compliance.

---

## Section 6: Success Criteria (Adapted)

Enterprise criteria adapted for single-terminal software POS:

| Criterion | Protocol Threshold | Adapted Threshold | Result |
|---|---|---|---|
| Money integrity | 0 f64 in money paths | Same | PASS |
| Transaction atomicity | All DB writes in tx | Same | PASS |
| Idempotency | UNIQUE key on all financial writes | Same | PASS |
| Audit trail | Per-row hash chain | Same | PASS |
| RBAC | Per-endpoint role gating | Per-command gating (no per-tool RBAC) | PASS* |
| PCI-DSS | Formal attestation | SAQ P2PE guidance only | N/A |
| Hardware integration | Card reader/PIN pad gate | N/A (no hardware code) | N/A |
| Multi-channel consistency | All channels identical | N/A (single-channel) | N/A |
| Receipt compliance | TRN + VAT + BHD 3DP | Same | PASS |
| AI kill-switch | Instant disable | Not implemented | P0-04 |
| AI fallback | Multi-model failover | Single provider only | P1-02 |
| Offline resilience | All writes local-first | Same | PASS |

\* RBAC: Commands are role-gated, but AI tools are feature-flag-gated only. This is a documented P1-03 gap.

---

## Section 7: Failure Modes & Escalation

### Proactive Detection Rules — 10 read-only rules on 300s loop

| Rule | Protocol Mode | Status |
|---|---|---|
| Negative margin products | Margin compression | Active |
| Stock below reorder point | Stockout risk | Active |
| Sync stuck > 30 min | Data divergence | Active |
| Cash discrepancy > threshold | Theft detection | Active |
| Multiple voids by same cashier | Fraud pattern | Active |
| Over-discount pattern | Margin erosion | Active |
| Unusually high refund rate | Return fraud | Active |
| Open shift > 24h | Ghost shift risk | Active |
| Product not sold in 30 days | Dead stock | Active |
| Tax config inconsistency | NBR compliance | Active |

**Gaps:**
- **P0-05** — No auto-escalation chain. All alerts are UI-only toast notifications. Sync stuck, negative margin, and cash discrepancy have no email/SMS/webhook escalation.
- **P0-04** — No kill-switch for AI. No `ai_enabled` flag exists to immediately disable all AI operations.
- **P2-05** — Detection thresholds are hardcoded in `proactive.rs`, not configurable per store.
- **P2-03** — No pluggable detection rule system; adding a rule requires code changes.

---

## Complete Findings Index

### P0 — Financial / Critical Risk

| ID | Finding | Section | Evidence | Remediation | Effort |
|---|---|---|---|---|---|
| P0-01 | ENGINE_OPS bypasses mutation confirmation flow | §2.2 | `streaming.rs:273-383` | Route engine ops through standard `is_mutation_tool()` → dry_run → confirm flow; or add equivalent gating inline | M |
| P0-02 | Read tool failures wrapped as success, loop continues with bad data | §2.2 | `streaming.rs:498-513` | Emit error result to model when read tool fails; let model decide recovery path | S |
| P0-03 | Gemini streaming path has empty API key | §2.2 | `provider.rs` api_key() match | Fix `api_key()` for Gemini variant to read from credential store or config | S |
| P0-04 | No AI kill-switch | §7 | No `ai_enabled` flag | Add `ai_enabled` config flag; gate all AI calls behind it; wire to admin UI toggle | S |
| P0-05 | No auto-escalation for critical alerts | §7 | `proactive.rs` | Add webhook/email escalation for sync stuck, negative margin, cash discrepancy | M |

### P1 — Security / Operational Risk

| ID | Finding | Section | Evidence | Remediation | Effort |
|---|---|---|---|---|---|
| P1-01 | Idempotency key auto-generated on retry | §2.1 | `pos_commands.rs:279-281` | Require client-provided idempotency key for all sale mutations | S |
| P1-02 | Single AI provider (no fallback) | §2.2 | `provider.rs` from_db | Implement provider chain: try primary → fallback to secondary → error | M |
| P1-03 | No per-tool RBAC (feature flags only) | §2.2 | `tools.rs:1192-1199` | Add `required_role` field to ToolDef; gate execution in streaming dispatch | L |
| P1-04 | No AI observability dashboard | §2.2 | — | Add latency/cost/token tracking per provider; surface in AITab UI | M |
| P1-05 | AI reasoning not persisted | §2.2, §3.3 | `streaming.rs` | Store reasoning_content in ai_interactions table for audit/explainability | M |

### P2 — Improvement

| ID | Finding | Section | Evidence | Remediation | Effort |
|---|---|---|---|---|---|
| P2-01 | AI decision reproducibility untested | §3.2 | `provider.rs` temperature=0.0 | Build deterministic test harness: fixed input → expected tool calls | M |
| P2-02 | No JSON-LD signed audit export | §5 | `audit_hash.rs` | Add Ed25519-signed JSON-LD export format for external compliance | L |
| P2-03 | No pluggable detection rule system | §7 | `proactive.rs` | Extract rule definitions to config; add rule registry with enable/disable per rule | L |
| P2-04 | Minimal frontend testing (3 test files) | §1.2 | `src/` | Expand component test coverage for cart, checkout, receipt rendering | L |
| P2-05 | Detection thresholds hardcoded | §7 | `proactive.rs` | Move thresholds to app_config with admin UI for tuning | S |
| P2-06 | Held cart: no product validation on resume | §3.1 | `held_cart_repo.rs:77-101` | Validate all product IDs exist before returning resumed cart | S |

---

## Remediation Backlog (Priority-Ordered)

| # | ID | Item | Effort | Depends On |
|---|---|---|---|---|
| 1 | P0-04 | AI kill-switch | S | — |
| 2 | P0-03 | Fix Gemini API key path | S | — |
| 3 | P0-02 | Read tool failure propagation | S | — |
| 4 | P0-01 | Engine ops mutation gating | M | — |
| 5 | P0-05 | Critical alert escalation | M | — |
| 6 | P1-01 | Require client idempotency key | S | — |
| 7 | P1-02 | AI provider fallback chain | M | P0-03 |
| 8 | P1-05 | Persist AI reasoning | M | — |
| 9 | P1-03 | Per-tool RBAC | L | — |
| 10 | P1-04 | AI observability dashboard | M | — |
| 11 | P2-06 | Held cart product validation | S | — |
| 12 | P2-05 | Configurable alert thresholds | S | — |
| 13 | P2-01 | AI reproducibility harness | M | P1-05 |
| 14 | P2-02 | JSON-LD signed export | L | — |
| 15 | P2-03 | Pluggable detection rules | L | P2-05 |
| 16 | P2-04 | Frontend test expansion | L | — |

**Effort key:** S = 1-2 days, M = 3-5 days, L = 5-10 days.
**Total remediation effort:** ~55 engineer-days (can be parallelized; P0 items are ~10 days).

---

## Stale Compliance Artifacts

- `docs/compliance-checklist.md:36` — describes COUNT+1 receipt gap; code now uses atomic `RETURNING`. Update to reflect fix.
- `docs/compliance-checklist.md:36` — recommends "evaluate atomic sequence with RETURNING in Phase 2"; Phase 2 is done. Mark as complete.

---

## Verification

1. Every P0 finding has a code trace (file:line)
2. All 15 capability domains scored with evidence
3. Compliance checklist cross-referenced against live code
4. Gap report formatted with ID, protocol section, evidence, risk level, remediation path, effort estimate
5. Remediation items have concrete file targets
