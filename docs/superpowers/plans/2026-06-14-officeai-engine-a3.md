# OfficeAI Engine A3 — Run Orchestration + RunPanel UI

## Goal
Wire the A1 engine core into the live AI streaming loop, add a dedicated
`ai_run_execute` Tauri command for confirmed execution, extend the chat
controller with RunState, and build a RunPanel component that shows
preview → progress → done/undo.

## Architecture

```
AI calls bulk_price_adjust({selector, adjustment})
    └─ streaming.rs: parse → create run (previewing) → emit RunPreview → Done
           │
    User sees RunPanel "preview" — "Adjust 312 products +20%"
    User clicks [Run]
           │
    useChatController.handleRunExecute(runId)
    → aiRunExecute(runId, userId, channel)
    → ai_run_execute Tauri command
    → execute_price_adjust() emits RunProgress per batch → RunDone/RunFailed
    → RunPanel shows progress bar → done + Undo button
```

## Files Touched

| File | Change |
|---|---|
| `src-tauri/src/ai/engine/batch.rs` | Add `on_progress` callback param |
| `src-tauri/src/ai/tools.rs` | Add `bulk_price_adjust` tool def + MUTATION_TOOLS entry |
| `src-tauri/src/ai/streaming.rs` | Replace TODO stub with engine dispatch (RunPreview + Done) |
| `src-tauri/src/commands/ai_admin_commands.rs` | Add `ai_run_execute` + `ai_run_undo` commands |
| `src-tauri/src/lib.rs` | Register 2 new commands |
| `src/officeai/officeAiTypes.ts` | Add `RunState`, extend `ChatState` |
| `src/tauri/commands.ts` | Add `aiRunExecute`, `aiRunUndo` bindings |
| `src/officeai/useChatController.ts` | Handle run events, add runState + handlers |
| `src/officeai/RunPanel.tsx` | New component: preview/progress/done/failed |
| `src/officeai/ChatPanel.tsx` | Render RunPanel, pass new props |
| `src/officeai/CopilotDock.tsx` | Pass-through new ctrl props (no change needed — uses ctrl object) |

## Tasks

### Task 1 — Modify `execute_price_adjust` to accept a progress callback

**File:** `src-tauri/src/ai/engine/batch.rs`

Change signature:
```rust
pub async fn execute_price_adjust(
    pool: &SqlitePool,
    run_id: &str,
    selector: &Selector,
    op: &PriceOp,
    batch_size: i64,
    on_progress: impl Fn(i64, i64) + Send,  // (done, total)
) -> AppResult<i64>
```

After `tx.commit().await?;` inside the loop, add:
```rust
on_progress(total_changed, total_expected);
```

where `total_expected` needs to be fetched before the loop:
```rust
let total_expected = selector.count(pool).await.unwrap_or(0);
```

Update the two `execute_price_adjust` calls in batch tests to pass `|_, _| {}` as the callback.

### Task 2 — Add `bulk_price_adjust` tool definition

**File:** `src-tauri/src/ai/tools.rs`

After the existing `bulk_update_prices` entry (line ~881), add:
```rust
ToolDef {
    name: "bulk_price_adjust".into(),
    description: "Increase, decrease, or set prices for ALL products matching a \
        category tree, text filter, or active status filter. The selector field \
        uses category_subtree (category ID — includes all descendants), active \
        (bool), or text (name/sku/barcode LIKE). The adjustment is one of: \
        {\"mode\":\"Percent\",\"value\":20.0} for +20%, {\"mode\":\"Absolute\",\"value\":-500} \
        for -500 fils, or {\"mode\":\"Set\",\"value\":5000} to set a fixed price. \
        Always shows a preview count before executing. \
        Example: increase all Toys > Girls prices by 20% = selector:{category_subtree:\"girls\"}, \
        adjustment:{mode:\"Percent\",value:20.0}".into(),
    input_schema: json!({
        "type": "object",
        "properties": {
            "selector": {
                "type": "object",
                "description": "Product filter — at least one field required",
                "properties": {
                    "category_subtree": { "type": "string", "description": "Category ID — matches this category and all subcategories" },
                    "active": { "type": "boolean", "description": "true = active products only, false = inactive only" },
                    "text": { "type": "string", "description": "Case-insensitive filter on name, SKU, or barcode" }
                }
            },
            "adjustment": {
                "type": "object",
                "description": "Price change to apply",
                "properties": {
                    "mode": { "type": "string", "enum": ["Percent", "Absolute", "Set"] },
                    "value": { "type": "number", "description": "Percent: e.g. 20.0 for +20%. Absolute: fils delta, e.g. -500. Set: exact fils price." }
                },
                "required": ["mode", "value"]
            }
        },
        "required": ["selector", "adjustment"]
    }),
},
```

