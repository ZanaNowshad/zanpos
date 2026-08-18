# ZanAI Shared Runtime and POS Widget Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give every authenticated POS user a floating ZanAI widget backed by the same conversation runtime as OfficeAI, with cashier read-only enforcement, visible till context, scanner protection, and checkout-safe minimization.

**Architecture:** Mount one `useChatController` instance in an authenticated `ZanAiProvider` above the POS/OfficeAI view switch. OfficeAI and the new POS widget consume the same controller; a surface-scoped send context supplies either OfficeAI tab context or a redacted POS snapshot. Rust derives the user's role from the session token, filters cashier-visible tools to an explicit read allowlist, and rechecks role at dispatch.

**Tech Stack:** React 19, TypeScript strict mode, Tauri v2 IPC/Channel, Rust 2021, sqlx/SQLite, Vitest, existing ZANPOS CSS/i18n systems.

**Spec:** `docs/superpowers/specs/2026-08-14-zanai-shared-runtime-pos-widget-design.md`

## Global Constraints

- Money remains integer minor units (`number` in TypeScript, `i64` in Rust); no floating-point money calculations.
- Frontend communicates with Rust only through wrappers in `src/tauri/commands.ts`.
- Session tokens, PINs, payment credentials, API keys, and private customer data never enter POS context or logs.
- Cashier read-only access is enforced from the backend-derived role, never from a client-supplied role or surface flag.
- Every mutation remains subject to existing confirmation, scope, hash, audit, expiry, cancellation, and undo policies.
- The widget never outranks critical POS dialogs and must not break F2/F3, F9–F12, barcode scanning, payment, refund, or shift-close flows.
- Preserve unrelated dirty worktree changes; stage and commit only files listed by each task.
- The worktree already contains unrelated staged user changes. Every task commit must use
  `git commit --only ... -- <task paths>` and must be inspected with `git show --stat --oneline HEAD`
  so unrelated staged content remains untouched.
- Use `npm run tauri dev` for runtime testing; `npm run dev` is insufficient.
- Do not add a new state-management dependency.

## File Map

### New frontend files

- `src/zanai/zanAiTypes.ts` — surface context, POS snapshot, navigation request, shared UI-state contracts.
- `src/zanai/zanAiState.ts` — pure identity, unread, and surface-state transitions.
- `src/zanai/ZanAiProvider.tsx` — one shared controller and context registration.
- `src/zanai/useZanAi.ts` — guarded context consumer.
- `src/zanai/posContext.ts` — redacted, bounded POS context builder/serializer.
- `src/zanai/widgetState.ts` — pure geometry and suppression transitions.
- `src/zanai/scannerBurst.ts` — pure burst classifier and draft-suffix removal.
- `src/zanai/usePosZanAiWindow.ts` — local persistence and pointer/keyboard window behavior.
- `src/zanai/useScannerBurstGuard.ts` — connects classifier to the chat composer and POS barcode handler.
- `src/zanai/PosZanAiWidget.tsx` — launcher, floating shell, context chip, and reused `ChatPanel`.
- `src/zanai/zanAiWidget.css` — widget-only responsive styling.

### Modified frontend files

- `src/App.tsx` — mount provider for authenticated normal flows.
- `src/officeai/useChatController.ts` — accept a per-send surface context.
- `src/officeai/OfficeAIPage.tsx` — consume shared controller and navigation requests.
- `src/officeai/ChatPanel.tsx` — add compact POS variant and scanner-aware send hook.
- `src/pages/PosPage.tsx` — build context, derive suppression, mount widget.
- `src/i18n/officeAiStrings.ts` and locale files — widget labels/status text.
- `src/App.css` only if a pre-existing global overlay token is required; widget layout stays in `zanAiWidget.css`.

### Modified backend files

