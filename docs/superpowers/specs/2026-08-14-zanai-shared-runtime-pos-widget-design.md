# ZanAI Shared Runtime and POS Widget — Design Specification

**Date:** 2026-08-14
**Status:** Approved design
**Primary delivery:** Shared ZanAI runtime plus a floating POS chat widget
**Program scope:** Reliability, safety, review, observability, grounding, and degraded-mode improvements

## 1. Objective

Make ZanAI continuously available in both the POS and Command/OfficeAI without creating two assistants or weakening checkout safety.

The first delivery introduces one shared conversation runtime and a floating POS widget. Follow-on deliveries complete the action registry, persistent Review queue, evaluations, observability, grounded answers, and deterministic degraded mode.

Success means:

- POS and Command display the same conversation and active response for one signed-in user and branch.
- Cashiers can use a read-only operational assistant from POS.
- Managers and owners retain their existing role-authorized confirmation and undo flows.
- Payment, refunds, shift closing, barcode scanning, and POS keyboard shortcuts remain authoritative.
- ZanAI failure never prevents selling, printing, or completing a shift.

## 2. Verified Current State

The repository already contains most of the reusable assistant surface:

- `src/officeai/useChatController.ts` owns conversation state, persisted history, streaming, cancellation, attachments, pending mutations, bulk runs, feedback, undo, and task resumption.
- `src/officeai/ChatPanel.tsx` is explicitly reusable across full and dock variants.
- `src/officeai/CopilotDock.tsx` is chrome around `ChatPanel`; it does not own chat state.
- `src/officeai/OfficeAIPage.tsx` currently creates the controller. Leaving OfficeAI therefore unmounts the live controller.
- `src/App.tsx` owns authenticated session, branch/view transitions, POS/OfficeAI handoffs, lock, logout, and shift state.
- `src/pages/PosPage.tsx` already exposes cart, shift, connectivity, modal, payment, barcode, and notification state needed for a POS context snapshot.
- `src/hooks/usePosShortcuts.ts` reserves POS function keys and ignores most shortcuts while a free-text input is active.
- `src/components/BarcodeInput.tsx` returns F2/F3 and window focus to the barcode input when safe.
- The backend already authenticates AI requests with opaque session tokens and has confirmation, hash, scope, expiry, audit, cancellation, and undo controls.

The current POS can hand work to OfficeAI only for manager/owner users. It cannot keep ZanAI open beside the till, and a cashier cannot access a safely reduced AI capability set.

## 3. Approved Product Decisions

1. ZanAI uses **one shared runtime** above POS and OfficeAI.
2. The POS presentation is a **floating chat window**, not a permanent dock or full side sheet.
3. Cashiers receive **read-only operational capabilities**.
4. Manager/owner mutations keep backend role checks, explicit confirmation, audit, and undo.
5. POS and Command share conversation, history, streaming state, attachments, and resumable tasks.
6. Every POS send includes a visible, removable till-context attachment by default.
7. The widget automatically minimizes for payment, refund, shift-close, and other critical dialogs.
8. The widget must not send barcode-scanner bursts as chat messages or consume POS function-key behavior.

## 4. Program Decomposition

The work is divided by dependency rather than visual surface.

### Phase 1 — Shared runtime foundation

- Introduce the shared provider and stable controller lifetime.
- Migrate OfficeAI to consume the shared controller with no behavior change.
- Prove navigation does not duplicate requests or lose state.

### Phase 2 — Floating POS widget

- Add minimized/open states, position/size persistence, unread state, and full-screen expansion.
- Add till context, scanner protection, and critical-flow suppression.
- Enable cashier read-only chat through backend-derived role policy.

### Phase 3 — Reliability core

- Finish the authoritative Action Registry cutover.
- Generate model-visible schemas and prompt capability descriptions from registry metadata.
- Route every provider through the same dispatch path.
- Add persistent action listing and make Review authoritative.

### Phase 4 — Quality and operations

- Add provider/tool latency, token, estimated-cost, cancellation, failure, and outcome metrics.
- Add English/Arabic evaluation fixtures and provider-parity tests.
- Add operator-visible provider health and failure guidance.

### Phase 5 — Grounding and degraded mode

- Attach branch, reporting period, freshness, and source/calculation metadata to business answers.
- Offer deterministic local lookup/report commands when the model provider is unavailable.
- Measure recommendation acceptance, edits, dismissals, and outcomes.

Each phase is independently releasable. A later phase may not weaken the authorization or checkout invariants established by Phases 1–2.

## 5. Phase 1 Architecture

