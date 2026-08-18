/**
 * AI action lifecycle — presentation model over the persisted backend states.
 *
 * Backend truth (src-tauri/src/db/repositories/ai_admin_repo.rs):
 *     prepared → executed | cancelled | expired
 *
 * These are the only persisted states. `executing` below is an explicitly
 * transient *frontend* state that exists between clicking confirm and the
 * backend answering; it is never read from or written to the database.
 *
 * UI labels are a presentation concern only — the backend state names are
 * never mutated to fit the wording.
 */
import type { AiActionSummary } from "../../../types";

export type ActionStatus = "prepared" | "executed" | "cancelled" | "expired";

/** Includes the transient client-side state. */
export type ActionUiState = ActionStatus | "executing";

export interface ActionStateMeta {
  labelKey: string;
  tone: "warn" | "ok" | "muted" | "danger" | "info";
  /** Terminal states can never be executed or cancelled again. */
  terminal: boolean;
}

export const ACTION_STATE: Record<ActionUiState, ActionStateMeta> = {
  prepared:  { labelKey: "actionPrepared",  tone: "warn",   terminal: false },
  executing: { labelKey: "actionExecuting", tone: "info",   terminal: false },
  executed:  { labelKey: "actionExecuted",  tone: "ok",     terminal: true },
  cancelled: { labelKey: "actionCancelled", tone: "muted",  terminal: true },
  expired:   { labelKey: "actionExpired",   tone: "muted",  terminal: true },
};

export function actionStatus(raw: string): ActionStatus {
  return (["prepared", "executed", "cancelled", "expired"].includes(raw)
    ? raw
    : "expired") as ActionStatus;
}

/** An action already carrying a failure, regardless of persisted status. */
export function hasFailed(action: AiActionSummary): boolean {
  return Boolean(action.error_message);
}

/**
 * Only a prepared, unexpired action may be confirmed.
 *
 * The backend enforces both conditions independently (status guard and an
 * RFC3339 expiry comparison); this mirrors them so the UI never offers a
 * button the server would reject.
 */
export function canConfirm(action: AiActionSummary, now: Date = new Date()): boolean {
  if (actionStatus(action.status) !== "prepared") return false;
  return !isExpired(action, now);
}

export function canCancel(action: AiActionSummary, now: Date = new Date()): boolean {
  return canConfirm(action, now);
}

/**
 * A prepared action whose expiry has passed is effectively expired even if the
 * sweep has not run yet — `expire_old_actions` is periodic, so the row can lag
 * reality. Treat the timestamp as authoritative for what to offer the user.
 */
export function isExpired(action: AiActionSummary, now: Date = new Date()): boolean {
  const t = Date.parse(action.expires_at);
  if (Number.isNaN(t)) return false;
  return t < now.getTime();
}

/** The state to display, accounting for a lagging expiry sweep. */
export function displayState(action: AiActionSummary, now: Date = new Date()): ActionStatus {
  const status = actionStatus(action.status);
  if (status === "prepared" && isExpired(action, now)) return "expired";
  return status;
}

/** Human-readable time remaining, or null when not applicable. */
export function expiresInMinutes(action: AiActionSummary, now: Date = new Date()): number | null {
  if (actionStatus(action.status) !== "prepared") return null;
  const t = Date.parse(action.expires_at);
  if (Number.isNaN(t)) return null;
  return Math.round((t - now.getTime()) / 60000);
}

/**
 * Turn a snake_case tool name into a readable action label, e.g.
 * `product_update` → "Product update". Used for the queue's type column;
 * the full sentence lives in `preview_text`.
 */
export function toolLabel(toolName: string): string {
  const words = toolName.replace(/[_-]+/g, " ").trim();
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** Queue filter presets, mapped to persisted statuses. */
export const REVIEW_FILTERS: { id: string; labelKey: string; statuses: ActionStatus[] }[] = [
  { id: "pending",  labelKey: "reviewPending",  statuses: ["prepared"] },
  { id: "executed", labelKey: "actionExecuted", statuses: ["executed"] },
  { id: "closed",   labelKey: "reviewClosed",   statuses: ["cancelled", "expired"] },
  { id: "all",      labelKey: "filterAll",      statuses: [] },
];