- `src-tauri/src/auth_session.rs` — resolve any active POS role without weakening `resolve_office`.
- `src-tauri/src/ai/tool_policy.rs` — actor-tier filtering and dispatch-time denial.
- `src-tauri/src/commands/ai_admin_commands.rs` — use chat authorization for chat/history/cancel/feedback commands while retaining Office authorization for configuration and mutations.
- `src-tauri/src/ai/streaming.rs` and `src-tauri/src/ai/tools.rs` — pass actor role into filtering/dispatch.
- `src-tauri/src/ai/tool_subsetting.rs` — subset only the already role-authorized definitions.

### Tests

- `src/__tests__/zanAiSharedRuntime.test.ts`
- `src/__tests__/zanAiPosContext.test.ts`
- `src/__tests__/zanAiWidgetState.test.ts`
- `src/__tests__/zanAiScannerBurst.test.ts`
- Existing Rust unit modules in the modified Rust files.
- Extend `src/__tests__/operatorWorkflowUx.test.tsx` for POS widget shell presence.

---

### Task 1: Define shared frontend contracts and state invariants

**Files:**
- Create: `src/zanai/zanAiTypes.ts`
- Create: `src/zanai/zanAiState.ts`
- Test: `src/__tests__/zanAiSharedRuntime.test.ts`

**Interfaces:**
- Produces: `ZanAiSurface`, `ZanAiSurfaceContext`, `ZanAiRuntimeIdentity`, `ZanAiUiState`, `sameRuntimeIdentity`, `reduceZanAiUiState`.
- Consumed by: Tasks 2–7.

- [ ] **Step 1: Write failing identity and unread-state tests**

```ts
import { describe, expect, it } from "vitest";
import { reduceZanAiUiState, sameRuntimeIdentity } from "../zanai/zanAiState";

describe("shared ZanAI runtime identity", () => {
  it("keeps one runtime only for the same token, user, and branch", () => {
    const a = { sessionToken: "t1", userId: "u1", branchId: "b1" };
    expect(sameRuntimeIdentity(a, { ...a })).toBe(true);
    expect(sameRuntimeIdentity(a, { ...a, sessionToken: "t2" })).toBe(false);
    expect(sameRuntimeIdentity(a, { ...a, branchId: "b2" })).toBe(false);
  });
});

describe("shared ZanAI UI state", () => {
  it("counts a completed reply only when its surface is not visible", () => {
    const initial = { activeSurface: "pos" as const, widgetOpen: false, widgetExpanded: false, unreadCount: 0 };
    expect(reduceZanAiUiState(initial, { type: "assistant_result" }).unreadCount).toBe(1);
    expect(reduceZanAiUiState({ ...initial, widgetOpen: true }, { type: "assistant_result" }).unreadCount).toBe(0);
  });

  it("opening the widget clears unread without altering the conversation", () => {
    const next = reduceZanAiUiState(
      { activeSurface: "pos", widgetOpen: false, widgetExpanded: false, unreadCount: 3 },
      { type: "open_widget" },
    );
    expect(next).toEqual({ activeSurface: "pos", widgetOpen: true, widgetExpanded: false, unreadCount: 0 });
  });
});
```

- [ ] **Step 2: Run the focused test and verify RED**

Run: `npm test -- src/__tests__/zanAiSharedRuntime.test.ts`  
Expected: FAIL because `../zanai/zanAiState` does not exist.

- [ ] **Step 3: Implement the contracts and pure reducer**

```ts
export type ZanAiSurface = "pos" | "office";

export interface ZanAiRuntimeIdentity {
  sessionToken: string;
  userId: string;
  branchId: string;
}

export interface ZanAiSurfaceContext {
  surface: ZanAiSurface;
  summary: string;
  structured?: unknown;
}

export interface ZanAiUiState {
  activeSurface: ZanAiSurface;
  widgetOpen: boolean;
  widgetExpanded: boolean;
  unreadCount: number;
}

export type ZanAiUiAction =
  | { type: "set_surface"; surface: ZanAiSurface }
  | { type: "open_widget" }
  | { type: "minimize_widget" }
  | { type: "assistant_result" };
```

Implement `sameRuntimeIdentity` with exact field equality. Implement `reduceZanAiUiState` so assistant results count only when POS is active and the widget is closed; opening clears unread.