### 5.1 Shared provider

Add `src/zanai/ZanAiProvider.tsx` and `src/zanai/useZanAi.ts`.

`ZanAiProvider` mounts only when an authenticated `SessionUser` exists. It calls `useChatController` exactly once for that user/branch and exposes the resulting `ChatController` through React context.

Provider identity is `(session_token, user_id, branch_id)`. A change to any identity field disposes the old controller and creates a new one. Lock and logout revoke the backend session and remove the provider, which clears transient AI state from memory. Persisted history remains in SQLite under existing scope rules.

The provider also owns UI-neutral shared state:

```ts
type ZanAiSurface = "pos" | "office";

interface ZanAiSurfaceContext {
  surface: ZanAiSurface;
  summary: string;
  structured?: PosAiContext;
}

interface ZanAiUiState {
  activeSurface: ZanAiSurface;
  widgetOpen: boolean;
  widgetExpanded: boolean;
  unreadCount: number;
}
```

The chat controller remains responsible for AI lifecycle state. The provider does not duplicate messages, pending actions, or stream bookkeeping.

### 5.2 Dynamic surface context

`useChatController` currently receives a `getUiContext()` closure. The provider replaces the OfficeAI-only closure with a stable function that reads a ref containing the latest registered surface context.

- `OfficeAIPage` updates the context ref when its active domain/tab changes.
- `PosPage` updates the context ref from its current POS snapshot.
- The context is read only when Send begins; in-progress requests are immutable.
- When both surfaces are mounted, the surface from which Send was invoked supplies the context. Merely rendering a surface cannot overwrite an already-started request.

`ChatController.handleSend` becomes:

```ts
handleSend(overrideText?: string, sendContext?: ZanAiSurfaceContext): Promise<void>;
```

This is an additive interface change. Existing OfficeAI callers can omit `sendContext` during migration.

### 5.3 OfficeAI migration

`OfficeAIPage` stops constructing `useChatController` and consumes `useZanAi()` instead. Its existing assistant workspace, dock, confirmations, navigation events, and mutation refresh behavior remain unchanged.

The provider receives callbacks that cannot be owned by one surface:

- Navigation events are queued as a shared request. OfficeAI consumes them when visible; POS exposes “Open in Command” when a requested destination is manager-only.
- Mutation completion increments a shared data epoch. OfficeAI tabs and POS context selectors use that epoch to refresh only when mounted.

Phase 1 acceptance requires an OfficeAI regression proving identical initial history, send, stream, Stop, confirmation, undo, and clear-chat behavior after the ownership move.

## 6. Phase 2 POS Widget

### 6.1 Components

Add:

- `src/zanai/PosZanAiWidget.tsx` — launcher and floating window.
- `src/zanai/PosZanAiContext.ts` — serializable context builder and redaction.
- `src/zanai/usePosZanAiWindow.ts` — bounded position, size, open/minimize, unread state.
- `src/zanai/useScannerBurstGuard.ts` — prevents scanner bursts from becoming chat sends.
- `src/zanai/zanAiWidget.css` — widget-only styling and responsive rules.

The widget reuses `ChatPanel` rather than forking message rendering or chat lifecycle code. `ChatPanel` gains a `variant="pos"` presentation option only where the POS needs different chrome or density.

### 6.2 Window behavior

- The minimized launcher is anchored beside the cart and remains above normal POS content but below critical dialogs.
- Opening creates a floating window with a draggable header and bounded resize handles.
- Position and size are clamped inside the current POS viewport on every restore and resize.
- Position, size, and the user's voluntary open/minimized preference are stored locally using a key scoped to device and user.
- Critical-flow minimization is temporary and does not overwrite the user's voluntary preference.
- A reset command restores the default bottom-right position if display dimensions change.
- Keyboard users can move/resize through accessible controls; drag is not the only mechanism.
- The close control minimizes rather than destroying the conversation.
- Expand opens the existing full assistant workspace for managers/owners. For cashiers it expands the read-only widget within POS rather than navigating into restricted Command areas.

### 6.3 Critical-flow suppression

`PosPage` derives a single `zanAiSuppressed` boolean from authoritative POS state.

Suppression includes:

- `activeModal.kind` of `payment`, `refund`, `shiftClose`, exchange/return completion, or any future modal marked critical.
- Fast-payment execution.
- Sale finalization and receipt recovery states that own keyboard focus.
- Lock, logout, critical-update, and session-expiry overlays.

When suppression begins, the widget minimizes synchronously and moves focus to the owning POS flow. The chat request may continue in the shared provider. Completion increments the unread badge without reopening the widget.

