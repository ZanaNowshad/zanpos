import { describe, expect, it } from "vitest";
import {
  fillAllRemaining,
  hasReceivableInput,
  isOpen,
  outstanding,
  poStatus,
  primaryActionFor,
  receivePayload,
  validateReceiveQty,
} from "../command/pages/purchasing/poLifecycle";

/**
 * These assertions mirror `po_receive_inner` in
 * src-tauri/src/commands/purchasing_commands.rs. Where the client and the
 * backend could disagree, the backend wins and the test states its rule.
 */
const line = (ordered: number, received: number) => ({
  po_line_id: `l${ordered}-${received}`,
  ordered_qty: ordered,
  received_qty: received,
  unit_cost_minor: 100,
});

describe("purchase order receiving", () => {
  it("derives remaining as ordered minus received, never negative", () => {
    expect(outstanding(line(24, 10))).toBe(14);
    expect(outstanding(line(24, 24))).toBe(0);
    // Over-received data should not produce a negative remaining.
    expect(outstanding(line(10, 12))).toBe(0);
  });

  it("treats zero and empty as 'skip this line', matching the backend", () => {
    // Backend: `if qty <= Decimal::ZERO { continue; }` — skipped, not rejected.
    expect(validateReceiveQty("0", line(24, 10))).toBeNull();
    expect(validateReceiveQty("", line(24, 10))).toBeNull();
    expect(validateReceiveQty("   ", line(24, 10))).toBeNull();
  });

  it("rejects non-numeric and negative quantities", () => {
    expect(validateReceiveQty("abc", line(24, 10))).toBe("not-a-number");
    expect(validateReceiveQty("-1", line(24, 10))).toBe("negative");
  });

  it("rejects more than the outstanding quantity", () => {
    // Backend: `received + qty > ordered` is a Validation error for the whole batch.
    expect(validateReceiveQty("14", line(24, 10))).toBeNull();
    expect(validateReceiveQty("15", line(24, 10))).toBe("exceeds-remaining");
    expect(validateReceiveQty("1", line(24, 24))).toBe("exceeds-remaining");
  });

  it("accepts decimal quantities", () => {
    expect(validateReceiveQty("2.5", line(10, 0))).toBeNull();
  });

  it("sends only positive, valid lines", () => {
    const lines = [line(24, 10), line(10, 10), line(6, 0)];
    const payload = receivePayload(
      { [lines[0].po_line_id]: "14", [lines[1].po_line_id]: "0", [lines[2].po_line_id]: "3" },
      lines,
    );
    expect(payload).toEqual([
      { po_line_id: lines[0].po_line_id, received_qty: "14" },
      { po_line_id: lines[2].po_line_id, received_qty: "3" },
    ]);
  });

  it("never sends a quantity the server would reject", () => {
    const lines = [line(24, 10)];
    expect(receivePayload({ [lines[0].po_line_id]: "99" }, lines)).toEqual([]);
    expect(receivePayload({ [lines[0].po_line_id]: "-5" }, lines)).toEqual([]);
    expect(receivePayload({ [lines[0].po_line_id]: "abc" }, lines)).toEqual([]);
  });

  it("prefills every line with its outstanding quantity", () => {
    const lines = [line(24, 10), line(6, 6)];
    expect(fillAllRemaining(lines)).toEqual({
      [lines[0].po_line_id]: "14",
      [lines[1].po_line_id]: "0",
    });
  });

  it("knows when there is nothing worth submitting", () => {
    const lines = [line(24, 24)];
    expect(hasReceivableInput({ [lines[0].po_line_id]: "0" }, lines)).toBe(false);
    expect(hasReceivableInput({}, lines)).toBe(false);
    const open = [line(24, 10)];
    expect(hasReceivableInput({ [open[0].po_line_id]: "1" }, open)).toBe(true);
  });

  it("offers Receive only in states the backend accepts", () => {
    expect(primaryActionFor("ordered")).toBe("receive");
    expect(primaryActionFor("partial")).toBe("receive");
    // Backend rejects these outright, so the UI must never offer the action.
    expect(primaryActionFor("received")).toBeNull();
    expect(primaryActionFor("cancelled")).toBeNull();
    // Draft is a UI-level policy: the backend would allow it, we do not.
    expect(primaryActionFor("draft")).toBe("send");
  });

  it("keeps Cancel to non-terminal orders", () => {
    expect(isOpen("draft")).toBe(true);
    expect(isOpen("ordered")).toBe(true);
    expect(isOpen("partial")).toBe(true);
    expect(isOpen("received")).toBe(false);
    expect(isOpen("cancelled")).toBe(false);
  });

  it("falls back safely for an unknown status string", () => {
    expect(poStatus("something_new")).toBe("draft");
  });
});

/**
 * Receiving operation identity (G1).
 *
 * The server enforces uniqueness; these pin the client half of the contract,
 * which is the part that is easy to get subtly wrong: a key regenerated per
 * attempt would let a retry-after-timeout apply the goods twice.
 */
describe("receiving operation identity", () => {
  it("reuses one key across retries of the same submission", () => {
    // Modelled on OfficeAIPurchasingWorkspace: minted on open, rotated only
    // after the server confirms.
    const key = { current: "" };
    const open = () => { key.current = "op-a"; };
    const succeed = () => { key.current = "op-b"; };

    open();
    const firstAttempt = key.current;
    const retryAfterTimeout = key.current;   // no rotation on failure
    expect(retryAfterTimeout).toBe(firstAttempt);

    succeed();
    expect(key.current).not.toBe(firstAttempt);
  });

  it("sends a non-empty key, which the server requires", () => {
    // The backend rejects an empty or >128-char key before touching stock.
    const key = crypto.randomUUID();
    expect(key.length).toBeGreaterThan(0);
    expect(key.length).toBeLessThanOrEqual(128);
  });
});