- [ ] **Step 4: Run focused test and type-check**

Run: `npm test -- src/__tests__/zanAiSharedRuntime.test.ts && npm run type-check`  
Expected: PASS.

- [ ] **Step 5: Commit only Task 1 files**

```powershell
git add -- src/zanai/zanAiTypes.ts src/zanai/zanAiState.ts src/__tests__/zanAiSharedRuntime.test.ts
git commit --only -m "feat: define shared ZanAI runtime state" -- src/zanai/zanAiTypes.ts src/zanai/zanAiState.ts src/__tests__/zanAiSharedRuntime.test.ts
```

---

### Task 2: Mount one shared controller above POS and OfficeAI

**Files:**
- Create: `src/zanai/ZanAiProvider.tsx`
- Create: `src/zanai/useZanAi.ts`
- Modify: `src/officeai/useChatController.ts`
- Modify: `src/App.tsx`
- Test: `src/__tests__/zanAiSharedRuntime.test.ts`

**Interfaces:**
- Consumes: Task 1 contracts.
- Produces: `ZanAiProvider`, `useZanAi()`, `registerSurfaceContext`, `navigationRequest`, `consumeNavigationRequest`, `dataEpoch`.

- [ ] **Step 1: Add failing tests for per-send context selection**

Add pure helper coverage to `zanAiSharedRuntime.test.ts`:

```ts
it("uses the context supplied by the sending surface", () => {
  const office = { surface: "office" as const, summary: "Catalogue / Products" };
  const pos = { surface: "pos" as const, summary: "Till · 2 items" };
  expect(selectSendContext(office, pos)).toBe(pos);
});
```

The production change that makes this pass is an exported `selectSendContext(registered, explicit)` helper returning `explicit ?? registered`.

- [ ] **Step 2: Verify RED**

Run: `npm test -- src/__tests__/zanAiSharedRuntime.test.ts`  
Expected: FAIL because `selectSendContext` is absent.

- [ ] **Step 3: Extend the controller send contract**

Change:

```ts
handleSend: (overrideText?: string, sendContext?: ZanAiSurfaceContext) => Promise<void>;
```

At the `aiChatStream` call, send:

```ts
ui_context: serializeSurfaceContext(
  selectSendContext({ surface: "office", summary: getUiContext() }, sendContext),
),
```

`serializeSurfaceContext` must use `JSON.stringify` for structured POS data and preserve the current OfficeAI summary behavior.

- [ ] **Step 4: Implement the provider**

The provider must:

```tsx
const ctrl = useChatController({
  sessionUser,
  getUiContext: () => registeredContextRef.current.summary,
  onNavigate: tab => setNavigationRequest({ id: crypto.randomUUID(), tab }),
  onMutationApplied: () => setDataEpoch(value => value + 1),
});
```

Expose stable callbacks through context. Use `useMemo` for the context value. Do not copy any controller field into separate state.

- [ ] **Step 5: Mount the provider in `App.tsx`**

Render `ZanAiProvider` as a component boundary around the existing authenticated normal-flow view switch. Keep `useChatController` inside that provider component, so `App` does not call hooks conditionally. The provider must wrap both `view === "pos"` and `view === "office_ai"`; it must not wrap login, setup, migration, or lock screens.

- [ ] **Step 6: Verify focused tests and frontend gates**

Run: `npm test -- src/__tests__/zanAiSharedRuntime.test.ts && npm run type-check && npm run lint`  
Expected: PASS with zero warnings.

- [ ] **Step 7: Commit Task 2 files**

```powershell
git add -- src/zanai/ZanAiProvider.tsx src/zanai/useZanAi.ts src/zanai/zanAiState.ts src/officeai/useChatController.ts src/App.tsx src/__tests__/zanAiSharedRuntime.test.ts
git commit --only -m "feat: mount one shared ZanAI controller" -- src/zanai/ZanAiProvider.tsx src/zanai/useZanAi.ts src/zanai/zanAiState.ts src/officeai/useChatController.ts src/App.tsx src/__tests__/zanAiSharedRuntime.test.ts
```

