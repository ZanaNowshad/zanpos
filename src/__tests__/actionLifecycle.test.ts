import { describe, expect, it } from "vitest";
import type { AiActionSummary } from "../types";
import {
  ACTION_STATE, actionStatus, canCancel, canConfirm, displayState,
  expiresInMinutes, hasFailed, isExpired, toolLabel,
} from "../command/pages/review/actionLifecycle";

/**
 * Mirrors the guards in `ai_execute_action`
 * (src-tauri/src/commands/ai_admin_commands.rs:648). Where the client could
 * disagree with the server, the server wins and the test says so.
 */
const NOW = new Date("2026-08-09T12:00:00Z");

function action(over: Partial<AiActionSummary> = {}): AiActionSummary {
  return {
    action_id: "01AAA",
    session_user_id: "U1",
    branch_id: "BRANCH1",
    tool_name: "product_update",
    preview_text: "Update price of Almarai Fresh Milk 1L to BHD 0.680",
    status: "prepared",
    prepared_at: "2026-08-09T11:50:00Z",
    confirmed_at: null,
    executed_at: null,
    expires_at: "2026-08-09T12:10:00Z",
    result_json: null,
    error_message: null,
    ...over,
  };
}

describe("AI action lifecycle", () => {
  it("models only the four persisted states plus a transient executing state", () => {
    expect(Object.keys(ACTION_STATE).sort()).toEqual(
      ["cancelled", "executed", "executing", "expired", "prepared"],
    );
    expect(ACTION_STATE.executed.terminal).toBe(true);
    expect(ACTION_STATE.cancelled.terminal).toBe(true);
    expect(ACTION_STATE.expired.terminal).toBe(true);
    expect(ACTION_STATE.prepared.terminal).toBe(false);
  });

  it("allows confirmation only for a prepared, unexpired action", () => {
    expect(canConfirm(action(), NOW)).toBe(true);
    expect(canConfirm(action({ status: "executed" }), NOW)).toBe(false);
    expect(canConfirm(action({ status: "cancelled" }), NOW)).toBe(false);
    expect(canConfirm(action({ status: "expired" }), NOW)).toBe(false);
  });

  it("refuses a prepared action whose expiry has already passed", () => {
    // expire_old_actions is periodic, so a row can still say "prepared" after
    // its expires_at. The server would reject it; so must the UI.
    const stale = action({ expires_at: "2026-08-09T11:59:00Z" });
    expect(isExpired(stale, NOW)).toBe(true);
    expect(canConfirm(stale, NOW)).toBe(false);
    expect(displayState(stale, NOW)).toBe("expired");
  });

  it("cancellation follows the same rule as confirmation", () => {
    expect(canCancel(action(), NOW)).toBe(true);
    expect(canCancel(action({ status: "executed" }), NOW)).toBe(false);
    expect(canCancel(action({ expires_at: "2026-08-09T11:00:00Z" }), NOW)).toBe(false);
  });

  it("never offers actions on terminal states", () => {
    for (const status of ["executed", "cancelled", "expired"] as const) {
      const a = action({ status });
      expect(canConfirm(a, NOW)).toBe(false);
      expect(canCancel(a, NOW)).toBe(false);
    }
  });

  it("reports remaining time only while prepared", () => {
    expect(expiresInMinutes(action(), NOW)).toBe(10);
    expect(expiresInMinutes(action({ status: "executed" }), NOW)).toBeNull();
  });

  it("surfaces a recorded failure independently of status", () => {
    expect(hasFailed(action())).toBe(false);
    expect(hasFailed(action({ error_message: "Stock level conflict" }))).toBe(true);
  });

  it("treats an unrecognised status as terminal rather than actionable", () => {
    // Fail closed: an unknown state must never render a confirm button.
    const unknown = action({ status: "something_new" });
    expect(actionStatus(unknown.status)).toBe("expired");
    expect(canConfirm(unknown, NOW)).toBe(false);
  });

  it("renders a readable action type from the tool name", () => {
    expect(toolLabel("product_update")).toBe("Product update");
    expect(toolLabel("bulk_import_products")).toBe("Bulk import products");
  });

  it("keeps an executed action displayed as executed", () => {
    expect(displayState(action({ status: "executed" }), NOW)).toBe("executed");
  });
});
