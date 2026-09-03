import { describe, expect, it } from "vitest";
import { initQtyMap, lineRefundAmount } from "../utils/refundMath";
import type { SaleItemForRefund } from "../types";

function item(overrides: Partial<SaleItemForRefund> = {}): SaleItemForRefund {
  return {
    sale_item_id: "L1",
    product_id: "P1",
    product_name_snapshot: "Cola",
    quantity: "3",
    unit_price_minor: 1000,
    line_total_minor: 3300,
    refunded_amount_minor: 0,
    ...overrides,
  } as SaleItemForRefund;
}

describe("what a returned line is worth", () => {
  it("gives back the share of what was collected, not the shelf price", () => {
    // Three units collected 3.300 including tax; one unit back is 1.100.
    expect(lineRefundAmount(item(), 1)).toBe(1100);
    expect(lineRefundAmount(item(), 3)).toBe(3300);
  });

  it("respects a discount the line was actually sold at", () => {
    // Three units at 1.000 with 0.600 off: the line collected 2.400, so one
    // unit back is 0.800. Refunding at the unit price would hand back 1.000 —
    // more than the customer paid, and the backend's ceiling would then refuse
    // the last unit of a fully returned line.
    const discounted = item({ line_total_minor: 2400 });
    expect(lineRefundAmount(discounted, 1)).toBe(800);
    expect(lineRefundAmount(discounted, 3)).toBe(2400);
  });

  it("never returns more than the line across every unit", () => {
    const line = item({ quantity: "7", line_total_minor: 1000 });
    const perUnit = [1, 2, 3, 4, 5, 6, 7].map(q => lineRefundAmount(line, q));
    expect(perUnit[perUnit.length - 1]).toBe(1000);
    expect(Math.max(...perUnit)).toBe(1000);
  });

  it("treats a line with no quantity as worth nothing rather than dividing by zero", () => {
    expect(lineRefundAmount(item({ quantity: "0" }), 1)).toBe(0);
    expect(lineRefundAmount(item({ quantity: "abc" }), 1)).toBe(0);
  });

  it("starts every line at its full quantity so a full refund is one tap", () => {
    const map = initQtyMap([item(), item({ sale_item_id: "L2", quantity: "1.5" })]);
    expect(map.get("L1")).toBe(3);
    expect(map.get("L2")).toBe(1.5);
  });
});