---

### Task 3: Migrate OfficeAI to the shared runtime without behavior changes

**Files:**
- Modify: `src/officeai/OfficeAIPage.tsx`
- Modify: `src/officeai/OfficeAIAssistantWorkspace.tsx` only if props can be simplified without duplication.
- Test: `src/__tests__/officeAiAssistantMinimal.test.tsx`
- Test: `src/__tests__/zanAiSharedRuntime.test.ts`

**Interfaces:**
- Consumes: `useZanAi()` from Task 2.
- Produces: shared navigation-request consumption and OfficeAI surface context registration.

- [ ] **Step 1: Add a failing regression for shared-controller consumption**

Add a source-boundary test that imports `OfficeAIPage` through a mocked `useZanAi` module and renders its stable assistant shell. Assert the mock controller's existing message is present. Do not assert that a mock element exists; assert the real `ChatPanel` output contains the message text.

- [ ] **Step 2: Verify RED**

Run: `npm test -- src/__tests__/officeAiAssistantMinimal.test.tsx`  
Expected: FAIL because `OfficeAIPage` still constructs a local controller.

- [ ] **Step 3: Replace local construction with shared consumption**

Remove the `useChatController` import and call. Use:

```ts
const {
  ctrl,
  navigationRequest,
  consumeNavigationRequest,
  registerSurfaceContext,
  dataEpoch,
} = useZanAi();
```

Register `{ surface: "office", summary: getUiContext() }` whenever the active tab/context changes. Consume navigation requests only after the target passes existing `canOpenOfficeTab` checks.

- [ ] **Step 4: Preserve mutation refresh semantics**

Replace the previous local `onMutationApplied` callback with an effect keyed by `dataEpoch`. Do not double-refresh on the same epoch.

- [ ] **Step 5: Run OfficeAI regressions**

Run: `npm test -- src/__tests__/officeAiAssistantMinimal.test.tsx src/__tests__/officeAiNav.test.ts src/__tests__/actionLifecycle.test.ts`  
Expected: PASS.

- [ ] **Step 6: Commit Task 3 files**

```powershell
git add -- src/officeai/OfficeAIPage.tsx src/officeai/OfficeAIAssistantWorkspace.tsx src/__tests__/officeAiAssistantMinimal.test.tsx
git commit --only -m "refactor: share ZanAI runtime with OfficeAI" -- src/officeai/OfficeAIPage.tsx src/officeai/OfficeAIAssistantWorkspace.tsx src/__tests__/officeAiAssistantMinimal.test.tsx
```

---

### Task 4: Build deterministic, redacted POS context

**Files:**
- Create: `src/zanai/posContext.ts`
- Test: `src/__tests__/zanAiPosContext.test.ts`
- Modify: `src/zanai/zanAiTypes.ts`

**Interfaces:**
- Consumes: `Cart`, `Shift`, `SessionUser`, `SyncStatus`, device/branch identifiers.
- Produces: `buildPosAiContext(input): PosAiContext`, `summarizePosAiContext(context): string`.

- [ ] **Step 1: Write failing context tests**

Cover these literal expectations:

```ts
expect(context.cart.total_minor).toBe(1800);
expect(context.cart.lines[0]).toEqual({
  product_id: "p1",
  name: "Milk",
  barcode: "6280001",
  quantity: "2",
  unit_price_minor: 650,
  line_total_minor: 1300,
});
expect(JSON.stringify(context)).not.toContain("session_token");
expect(JSON.stringify(context)).not.toContain("pin");
```

Add a 60-line cart fixture and assert output has 40 lines plus `{ truncated_line_count: 20 }`.

- [ ] **Step 2: Verify RED**

Run: `npm test -- src/__tests__/zanAiPosContext.test.ts`  
Expected: FAIL because `posContext.ts` does not exist.

- [ ] **Step 3: Implement a strict input contract**