The widget never renders above a critical modal and cannot confirm a mutation while suppressed.

### 6.4 POS context

```ts
interface PosAiContext {
  surface: "pos";
  captured_at: string;
  branch: { id: string; name: string };
  device: { id: string };
  shift: { id: string; opened_at: string };
  cart: {
    item_count: number;
    subtotal_minor: number;
    discount_minor: number;
    tax_minor: number;
    total_minor: number;
    lines: Array<{
      product_id: string | null;
      name: string;
      barcode: string | null;
      quantity: number;
      unit_price_minor: number;
      line_total_minor: number;
    }>;
  };
  customer: { id: string; display_name: string } | null;
  connection: { online: boolean; pending_sync_count: number | null };
}
```

The visible chip summarizes the attachment. Removing it omits the structured context for that send only.

The context never contains session tokens, PINs, payment credentials, full card data, provider keys, customer secrets, or hidden UI state. Money remains integer minor units. The backend treats this context as untrusted input and uses database tools for authoritative answers.

Context is serialized deterministically and size-bounded. If the cart exceeds the line cap, the attachment includes totals plus the first bounded set of lines and an explicit truncation marker.

### 6.5 Scanner and shortcut protection

Keyboard-wedge scanners are indistinguishable from typing until their rapid burst completes. Protection therefore occurs at the POS widget input boundary:

- Track printable-key timing and the current input suffix while ZanAI's textarea owns focus.
- A rapid barcode-shaped suffix terminated by Enter is removed from the chat input, prevented from sending, and routed to the existing `handleBarcode` path.
- Human-paced Enter keeps its normal chat-send behavior.
- F2/F3 always return focus to the barcode input through existing POS handlers.
- F9–F12 and other authoritative POS function keys retain their current behavior.
- Widget pointer/keyboard handlers stop only widget-local events; they do not install a second global POS shortcut manager.

The burst classifier is a pure, configurable function with tests for scanner speed, human typing, quantity prefixes, existing draft text, Arabic/English chat text, and malformed input. False positives must preserve the draft rather than silently discarding text.

## 7. Authorization and Tool Policy

The client never selects its privilege tier. The backend derives role and branch from the opaque session token on every AI request.

### Cashier tier

Cashiers receive a small allowlist of read-only operational actions, initially:

- Product/barcode lookup and sellability.
- Stock availability and location.
- Current cart explanation and price/tax explanation.
- Customer lookup required for the current sale, subject to existing privacy scope.
- Order/delivery status lookup.
- Store procedure/help content.
- Sync/provider health summaries that do not expose secrets.

Cashiers cannot receive mutation schemas. If a mutation name is submitted manually or produced by a provider, backend dispatch rejects it before execution and writes a security-relevant denial event without sensitive input.

### Manager/owner tier

Managers and owners keep registry-declared capabilities subject to existing RBAC, confirmation, payload hash, expiry, scope, audit, cancellation, and undo rules. Rendering inside POS grants no additional authority.

Every mutation path must enforce role at dispatch/execution time, not only when tool definitions are filtered.

## 8. Shared Conversation and Concurrency

- One provider owns one active stream per `(user, branch)` frontend session.
- `handleSend` refuses a second send while state is non-idle.
- POS and OfficeAI observe the same `chatState`, request ID, tokens, tool calls, pending action, and cancellation control.
- Surface unmount does not cancel a stream; provider unmount, logout, lock, or explicit Stop does.
- Message persistence remains backend-authoritative.
- An unread count increments only when an assistant/system result arrives while the POS widget is minimized or another surface is active.
- Pending confirmations remain visible in both authorized surfaces. Cashiers never see or confirm manager-only pending actions.

## 9. Reliability-Core Follow-On

The existing large tool surface and multiple dispatch paths are consolidated incrementally:

1. Action metadata becomes the source for name, schema, kind, risk, minimum role, confirmation, audit, and undo policy.
2. Prompt capability text is generated from the same registry.
3. Anthropic, OpenAI-compatible, and non-streaming paths call one dispatch entry.
4. Raw internal tools are removed from model-visible definitions after parity is proven.
5. `ai_list_actions` exposes scoped, paginated persisted actions so Review no longer depends on in-memory chat state.

The migration is additive until parity tests pass. No big-bang replacement is permitted.

## 10. Observability, Grounding, and Degraded Mode

### Observability

Record per request/turn:

- Provider and model identifiers.
- Start/end timestamps and latency.
- Input/output token counts where provided; explicit estimates otherwise.
- Estimated cost using versioned pricing configuration.
- Tool name, duration, outcome, and safe error classification.
- Cancellation source and action outcome.

