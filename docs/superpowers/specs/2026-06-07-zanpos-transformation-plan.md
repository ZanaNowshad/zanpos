# ZANPOS Full-Spectrum Transformation Plan — 2026-06-07

> **Source audits:** VISUAL_GAPS.md (12 findings) · UX_GAPS.md (14 findings) · ENTERPRISE_GAPS.md (15 findings)  
> **Synthesis:** Deduplicated, conflict-resolved (cashier speed wins over visual polish when in conflict), risk-ranked by `(business_impact × compliance_risk) ÷ implementation_effort`  
> **Baseline:** 89 tests passing, 0 compile errors, 0 TypeScript errors as of 2026-06-07

---

## All Findings — Ranked

| ID | Dimension | Component | Finding | Severity | Effort | Files |
|----|-----------|-----------|---------|----------|--------|-------|
| T01 | Enterprise | shift_repo | SELECT-then-INSERT race in `open_shift` — no UNIQUE index on (device_id, status='open'). Two concurrent opens create ghost shift. | P0 | S | `shift_repo.rs:201`, migration |
| T02 | Enterprise | admin_commands | 8× `let _ = audit_hash::insert_audit_entry(...)` silently swallows audit failures — NBR compliance exposure | P0 | S | `admin_commands.rs:343,495,671,763,1261,1307,1362,1481` |
| T03 | UX | PaymentModal | Enter / NumpadEnter key does NOT confirm payment — forces touch tap on every sale | P0 | S | `PaymentModal.tsx` |
| T04 | UX + Visual | ProductGrid | Empty state is bare "No products found." — no icon, no guidance, no CTA. First-launch blocker. | P0 | S | `ProductGrid.tsx:42–46`, `App.css` |
| T05 | UX | SyncChip | Offline state shows state, not instruction. "Offline" with no "your sales are safe, keep selling" explanation causes cashier panic. | P0 | S | `SyncChip.tsx` |
| T06 | Enterprise | sale_repo | Loyalty point UPDATE at line 443 is post-commit (uses pool, not tx). Crash between commit and loyalty update silently loses points. | P1 | S | `sale_repo.rs:436–449` |
| T07 | Enterprise | admin_commands | Bulk import SQL errors silently swallowed (lines 1076, 1090, 1108) — partial imports appear successful | P1 | S | `admin_commands.rs:1076–1110` |
| T08 | Enterprise | customer_commands | `let _ = audit_hash::insert_audit_entry(...)` at line 156 — customer mutations silently unaudited on failure | P1 | S | `customer_commands.rs:156` |
| T09 | Enterprise | migration_commands | `let _ = audit_hash::insert_audit_entry(...)` at line 859 — bulk migration silently unaudited on failure | P1 | S | `migration_commands.rs:859` |
| T10 | UX | ShiftModal / PaymentModal | Cold start to first sale requires 7 interactions minimum. Target is 5. Two saves: auto-skip 0-float shift open, skip payment picker for single-method exact-cash. | P1 | M | `ShiftModal.tsx`, `PaymentModal.tsx` |
| T11 | UX | shift_commands | Shift double-open already a P0 DB fix (T01). UX side: the error message "A shift is already open for this device" should be surfaced as a clear toast, not a raw error. | P1 | S | `shift_commands.rs`, `errors.rs` |
| T12 | Enterprise | delivery_commands | WA notification failures (`let _ =` at lines 58, 72, 95) are best-effort but **completely invisible** — no log, no trace, no metric | P1 | S | `delivery_commands.rs:58,72,95` |
| T13 | Enterprise | sync/worker | Clock skew: retention prune uses `created_at < cutoff` — verify `AND sync_status = 'synced'` guard is present on both audit_log and stock_movements DELETE | P1 | S | `sync/worker.rs` |
| T14 | Enterprise | ai/tools | AI-executed mutations need `insert_audit_entry(..., "ai", ...)` — verify every tool that writes to DB has audit entry with actor_type="ai" | P1 | M | `ai/tools.rs`, `ai_admin_commands.rs` |
| T15 | Enterprise | sync/central_schema | Supabase RPC upserts — verify every entity type uses `ON CONFLICT DO UPDATE`, not bare INSERT, for idempotent retry safety | P1 | M | `sync/central_schema.rs` |
| T16 | Visual | AdminChatPage | 30+ tool entries use hardcoded hex colors (#6366f1, #0ea5e9, etc.) — completely outside CSS variable system | major | M | `AdminChatPage.tsx:76–130` |
| T17 | Visual + UX | AdminChatPage | Emoji (📊 💲 📦 ⚠) used as functional tool-category icons — should be Lucide icons | major | M | `AdminChatPage.tsx:76–130` |
| T18 | Visual | All modals | `modal-in` exists but NO `modal-out` — modals snap closed with zero animation, breaking the premium feel | major | S | `App.css:1949`, modal components |
| T19 | Visual | StickyNotesPanel | Hardcoded note colors (#fef08a, #bbf7d0 etc.) outside CSS variable system | major | S | `StickyNotesPanel.tsx:9–13`, `App.css:root` |
| T20 | UX | HoldModal | Held carts show generic labels ("Held Cart 1") — cashier can't distinguish 3 queued customers | P1 | M | `HoldModal.tsx`, `held_cart_commands.rs` |
| T21 | UX | BarcodeInput / errors | Unknown barcode error message needs operational English: what happened + what to do | P1 | S | `errors.rs`, `BarcodeInput.tsx` |
| T22 | UX | errors.rs | Several AppError variants produce developer-facing messages. Audit and rewrite for operational English. | P1 | M | `errors.rs` |
| T23 | UX | CartPanel | No visual success feedback after finalize_sale — cart just clears. No "Sale complete" moment. | P2 | S | `CartPanel.tsx`, `App.css` |
| T24 | UX | PosPage | No "Refund by receipt #" quick entry on POS screen — cashier needs 4+ taps through Back Office | P2 | M | `PosPage.tsx`, `RefundModal.tsx` |
| T25 | UX | Dialpad | Manager override PIN dialpad — verify touch targets ≥ 56px for 15-inch touchscreen compliance | P2 | S | `App.css` (dialpad section) |
| T26 | UX | PosPage | Active category tab does not auto-scroll into view when selected while off-screen | P2 | S | `PosPage.tsx` |
| T27 | Visual | CustomersTab / ReportsTab | Empty state icons are raw emoji — replace with Lucide `UserX`, `BarChart2`, `Receipt` | minor | S | `CustomersTab.tsx:122`, `ReportsTab.tsx:283,319,384` |
| T28 | Visual | App.css | No `--transition-fast` / `--transition-base` / `--transition-slow` design tokens — 9 different timing values scattered | minor | M | `App.css:1–110` |
| T29 | Visual | App.css | 3× `transition: all` — triggers layout-property repaints on scroll (lines 2150, 2259, 2342) | minor | S | `App.css:2150,2259,2342` |
| T30 | Visual | ProductGrid | No skeleton loading state while products fetch — grid area blank during load | minor | S | `ProductGrid.tsx:42`, `App.css` |
| T31 | Visual | BarcodeInput | Error shake (scan-flash) only flashes border — no physical shake transform on bad scan | minor | S | `App.css:798–810` |
| T32 | UX | PosPage | Printer status not shown proactively — cashier only discovers printer offline when print fails | P2 | M | `PosPage.tsx`, `thermal_commands.rs` |
| T33 | UX | ConfirmActionModal | Audit 3 usages — any guarding a reversible action should be replaced with undo-toast | P2 | S | `ConfirmActionModal.tsx` usages |
| T34 | Visual | Global motion | `modal-in` uses premium spring `cubic-bezier(0.2,0,0,1)` but buttons/cards use `ease` — inconsistent motion personality | minor | M | `App.css` throughout |

---

## TOP 15 — Highest Leverage Changes

### T01 · Shift UNIQUE index (P0 Enterprise · Effort: S)
**Risk:** Ghost shifts from concurrent opens corrupt EOD cashup totals.  
**Before:** `open_shift` does `SELECT → if None → INSERT`. Two concurrent callers both see None.  
**After:** `CREATE UNIQUE INDEX IF NOT EXISTS idx_shifts_one_open_per_device ON shifts(device_id) WHERE status = 'open'` — second INSERT fails with constraint error, caught as `AppError::Conflict`.  
**Files:** `shift_repo.rs:221` (INSERT) + next migration file  
**Est. lines:** 3 (migration SQL) + 0 (code change)

---

### T02 · Audit failure logging (P0 Enterprise · Effort: S)
**Risk:** NBR compliance — price changes and product mutations untracked when audit DB errors.  
**Before:** `admin_commands.rs:343` → `let _ = audit_hash::insert_audit_entry(...).await;`  
**After:** `if let Err(e) = audit_hash::insert_audit_entry(...).await { tracing::error!("AUDIT WRITE FAILED [{}]: {:?}", event_type, e); }`  
**Files:** `admin_commands.rs:343,495,671,763,1261,1307,1362,1481`, `customer_commands.rs:156`, `migration_commands.rs:859`  
**Est. lines:** 18 (9 sites × 2 lines each)

---

### T03 · Enter key confirms payment (P0 UX · Effort: S)
**Risk:** Every sale requires a mouse/touch tap even with keyboard nearby. Highest-frequency friction in the entire app.  
**Before:** No keyboard handler in PaymentModal.  
**After:** `PaymentModal.tsx` — add `useEffect(() => { const handler = (e: KeyboardEvent) => { if ((e.key === 'Enter' || e.code === 'NumpadEnter') && canCharge) handleCharge(); }; window.addEventListener('keydown', handler); return () => window.removeEventListener('keydown', handler); }, [canCharge]);`  
**Files:** `PaymentModal.tsx`  
**Est. lines:** 8

---

### T04 · ProductGrid warm empty state (P0 UX + Visual · Effort: S)
**Risk:** First-launch experience — cashier sees a void with no guidance.  
**Before:** `ProductGrid.tsx:43` → `<div className="product-grid-msg" role="status">No products found.</div>`  
**After:** Structured empty state with Lucide `PackageSearch`, heading "Ready to start selling?", hint "Your manager needs to add products — open Back Office → Products", optional manager CTA button.  
**Files:** `ProductGrid.tsx:42–46`, `App.css` (add `.product-grid-empty` styles)  
**Est. lines:** 20

---

### T05 · SyncChip offline instruction (P0 UX · Effort: S)
**Risk:** Cashier panic on every connectivity drop — no reassurance that data is safe.  
**Before:** `SyncChip.tsx` shows "Offline" state only.  
**After:** Add subtitle: "Sales saved locally — sync resumes automatically." + `?` tooltip with full explanation.  
**Files:** `SyncChip.tsx`  
**Est. lines:** 12

---

### T06 · Loyalty update inside transaction (P1 Enterprise · Effort: S)
**Risk:** Process crash between `tx.commit()` (line 436) and loyalty UPDATE (line 443) silently loses customer points.  
**Before:** `sale_repo.rs:443` — `sqlx::query("UPDATE customers...").execute(pool).await`  
**After:** Move the loyalty UPDATE inside `tx` before `tx.commit()`: `.execute(&mut *tx).await` and bind before line 436.  
**Files:** `sale_repo.rs:436–449`  
**Est. lines:** 5 (move + rebind)

---

### T07 · Bulk import error collection (P1 Enterprise · Effort: S)
**Risk:** Silent partial imports — owner thinks 500 products imported, actually 487 imported with 13 silently skipped.  
**Before:** `admin_commands.rs:1076,1090,1108` → `let _ = sqlx::query(...)`  
**After:** Collect errors into `errors: Vec<String>` per row, return `{ success_count, error_count, errors }` from command.  
**Files:** `admin_commands.rs:1076–1110`  
**Est. lines:** 15

---

### T12 · WA notification logging (P1 Enterprise · Effort: S)
**Risk:** WhatsApp failures are production-invisible — operator can't diagnose "why didn't the customer get notified?"  
**Before:** `delivery_commands.rs:58` → `let _ = whatsapp_send_delivery_impl(...).await;`  
**After:** `if let Err(e) = whatsapp_send_delivery_impl(...).await { tracing::warn!("WA send failed for delivery {}: {:?}", delivery_order_id, e); }`  
**Files:** `delivery_commands.rs:58,72,95`  
**Est. lines:** 6

---

### T18 · Modal close animation (major Visual · Effort: S)
**Risk:** Premium open, jarring close — every modal dismiss breaks the premium feel.  
**Before:** `App.css:1949` — `modal-in` only. No modal-out keyframe.  
**After:** Add `@keyframes modal-out { from { opacity:1; transform:scale(1); } to { opacity:0; transform:scale(0.97); } }` + JS: on close, add `.closing` class, wait 120ms, then unmount.  
**Files:** `App.css:1949`, all modal components (add `onClose` animation hook)  
**Est. lines:** 8 CSS + ~5 per modal component

---

### T16+T17 · AdminChatPage color + icon system (major Visual · Effort: M)
**Risk:** 30+ hardcoded hex colors break theme consistency. Emoji icons look unprofessional in an enterprise context.  
**Before:** `AdminChatPage.tsx:76` → `{ icon: "📊", color: "#6366f1" }`  
**After:** Map to 6 semantic CSS variable buckets + Lucide icon names: `{ icon: BarChart2, colorVar: '--info' }`, render `<Icon size={14} style={{color: \`var(${colorVar})\`}} />`  
**Files:** `AdminChatPage.tsx:76–130`  
**Est. lines:** 60 (map rewrite) + 10 (render update)

---

### T20 · Held cart customer labels (P1 UX · Effort: M)
**Risk:** Queue confusion — cashier can't distinguish 3 simultaneous customers' carts.  
**Before:** HoldModal shows "Held Cart 1", "Held Cart 2".  
**After:** Prompt for optional note on hold ("Customer name / table"); display prominently on each held cart card. 2-second auto-dismiss on the prompt.  
**Files:** `HoldModal.tsx`, `held_cart_commands.rs`  
**Est. lines:** 30

---

### T21 · Unknown barcode operational message (P1 UX · Effort: S)
**Risk:** "GhostBarcode" error confuses cashier — they don't know item is flagged for manager resolution.  
**Before:** Generic error toast on unknown scan.  
**After:** Toast: "Unknown barcode recorded. Your manager can look it up in Back Office → Products → Unknown Barcodes."  
**Files:** `errors.rs` (GhostBarcode Display impl), `BarcodeInput.tsx`  
**Est. lines:** 8

---

### T13 · Retention prune guard verification (P1 Enterprise · Effort: S)
**Risk:** If `AND sync_status = 'synced'` was not applied by June-05 fix, unsynced audit logs could be pruned.  
**Before:** Unknown — verify current state.  
**After:** `grep -n "DELETE FROM audit_logs\|DELETE FROM stock_movements" src-tauri/src/sync/worker.rs` — confirm both have `AND sync_status = 'synced'`. Add if missing.  
**Files:** `sync/worker.rs`  
**Est. lines:** 0–4 (conditional on verification result)

---

### T19 · Sticky note colors into design system (major Visual · Effort: S)
**Risk:** StickyNotesPanel uses Tailwind-palette hex values that don't adapt to the gold theme or future theming.  
**Before:** `StickyNotesPanel.tsx:9` → `{ bg: "#fef08a", border: "#eab308" }`  
**After:** Add to `App.css :root`: `--note-yellow-bg: rgba(240,165,0,0.15); --note-yellow-border: var(--accent);` etc. Use in StickyNotesPanel via CSS variables.  
**Files:** `StickyNotesPanel.tsx:9–13`, `App.css:1–110`  
**Est. lines:** 12

---

### T23 · Sale success flash (P2 UX + Visual · Effort: S)
**Risk:** No positive feedback after every sale — missed opportunity for cashier confidence and rhythm.  
**Before:** Cart clears silently after finalize.  
**After:** After successful finalize, briefly apply `.cart-success-flash` to cart panel: 2× `box-shadow: 0 0 0 3px var(--success)` pulse over 400ms, then clear.  
**Files:** `CartPanel.tsx`, `App.css`  
**Est. lines:** 10

---

## Implementation Order (respects dependencies)

**Phase 1 — P0 fixes (run first, no dependencies):**
T01 → T02 → T03 → T04 → T05

**Phase 2 — P1 enterprise hardening:**
T06 → T07 → T08 → T09 → T12 → T13

**Phase 3 — P1 UX improvements:**
T10 → T11 → T20 → T21 → T22

**Phase 4 — Visual + P2 polish:**
T16+T17 (merge) → T18 → T19 → T23 → T24 → T25 → T26 → T27 → T28 → T29 → T30 → T31

**Phase 5 — Verification:**
T14 (AI audit trail audit) → T15 (Supabase RPC idempotency audit) → T32 → T33 → T34

---

## Conflict Resolutions

| Visual want | UX reality | Resolution |
|-------------|------------|------------|
| Modal-out animation (120ms) | Could add latency to modal close | Resolve: 120ms is below perception threshold; proceed |
| Cart success flash animation | Could distract cashier starting next sale | Resolve: flash triggers on clear, which resets cart anyway; no conflict |
| Transition token system | Pure CSS refactor, no UX impact | Resolve: Phase 4, low risk |
| StickyNote colors | No UX impact | Resolve: Phase 4 |