```ts
export interface BuildPosAiContextInput {
  cart: Cart;
  shift: Shift;
  user: Pick<SessionUser, "user_id" | "display_name" | "branch_id">;
  branchName: string;
  deviceId: string;
  netTotalMinor: number;
  taxTotalMinor: number;
  syncStatus: SyncStatus | null;
  capturedAt: string;
}
```

Filter voided lines, preserve quantity as its decimal string, calculate item count from non-voided lines, copy only allowlisted fields, and never spread source objects.

- [ ] **Step 4: Run context tests and type-check**

Run: `npm test -- src/__tests__/zanAiPosContext.test.ts && npm run type-check`  
Expected: PASS.

- [ ] **Step 5: Commit Task 4 files**

```powershell
git add -- src/zanai/posContext.ts src/zanai/zanAiTypes.ts src/__tests__/zanAiPosContext.test.ts
git commit --only -m "feat: build redacted ZanAI till context" -- src/zanai/posContext.ts src/zanai/zanAiTypes.ts src/__tests__/zanAiPosContext.test.ts
```

---

### Task 5: Enforce cashier read-only access in Rust

**Files:**
- Modify: `src-tauri/src/auth_session.rs`
- Modify: `src-tauri/src/ai/tool_policy.rs`
- Modify: `src-tauri/src/commands/ai_admin_commands.rs`
- Modify: `src-tauri/src/ai/tools.rs`
- Modify: `src-tauri/src/ai/streaming.rs`
- Modify: `src-tauri/src/ai/tool_subsetting.rs`

**Interfaces:**
- Produces: `SessionStore::resolve_ai`, `AiAccessTier`, `filter_definitions_for_role`, `require_role_allows_tool`.
- Invariant: `resolve_office` remains manager/owner-only for settings, mutations, and OfficeAI administration.

- [ ] **Step 1: Add failing auth-session tests**

Extend `auth_session.rs`:

```rust
#[tokio::test]
async fn ai_resolution_accepts_active_cashier_without_weakening_office_resolution() {
    let pool = pool().await;
    let store = SessionStore::new(Duration::from_secs(60));
    let cashier = store.issue("01JUSER000000000000CASH01").await;
    assert_eq!(store.resolve_ai(&pool, &cashier.token).await.unwrap().role_name, "cashier");
    assert!(store.resolve_office(&pool, &cashier.token).await.is_err());
}
```

- [ ] **Step 2: Verify auth RED**

Run from `src-tauri`: `cargo test --lib auth_session::tests::ai_resolution_accepts_active_cashier_without_weakening_office_resolution`  
Expected: FAIL because `resolve_ai` is absent.

- [ ] **Step 3: Implement `resolve_ai` without duplicating token parsing**

Extract the common active-session lookup into a private method. `resolve_ai` queries any active role in `('cashier','manager','owner')`; `resolve_office` retains its existing manager/owner filter and error.

- [ ] **Step 4: Add failing role-policy tests**

In `tool_policy.rs` assert:

```rust
#[test]
fn cashier_catalogue_contains_only_explicit_operational_reads() {
    let definitions = crate::ai::tools::all_tool_definitions();
    let filtered = filter_definitions_for_role(&definitions, "cashier").unwrap();
    assert!(filtered.iter().any(|d| d.name == "lookup_barcode"));
    assert!(filtered.iter().any(|d| d.name == "get_stock_levels"));
    assert!(filtered.iter().all(|d| ToolRegistry::global().unwrap().get(&d.name).unwrap().kind == ToolKind::Read));
    assert!(!filtered.iter().any(|d| d.name == "list_users"));
}

#[test]
fn cashier_direct_mutation_dispatch_is_denied() {
    assert!(require_role_allows_tool("cashier", "update_product_price").is_err());
}
```

- [ ] **Step 5: Implement explicit cashier allowlist and dispatch guard**

Define the initial allowlist as a constant containing operational reads only:

```rust
const CASHIER_AI_READ_TOOLS: &[&str] = &[
    "lookup_barcode", "search_products", "get_product", "get_product_detail",
    "get_stock_levels", "get_active_shift", "get_user_shift_summary",
    "get_sync_status", "get_whatsapp_status", "list_deliveries",
];
```

`filter_definitions_for_role` returns the full already-enabled catalogue for manager/owner and the explicit intersection for cashier. `require_role_allows_tool` validates role, registry membership, read kind, and allowlist membership.

- [ ] **Step 6: Wire role through chat only**

Use `authorize_ai_chat`/`resolve_ai` for streaming chat, cancel, history load/clear, task-resume, and feedback commands needed by the widget. Keep `authorize_office` on provider configuration, alerts administration, mutation execute/cancel/undo, run execution, and every management command.

Filter enabled definitions by role before message-domain subsetting. Pass the authenticated `role_name` into the dispatch closure and call `require_role_allows_tool` immediately before every tool execution. Never trust `ui_context.surface` for authorization.

- [ ] **Step 7: Run Rust policy tests**

Run from `src-tauri`:

```text
cargo fmt --all -- --check
cargo test --lib auth_session::tests
cargo test --lib tool_policy::tests
cargo test --lib ai_admin_commands::
```

Expected: PASS; existing office-resolution cashier-rejection test remains green.

- [ ] **Step 8: Commit Task 5 files**

```powershell
git add -- src-tauri/src/auth_session.rs src-tauri/src/ai/tool_policy.rs src-tauri/src/commands/ai_admin_commands.rs src-tauri/src/ai/tools.rs src-tauri/src/ai/streaming.rs src-tauri/src/ai/tool_subsetting.rs
git commit --only -m "feat: enforce cashier read-only ZanAI access" -- src-tauri/src/auth_session.rs src-tauri/src/ai/tool_policy.rs src-tauri/src/commands/ai_admin_commands.rs src-tauri/src/ai/tools.rs src-tauri/src/ai/streaming.rs src-tauri/src/ai/tool_subsetting.rs
```

---

### Task 6: Implement widget geometry and scanner protection as pure behavior

**Files:**
- Create: `src/zanai/widgetState.ts`
- Create: `src/zanai/scannerBurst.ts`
- Test: `src/__tests__/zanAiWidgetState.test.ts`
- Test: `src/__tests__/zanAiScannerBurst.test.ts`

**Interfaces:**
- Produces: `clampWidgetRect`, `applyWidgetSuppression`, `classifyScannerBurst`, `removeBurstSuffix`.
- Consumed by: Task 7.

- [ ] **Step 1: Write failing geometry tests**

Test exact viewport clamping, minimum `360×420`, maximum 70% viewport, invalid persisted numbers resetting to defaults, and suppression preserving voluntary open state:

```ts
expect(applyWidgetSuppression({ preferredOpen: true, visible: true }, true)).toEqual({
  preferredOpen: true,
  visible: false,
});
```

- [ ] **Step 2: Write failing scanner tests**

Use literal timed events:

```ts
const scan = [
  { key: "6", at: 0 }, { key: "2", at: 9 }, { key: "8", at: 18 },
  { key: "0", at: 27 }, { key: "0", at: 36 }, { key: "1", at: 45 },
  { key: "Enter", at: 54 },
];
expect(classifyScannerBurst(scan)).toEqual({ kind: "barcode", value: "628001" });
expect(classifyScannerBurst([{ key: "h", at: 0 }, { key: "i", at: 180 }, { key: "Enter", at: 500 }])).toEqual({ kind: "text" });
expect(removeBurstSuffix("please check 628001", "628001")).toBe("please check ");
```

- [ ] **Step 3: Verify RED**

Run: `npm test -- src/__tests__/zanAiWidgetState.test.ts src/__tests__/zanAiScannerBurst.test.ts`  
Expected: FAIL because production modules do not exist.

- [ ] **Step 4: Implement minimal pure behavior**

The scanner classifier requires at least 4 barcode characters, an Enter terminator, no whitespace, and a maximum 35ms average gap. Quantity-prefix grammar accepts `3*628001` and `3x628001`. Any ambiguity returns `{ kind: "text" }` and preserves the draft.