Add `"bulk_price_adjust"` to `MUTATION_TOOLS` const (line ~976).

### Task 3 — Wire Run Engine dispatch in `streaming.rs`

**File:** `src-tauri/src/ai/streaming.rs`

Replace the TODO comment block (lines 257-264) with:
```rust
// ── Run Engine dispatch ────────────────────────────────────────────────
const ENGINE_OPS: &[&str] = &["bulk_price_adjust"];
if ENGINE_OPS.contains(&tool_name.as_str()) {
    use crate::ai::engine::{selector::Selector, PriceOp, runs};
    let selector: Selector = serde_json::from_value(
        tool_input.get("selector").cloned().unwrap_or_default(),
    )
    .unwrap_or_default();
    let price_op: PriceOp = serde_json::from_value(
        tool_input.get("adjustment").cloned().unwrap_or_default(),
    )
    .unwrap_or(PriceOp::Percent(0.0));
    let count = selector.count(pool).await.unwrap_or(0);
    let selector_json = serde_json::to_string(&selector).unwrap_or_default();
    let params_json = serde_json::to_string(&price_op).unwrap_or_default();
    let run_id = runs::create_run(
        pool,
        &tool_name,
        &selector_json,
        &params_json,
        count,
        &input.user_id,
    )
    .await?;
    let description = match &price_op {
        PriceOp::Percent(p) if *p >= 0.0 => format!("Increase prices by {p}% for {count} products"),
        PriceOp::Percent(p) => format!("Decrease prices by {}% for {count} products", p.abs()),
        PriceOp::Absolute(d) if *d >= 0 => format!("Add {} fils to {count} products", d),
        PriceOp::Absolute(d) => format!("Subtract {} fils from {count} products", d.abs()),
        PriceOp::Set(v) => format!("Set price to {} fils for {count} products", v),
    };
    let _ = on_event.send(StreamEvent::RunPreview {
        run_id,
        op_id: tool_name,
        description,
        count,
        samples: vec![],
    });
    let _ = on_event.send(StreamEvent::Done);
    return Ok(accumulated_text);
}
```

### Task 4 — Add `ai_run_execute` + `ai_run_undo` Tauri commands

**File:** `src-tauri/src/commands/ai_admin_commands.rs`

Add after the `ai_undo_action` function:

