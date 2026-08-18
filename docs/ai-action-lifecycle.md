# AI action lifecycle — traced from source

Authoritative as of this trace. Every claim cites the file that proves it.
Written because the Review redesign depends on knowing exactly which states are
real, and one required capability turns out not to exist yet.

## 1. Persisted state machine

Table `ai_actions` (`src-tauri/migrations/0001_initial.sql:599`).

Columns that carry state: `status`, `prepared_at`, `confirmed_at`,
`executed_at`, `expires_at`, `result_json`, `error_message`,
`tool_input_json`, `tool_input_hash`, `confirmation_token`.

| Status | Set by | Terminal | Executable | Cancellable |
|---|---|---|:--:|:--:|
| `prepared` | `create_action` (default) | no | yes | yes |
| `executed` | `mark_executed` | yes | no | no |
| `cancelled` | `mark_cancelled` | yes | no | no |
| `expired` | `expire_old_actions` | yes | no | no |

Transitions are all guarded in SQL, not just in application code
(`src-tauri/src/db/repositories/ai_admin_repo.rs`):

```sql
-- 105: cannot execute anything that is not still prepared
UPDATE ai_actions SET status='executed', executed_at=?, result_json=?
 WHERE action_id=? AND status='prepared'
-- 124
UPDATE ai_actions SET status='cancelled' WHERE action_id=? AND status='prepared'
-- 140
UPDATE ai_actions SET status='expired' WHERE status='prepared' AND expires_at < ?
```

Undo lives in `undo_records`, guarded the same way (`ai_admin_repo.rs:218`):

```sql
UPDATE undo_records SET status='undone', undone_at=?, undone_by_user_id=?
 WHERE undo_id=? AND status='available'
```

## 2. Safety invariants — verified

All in `ai_execute_action` (`src-tauri/src/commands/ai_admin_commands.rs:648`).

| # | Invariant | Proof |
|---|---|---|
| 1 | Confirmation references a persisted identity | `get_action(&state.db, &input.action_id)`; client sends only `action_id` |
| 2 | Backend does **not** execute client-supplied mutation data | `tool_name` and `tool_input` are read from the row, never from the request |
| 2b | Payload integrity is verified | `hash_str(&action.tool_input_json) != action.tool_input_hash` → `Conflict` |
| 3 | Expired actions cannot execute | `expires_at` parsed as RFC3339 `DateTime` and compared to now → `Conflict` |
| 4 | Cancelled cannot execute | `action.status != "prepared"` → `Conflict` |
| 5 | Executed cannot execute twice | same status guard, plus the SQL `WHERE status='prepared'` |
| 6 | Actor cannot be spoofed | `authorize_office(session_token)` then `input.user_id.clone_from(&actor.user_id)` — the request's own actor fields are overwritten |
| 6b | Scope is enforced | `require_actor_scope(&actor, &action.session_user_id, &action.branch_id, "Action")` |
| 10 | Undo cannot repeat | `record.status != "available"` → rejected (`ai_admin_commands.rs:877`) |
| 11 | Expiry is server-side | not a UI concern; see 3 |

Note the deliberate fix already in the source at line 669: expiry had been
compared as strings, which breaks across `+00:00` vs `Z`. It now parses first.

**No safety defect was found.** This path is stronger than the redesign brief
assumed — the strongest available invariant (persisted identity + hash-verified
payload + server-side scope and expiry) is already enforced at the transport
layer.

## 3. The gap that blocks the Review queue

`ai_actions` is **write-and-fetch-by-id only**. The repository exposes:

```
create_action  get_action(action_id)  mark_executed  mark_cancelled  expire_old_actions
```

There is **no list/query function** and **no Tauri command** that returns
pending or historical actions. Consequently the current Review surface builds
its queue from in-session chat state (`ctrl.pendingAction`,
`ctrl.pendingBatchActions` — `src/officeai/officeAiData.ts:271`), not from the
database.

That means a Review queue built today would show only actions proposed in the
**current chat session**. A manager opening Review would see an empty queue even
with genuinely pending actions from another session or device, while the UI
implied it was authoritative. That is a worse failure than having no queue.

### Required to unblock

Backend, roughly:

```rust
// src-tauri/src/db/repositories/ai_admin_repo.rs
pub async fn list_actions(
    pool: &SqlitePool,
    branch_id: &str,
    statuses: &[&str],
    limit: i64,
    offset: i64,
) -> AppResult<Vec<AiAction>>;

// src-tauri/src/commands/ai_admin_commands.rs
#[tauri::command]
pub async fn ai_list_actions(
    state: State<'_, AppState>,
    session_token: String,
    statuses: Vec<String>,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<AiActionRow>>;   // must apply require_actor_scope
```

Frontend then binds the existing `DataTable` + `EmptyState` primitives to it.

## 4. What *is* readable today

| Surface | Backend | Status |
|---|---|---|
| Pending AI actions | — | **Not readable.** Blocked as above |
| Audit / history | `audit_log_list` (`src/tauri/commands.ts:846`) | Readable: `audit_log_id, event_type, entity_type, entity_id, actor_user_id, created_at` |
| Conflicts | `OfficeAIConflictInbox` | Present; semantics not re-verified in this trace |
| Single action preview | `ToolPreview` via chat state | In-session only |
| Undo | `undo_records` + guard | Per-action, in-session reference |

## 5. Consequence for the redesign

The Review domain cannot become the authoritative control plane for AI
proposals until `ai_list_actions` exists. Everything else in the brief —
queue UI, item-type distinction, detail surface, confirmation boundary,
execution states — is frontend work that becomes straightforward once the
list command lands, because the state machine and its guarantees are already
correct and complete.