- [ ] **Step 5: Run focused tests**

Run: `npm test -- src/__tests__/zanAiWidgetState.test.ts src/__tests__/zanAiScannerBurst.test.ts && npm run type-check`  
Expected: PASS.

- [ ] **Step 6: Commit Task 6 files**

```powershell
git add -- src/zanai/widgetState.ts src/zanai/scannerBurst.ts src/__tests__/zanAiWidgetState.test.ts src/__tests__/zanAiScannerBurst.test.ts
git commit --only -m "feat: add checkout-safe ZanAI widget behavior" -- src/zanai/widgetState.ts src/zanai/scannerBurst.ts src/__tests__/zanAiWidgetState.test.ts src/__tests__/zanAiScannerBurst.test.ts
```

---

### Task 7: Build and integrate the floating POS widget

**Files:**
- Create: `src/zanai/usePosZanAiWindow.ts`
- Create: `src/zanai/useScannerBurstGuard.ts`
- Create: `src/zanai/PosZanAiWidget.tsx`
- Create: `src/zanai/zanAiWidget.css`
- Modify: `src/officeai/ChatPanel.tsx`
- Modify: `src/pages/PosPage.tsx`
- Modify: `src/i18n/officeAiStrings.ts`
- Modify: `src/i18n/locales/en/officeAi.json`
- Modify: `src/i18n/locales/ar/officeAi.json`
- Test: `src/__tests__/operatorWorkflowUx.test.tsx`

**Interfaces:**
- Consumes: shared controller, POS context, window/scanner pure functions, `handleBarcode`, `focusBarcode`.
- Produces: visible POS launcher/window and compact `ChatPanel` variant.

- [ ] **Step 1: Add failing POS shell regression**

Render `PosZanAiWidget` with a complete fake `ChatController` and assert real output contains an accessible button named “Open ZanAI”, unread count when nonzero, a dialog named “ZanAI”, and the removable “Till context” control when open.

- [ ] **Step 2: Verify RED**

Run: `npm test -- src/__tests__/operatorWorkflowUx.test.tsx`  
Expected: FAIL because `PosZanAiWidget` does not exist.

- [ ] **Step 3: Implement hooks and widget shell**

`usePosZanAiWindow` reads/writes only sanitized geometry and preference under:

```ts
`zanai-pos-window:${DEVICE.device_id}:${sessionUser.user_id}`
```

`useScannerBurstGuard` receives `(draft, setDraft, onBarcode)` and returns an `onKeyDown` handler. It removes a confirmed scanner suffix before calling `onBarcode`; human Enter delegates to `ctrl.handleSend(undefined, posContext)`.

- [ ] **Step 4: Add `variant="pos"` to `ChatPanel`**

The POS variant keeps messages, attachments, Stop, clear, export, and feedback. It hides manager-only confirmation/run controls for cashier through an explicit `canMutate` presentation prop, while backend enforcement remains authoritative.

- [ ] **Step 5: Integrate with `PosPage`**

Build `PosAiContext` with `useMemo` from cart/shift/totals/sync data and capture the timestamp only on Send. Derive:

```ts
const zanAiSuppressed = activeModal.kind !== "none" || payFastLoading || showSaleDetails;
```

Mount the widget after normal POS content and before modal overlay components in DOM order; CSS z-index keeps it below all critical overlays. Pass `handleBarcode` and `focusBarcode` directly.

- [ ] **Step 6: Add bilingual strings and parity coverage**

Add English/Arabic keys for open, minimize, close, expand, unread, till context, remove context, reset position, unavailable, retry, and read-only cashier status. Run locale parity tests.

- [ ] **Step 7: Run focused UI tests and gates**

Run:

```text
npm test -- src/__tests__/operatorWorkflowUx.test.tsx src/i18n/locales/parity.test.ts src/__tests__/zanAiPosContext.test.ts src/__tests__/zanAiWidgetState.test.ts src/__tests__/zanAiScannerBurst.test.ts
npm run type-check
npm run lint
```