```rust
/// Execute a previewed bulk run. Emits RunProgress per batch, then RunDone/RunFailed.
/// The run_id was generated during the streaming preview phase.
#[tauri::command]
pub async fn ai_run_execute(
    state: tauri::State<'_, AppState>,
    run_id: String,
    _user_id: String,
    on_event: tauri::ipc::Channel<crate::domain::ai_admin::StreamEvent>,
) -> AppResult<()> {
    use crate::ai::engine::{batch, runs, selector::Selector, PriceOp};
    let pool = &state.pool;
    let run = runs::get_run(pool, &run_id).await?;
    let selector: Selector = serde_json::from_str(&run.selector_json)
        .unwrap_or_default();
    let price_op: PriceOp = serde_json::from_str(&run.params_json)
        .unwrap_or(PriceOp::Percent(0.0));
    let run_id2 = run_id.clone();
    let result = batch::execute_price_adjust(
        pool,
        &run_id,
        &selector,
        &price_op,
        100,
        move |done, total| {
            let _ = on_event.send(crate::domain::ai_admin::StreamEvent::RunProgress {
                run_id: run_id2.clone(),
                done,
                total,
            });
        },
    )
    .await;
    match result {
        Ok(_changed) => {
            let _ = on_event.send(crate::domain::ai_admin::StreamEvent::RunDone {
                run_id: run_id.clone(),
            });
        }
        Err(e) => {
            let _ = runs::set_failed(pool, &run_id, &e.to_string()).await;
            let _ = on_event.send(crate::domain::ai_admin::StreamEvent::RunFailed {
                run_id: run_id.clone(),
                error: e.to_string(),
            });
        }
    }
    Ok(())
}

/// Undo a completed bulk run by replaying undo log in reverse.
#[tauri::command]
pub async fn ai_run_undo(
    state: tauri::State<'_, AppState>,
    run_id: String,
    _user_id: String,
) -> AppResult<serde_json::Value> {
    use crate::ai::engine::batch;
    let pool = &state.pool;
    let restored = batch::undo_run(pool, &run_id).await?;
    Ok(serde_json::json!({ "followup": format!("Done — restored prices for {} products.", restored) }))
}
```

### Task 5 — Register commands in `lib.rs`

**File:** `src-tauri/src/lib.rs`

Find the invoke_handler list. After `ai_undo_action`, add:
```rust
commands::ai_admin_commands::ai_run_execute,
commands::ai_admin_commands::ai_run_undo,
```

### Task 6 — Extend types: `RunState` + `ChatState`

**File:** `src/officeai/officeAiTypes.ts`

Extend `ChatState`:
```typescript
export type ChatState = "idle" | "thinking" | "confirm" | "run_confirm" | "run_executing";
```

Add after `KpiSnapshot`:
```typescript
export interface RunState {
  runId: string;
  opId: string;
  description: string;
  count: number;
  done: number;
  phase: "preview" | "executing" | "done" | "failed";
  error?: string;
}
```

### Task 7 — Add TS command bindings

**File:** `src/tauri/commands.ts`

After `aiChatStream`:
```typescript
export const aiRunExecute = (
  runId: string,
  userId: string,
  onEvent: Channel<StreamEvent>
): Promise<void> => invoke("ai_run_execute", { runId, userId, onEvent });

export const aiRunUndo = (
  runId: string,
  userId: string,
): Promise<{ followup: string }> => invoke("ai_run_undo", { runId, userId });
```

### Task 8 — Handle run events + expose handlers in `useChatController.ts`

**File:** `src/officeai/useChatController.ts`

1. Import `RunState` from types, import `aiRunExecute`, `aiRunUndo` from commands.
2. Add `runState` state: `const [runState, setRunState] = useState<RunState | null>(null);`
3. In the channel `onmessage` handler, add 4 new branches after the `navigate` branch:

```typescript
} else if (event.type === "run_preview") {
  setRunState({
    runId: event.run_id, opId: event.op_id,
    description: event.description, count: event.count,
    done: 0, phase: "preview",
  });
  setChatState("run_confirm");
} else if (event.type === "run_progress") {
  setRunState(prev => prev ? { ...prev, done: event.done, phase: "executing" } : prev);
} else if (event.type === "run_done") {
  setRunState(prev => prev ? { ...prev, phase: "done" } : prev);
  setChatState("idle");
  setTimeout(() => { fetchKpi(); onMutationApplied(); }, 500);
} else if (event.type === "run_failed") {
  setRunState(prev => prev ? { ...prev, phase: "failed", error: event.error } : prev);
  setChatState("idle");
}
```

4. Also in the `done` branch (line ~232) guard: `setChatState(prev => (prev === "confirm" || prev === "run_confirm") ? prev : "idle");`

5. Add handlers at bottom:

```typescript
const handleRunExecute = useCallback(async () => {
  if (!runState || runState.phase !== "preview") return;
  setRunState(prev => prev ? { ...prev, phase: "executing" } : prev);
  setChatState("run_executing");
  const onEvent = new Channel<StreamEvent>();
  onEvent.onmessage = (event: StreamEvent) => {
    if (event.type === "run_progress") {
      setRunState(prev => prev ? { ...prev, done: event.done, phase: "executing" } : prev);
    } else if (event.type === "run_done") {
      setRunState(prev => prev ? { ...prev, phase: "done" } : prev);
      setChatState("idle");
      setTimeout(() => { fetchKpi(); onMutationApplied(); }, 500);
    } else if (event.type === "run_failed") {
      setRunState(prev => prev ? { ...prev, phase: "failed", error: event.error } : prev);
      setChatState("idle");
    }
  };
  try {
    await aiRunExecute(runState.runId, sessionUser.user_id, onEvent);
  } catch (e) {
    setRunState(prev => prev ? { ...prev, phase: "failed", error: String(e) } : prev);
    setChatState("idle");
  }
}, [runState, sessionUser.user_id, fetchKpi, onMutationApplied]);

const handleRunCancel = useCallback(() => {
  setRunState(null);
  setChatState("idle");
}, []);

const handleRunUndo = useCallback(async () => {
  if (!runState || runState.phase !== "done") return;
  try {
    const result = await aiRunUndo(runState.runId, sessionUser.user_id);
    setRunState(null);
    addMessage({ role: "system", text: result.followup });
    setTimeout(() => { fetchKpi(); onMutationApplied(); }, 500);
  } catch (e) {
    addMessage({ role: "system", text: `Run undo failed: ${String(e)}` });
  }
}, [runState, sessionUser.user_id, addMessage, fetchKpi, onMutationApplied]);
```

6. Extend `ChatController` interface:
```typescript
runState: RunState | null;
handleRunExecute: () => Promise<void>;
handleRunCancel: () => void;
handleRunUndo: () => Promise<void>;
```

7. Add to return object: `runState, handleRunExecute, handleRunCancel, handleRunUndo`

### Task 9 — Create `RunPanel.tsx`

**File:** `src/officeai/RunPanel.tsx` (new, ~130 lines)

```tsx
import { Zap, X, RotateCcw, CheckCircle2, XCircle } from "lucide-react";
import type { RunState } from "./officeAiTypes";

interface Props {
  runState: RunState;
  onExecute: () => void;
  onCancel: () => void;
  onUndo: () => void;
}

export default function RunPanel({ runState, onExecute, onCancel, onUndo }: Props) {
  const pct = runState.count > 0 ? Math.round((runState.done / runState.count) * 100) : 0;

  return (
    <div className={`run-panel run-panel--${runState.phase}`}>
      <div className="run-panel-icon">
        {runState.phase === "done" && <CheckCircle2 size={20} className="run-icon-done" />}
        {runState.phase === "failed" && <XCircle size={20} className="run-icon-failed" />}
        {(runState.phase === "preview" || runState.phase === "executing") && <Zap size={20} className="run-icon-active" />}
      </div>

      <div className="run-panel-body">
        <div className="run-panel-desc">{runState.description}</div>

        {runState.phase === "preview" && (
          <div className="run-panel-count">{runState.count.toLocaleString()} products will be updated</div>
        )}

        {runState.phase === "executing" && (
          <>
            <div className="run-progress-bar">
              <div className="run-progress-fill" style={{ width: `${pct}%` }} />
            </div>
            <div className="run-panel-count">{runState.done.toLocaleString()} / {runState.count.toLocaleString()} done</div>
          </>
        )}

        {runState.phase === "done" && (
          <div className="run-panel-count">All {runState.count.toLocaleString()} products updated</div>
        )}

        {runState.phase === "failed" && (
          <div className="run-panel-error">{runState.error}</div>
        )}
      </div>

      <div className="run-panel-actions">
        {runState.phase === "preview" && (
          <>
            <button className="run-btn run-btn--primary" onClick={onExecute}>
              <Zap size={14} /> Run {runState.count.toLocaleString()}
            </button>
            <button className="run-btn run-btn--ghost" onClick={onCancel}>
              <X size={14} /> Cancel
            </button>
          </>
        )}
        {runState.phase === "done" && (
          <button className="run-btn run-btn--ghost" onClick={onUndo}>
            <RotateCcw size={14} /> Undo
          </button>
        )}
      </div>
    </div>
  );
}
```

