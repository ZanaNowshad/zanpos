import { describe, expect, it } from "vitest";
import { parseSnapshot } from "../hooks/useCartRecovery";
import type { Cart, CartLine } from "../types";

function line(overrides: Partial<CartLine> = {}): CartLine {
  return {
    cart_line_id: "L1", product_id: "P1", product_name: "Milk", sku: null, barcode: null,
    quantity: "1", unit_price_minor: 500, line_discount_minor: 0, line_discount_reason: null,
    tax_rule_id: "T1", tax_rate_basis_points: 0, tax_inclusive: true, tax_amount_minor: 0,
    line_total_minor: 500, note: null, voided: false, ...overrides,
  };
}

const cart = (lines: CartLine[]): Cart => ({
  cart_id: "C1", branch_id: "B1", device_id: "D1", shift_id: "S-OLD",
  cashier_user_id: "U1", lines, bill_discount_minor: 0, bill_discount_reason: null,
});

const snapshot = (lines: CartLine[]) =>
  JSON.stringify({ cart: cart(lines), savedAt: "2026-07-26T00:00:00Z", lineCount: lines.length });

describe("interrupted-sale recovery", () => {
  it("offers a snapshot that still has live lines", () => {
    const parsed = parseSnapshot(snapshot([line()]));
    expect(parsed?.cart.lines).toHaveLength(1);
  });

  it("does not interrupt anyone for an empty cart", () => {
    expect(parseSnapshot(snapshot([]))).toBeNull();
  });

  it("ignores a cart whose only lines were voided", () => {
    // Voided lines are already gone as far as the cashier is concerned;
    // restoring them would resurrect items someone deliberately removed.
    expect(parseSnapshot(snapshot([line({ voided: true })]))).toBeNull();
  });

  it("discards a corrupt snapshot instead of offering it", () => {
    expect(parseSnapshot("{not json")).toBeNull();
  });

  it("treats a missing snapshot as nothing to recover", () => {
    expect(parseSnapshot(null)).toBeNull();
  });

  it("survives a snapshot missing the cart entirely", () => {
    expect(parseSnapshot(JSON.stringify({ savedAt: "x" }))).toBeNull();
  });
});