Expected: PASS with zero warnings.

- [ ] **Step 8: Commit Task 7 files**

```powershell
git add -- src/zanai/usePosZanAiWindow.ts src/zanai/useScannerBurstGuard.ts src/zanai/PosZanAiWidget.tsx src/zanai/zanAiWidget.css src/officeai/ChatPanel.tsx src/pages/PosPage.tsx src/i18n/officeAiStrings.ts src/i18n/locales/en/officeAi.json src/i18n/locales/ar/officeAi.json src/__tests__/operatorWorkflowUx.test.tsx
git commit --only -m "feat: add floating ZanAI widget to POS" -- src/zanai/usePosZanAiWindow.ts src/zanai/useScannerBurstGuard.ts src/zanai/PosZanAiWidget.tsx src/zanai/zanAiWidget.css src/officeai/ChatPanel.tsx src/pages/PosPage.tsx src/i18n/officeAiStrings.ts src/i18n/locales/en/officeAi.json src/i18n/locales/ar/officeAi.json src/__tests__/operatorWorkflowUx.test.tsx
```

---

### Task 8: End-to-end verification and release evidence

**Files:**
- Create: `tests/e2e-tauri/specs/zanai-pos-widget.spec.ts`
- Modify: `tests/e2e-tauri/pageobjects/pos.page.ts`

**Interfaces:**
- Consumes: all prior tasks.
- Produces: verified shared-session, checkout-safety, scanner, and role-boundary evidence.

- [ ] **Step 1: Add E2E coverage using existing real-IPC fixtures**

Cover:

1. Manager opens POS widget, sends a read question, enters OfficeAI, and sees the same conversation.
2. A stream started in POS exposes Stop in OfficeAI and cannot be duplicated.
3. Opening payment minimizes the widget and F9/F12/payment Enter behavior remains authoritative.
4. A scanner-shaped input entered in the widget routes to POS and does not create a chat message.
5. Cashier receives a stock answer but a direct mutation tool request is denied and no `ai_actions` row is prepared.

- [ ] **Step 2: Run complete frontend verification**

```text
npm run check
npm run build
```

Expected: zero exit status; chunk-size warnings may be reported but no build errors.

- [ ] **Step 3: Run complete Rust verification**

From `src-tauri/`:

```text
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --lib
```

Expected: zero exit status and no Clippy warnings.

- [ ] **Step 4: Run runtime smoke test**

Run `npm run tauri dev`. Verify with owner, manager, and cashier sessions; test minimized/open/reset states, image attachment, shared history, Stop, payment suppression, refund suppression, shift-close suppression, F2/F3, F9/F12, and one real barcode scan.

- [ ] **Step 5: Inspect scope and security evidence**

Run:

```powershell
git diff --check
git status --short
rg -n "session_token|api_key|pin_hash" src/zanai src/__tests__/zanAi*.test.ts
```

The `rg` matches must be limited to explicit negative/redaction tests or type selection; no context payload/logger may include secret fields.

- [ ] **Step 6: Commit verification-only files**

```powershell
git add -- tests/e2e-tauri/specs/zanai-pos-widget.spec.ts tests/e2e-tauri/pageobjects/pos.page.ts
git commit --only -m "test: verify shared ZanAI POS workflow" -- tests/e2e-tauri/specs/zanai-pos-widget.spec.ts tests/e2e-tauri/pageobjects/pos.page.ts
```

---

## Follow-On Plans

After Task 8 is verified, create and execute separate plans in this order:

1. `zanai-action-registry-review-queue` — registry cutover, provider dispatch parity, persisted `ai_list_actions`, authoritative Review.
2. `zanai-observability-evaluations` — latency/token/cost/outcome metrics and English/Arabic eval harness.
3. `zanai-grounding-degraded-mode` — freshness/provenance presentation, deterministic local commands, recommendation feedback.

These follow-on plans consume the shared role/session/runtime interfaces created here and must not create a second chat controller or authorization path.