Do not persist hidden chain-of-thought. Persist user-visible rationale, tool provenance, and structured outcomes only.

### Grounding

Business-result presentation carries:

- Branch and device scope where relevant.
- Reporting period.
- Data freshness timestamp.
- Tool/query provenance and calculation summary.
- Missing-data warnings and confidence classification.

### Degraded mode

Provider failure does not disable POS or local deterministic capabilities. The widget distinguishes:

- AI provider unavailable.
- Local database unavailable.
- Offline network state.
- Cancelled or timed-out request.

Read-only deterministic commands can remain available without the model: barcode/product lookup, stock checks, current-cart totals, shift summary, and sync health. They use existing Rust commands rather than simulated AI answers.

## 11. Error Handling

- Controller errors are shared, recoverable state; they do not unmount the widget.
- Retry creates a new request ID and never reuses a possibly applied mutation request.
- Stream cancellation is idempotent from either surface.
- Provider timeout leaves partial text clearly marked and returns the controller to idle.
- Context serialization failure sends without context and tells the user; it never blocks checkout.
- Invalid persisted window geometry resets to defaults.
- History-load failure offers retry and a clearly labeled temporary conversation; it does not silently fork permanent history.
- Confirmation expiry, cancellation, execution failure, and undo failure use the persisted action state as authority.

## 12. Testing and Verification

### Frontend unit/component tests

- Provider mounts one controller and both surfaces receive the same reference/state.
- Navigation between POS and OfficeAI preserves messages, attachments, stream state, and pending actions.
- Identity change resets transient state.
- Context builder uses integer money, redacts secrets, truncates deterministically, and honors chip removal.
- Widget geometry clamps and recovers from invalid storage.
- Critical state minimizes without changing voluntary preference.
- Scanner bursts route to `handleBarcode`; human text and Enter remain chat input.
- Cashier presentation contains no confirmation controls or manager-only actions.

### Rust tests

- Session-derived role selects the correct registry subset.
- Cashier mutation calls fail even when invoked directly by name.
- Manager/owner mutations retain confirmation and scope requirements.
- Registry/prompt/schema sets cannot drift or contain duplicate names.
- Provider dispatch emits equivalent lifecycle events.
- Persisted action listing enforces branch, user role, status, pagination, and expiry.

### Integration/E2E tests

- Open widget in POS, ask a stock question, navigate to OfficeAI, and observe the same conversation.
- Start a response in one surface and Stop it in the other.
- Open payment while streaming; widget minimizes and payment keyboard behavior remains intact.
- Scan while chat input is focused; product is added and barcode text is not sent.
- Cashier asks for a price change; no mutation is prepared or executed.
- Manager prepares, confirms, audits, and undoes a mutation across surfaces.
- Provider unavailable: selling, payment, printing, and deterministic lookup remain usable.

### Required gates per delivery

```text
npm run type-check
npm run lint
npm test
npm run build

cd src-tauri
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --lib
```

Runtime smoke testing uses `npm run tauri dev`, never Vite-only development.

## 13. Acceptance Criteria

Phase 1 is accepted when:

- OfficeAI behavior remains functionally equivalent after controller ownership moves.
- One stream and conversation persist across POS/OfficeAI navigation.
- Logout, lock, or identity change safely disposes the runtime.

Phase 2 is accepted when:

- Every authenticated POS role can open the floating ZanAI widget.
- Cashiers receive only backend-enforced read actions.
- Context is visible, removable, current at send time, redacted, and bounded.
- Critical checkout flows minimize the widget and retain keyboard/focus authority.
- Scanner bursts cannot be sent as ZanAI messages.

The complete program is accepted when:

- Action definitions, prompts, permissions, and dispatch share one registry.
- Review lists persisted actions authoritatively.
- Provider/tool quality and costs are measurable.
- English/Arabic evaluation and provider-parity suites pass.
- Grounding and degraded-mode behavior are visible and tested.

## 14. Rollout and Recovery

- Guard the POS widget and cashier AI access with independent configuration flags during rollout.
- Enable shared runtime for manager/owner OfficeAI first and prove equivalence.
- Enable the POS widget for manager/owner users next.
- Enable cashier read-only access only after backend denial tests and audit events pass.
- Existing OfficeAI handoff remains available until shared-runtime E2E tests pass.
- Each phase is committed separately and can be reverted without database rollback unless its own reviewed migration explicitly requires one.
- No phase deploys, publishes, merges, or changes production configuration without separate user authority.