### Task 10 — Add RunPanel CSS to `App.css`

Add a `/* ── RunPanel (run-*) ── */` block:
```css
.run-panel {
  display: flex; align-items: flex-start; gap: 10px;
  padding: 12px 14px; border-radius: 10px; margin: 8px 0;
  border: 1px solid var(--border); background: var(--surface);
  transition: border-color var(--t-base);
}
.run-panel--preview  { border-color: var(--accent); }
.run-panel--executing { border-color: var(--warning); }
.run-panel--done     { border-color: var(--success); }
.run-panel--failed   { border-color: var(--error); }

.run-panel-icon { padding-top: 2px; flex-shrink: 0; }
.run-icon-active { color: var(--accent); }
.run-icon-done   { color: var(--success); }
.run-icon-failed { color: var(--error); }

.run-panel-body { flex: 1; min-width: 0; }
.run-panel-desc  { font-size: 13px; font-weight: 600; color: var(--text); }
.run-panel-count { font-size: 12px; color: var(--text-dim); margin-top: 3px; }
.run-panel-error { font-size: 12px; color: var(--error); margin-top: 3px; }

.run-progress-bar {
  height: 4px; background: var(--border); border-radius: 2px; margin-top: 6px; overflow: hidden;
}
.run-progress-fill {
  height: 100%; background: var(--warning); border-radius: 2px;
  transition: width 0.3s ease;
}
.run-panel--done .run-progress-fill { background: var(--success); width: 100% !important; }

.run-panel-actions { display: flex; gap: 6px; flex-shrink: 0; align-items: center; }
.run-btn {
  display: flex; align-items: center; gap: 4px;
  padding: 6px 12px; border-radius: 6px; font-size: 12px; font-weight: 500;
  border: 1px solid var(--border); cursor: pointer; transition: all var(--t-fast);
  white-space: nowrap;
}
.run-btn--primary {
  background: var(--accent); color: #fff; border-color: var(--accent);
}
.run-btn--primary:hover { opacity: 0.85; }
.run-btn--ghost { background: transparent; color: var(--text-dim); }
.run-btn--ghost:hover { background: var(--surface-hover); color: var(--text); }
```

### Task 11 — Wire RunPanel into `ChatPanel.tsx`

**File:** `src/officeai/ChatPanel.tsx`

1. Import `RunPanel` from `"./RunPanel"` and `RunState` from `"./officeAiTypes"`.
2. Add to Props interface:
```typescript
runState: RunState | null;
onRunExecute: () => void;
onRunCancel: () => void;
onRunUndo: () => void;
```
3. Below the `ConfirmActionModal` render (or below the `LiveActivityBar`), add:
```tsx
{runState && (
  <RunPanel
    runState={runState}
    onExecute={onRunExecute}
    onCancel={onRunCancel}
    onUndo={onRunUndo}
  />
)}
```
4. Update all ChatPanel usages (in OfficeAIPage.tsx and CopilotDock.tsx) to pass ctrl-derived props:
   - `runState={ctrl.runState}`
   - `onRunExecute={ctrl.handleRunExecute}`
   - `onRunCancel={ctrl.handleRunCancel}`
   - `onRunUndo={ctrl.handleRunUndo}`

### Task 12 — Verify

```bash
cd src-tauri && cargo check 2>&1 | grep -E "^error"
cd .. && npm run type-check 2>&1
cd src-tauri && cargo test --lib -- ai::engine 2>&1
```

All three must pass clean. Then commit:
```
feat(engine): A3 — RunPanel, run orchestration, bulk_price_adjust wired end-to-end
```
