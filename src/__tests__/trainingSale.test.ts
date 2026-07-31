import { describe, expect, it } from "vitest";
import { buildTrainingSale } from "../utils/trainingSale";
import type { Cart, CartLine, PaymentInput, SessionUser } from "../types";

function line(overrides: Partial<CartLine> = {}): CartLine {
  return {
    cart_line_id: "L1",
    product_id: "P1",
    product_name: "Milk 1L",
    sku: null,
    barcode: null,
    quantity: "2",
    unit_price_minor: 500,
    line_discount_minor: 0,
    line_discount_reason: null,
    tax_rule_id: "T1",
    tax_rate_basis_points: 1000,
    tax_inclusive: true,
    tax_amount_minor: 90,
    line_total_minor: 1000,
    note: null,
    voided: false,
    ...overrides,
  };
}

const cart = (lines: CartLine[], billDiscount = 0): Cart => ({
  cart_id: "C1",
  branch_id: "B1",
  device_id: "D1",
  shift_id: "S1",
  cashier_user_id: "U1",
  lines,
  bill_discount_minor: billDiscount,
  bill_discount_reason: null,
});

const session = { user_id: "U1", display_name: "Fatima", role_name: "cashier" } as SessionUser;

const build = (c: Cart, payments: PaymentInput[]) =>
  buildTrainingSale(c, payments, session, "Amwaj Al Dair", "BHD");

describe("training sale", () => {
  it("marks the receipt so it can never be mistaken for a real one", () => {
    const sale = build(cart([line()]), [{ method: "cash", amount_minor: 1000 }]);
    expect(sale.receipt_number).toBe("TRAINING");
    expect(sale.sale_id.startsWith("training-")).toBe(true);
  });

  it("excludes voided lines from totals and items", () => {
    const sale = build(
      cart([line(), line({ cart_line_id: "L2", voided: true, line_total_minor: 4000 })]),
      [{ method: "cash", amount_minor: 1000 }],
    );
    expect(sale.items).toHaveLength(1);
    expect(sale.net_total_minor).toBe(1000);
  });

  it("applies the bill discount and never reports a negative total", () => {
    const sale = build(cart([line()], 9999), [{ method: "cash", amount_minor: 0 }]);
    expect(sale.net_total_minor).toBe(0);
  });

  it("computes cash change but leaves card change null", () => {
    const cash = build(cart([line()]), [
      { method: "cash", amount_minor: 1000, tendered_minor: 1500 },
    ]);
    expect(cash.payments[0].change_minor).toBe(500);

    const card = build(cart([line()]), [{ method: "card", amount_minor: 1000 }]);
    expect(card.payments[0].change_minor).toBeNull();
  });

  it("reports no stock alerts, since nothing moved", () => {
    const sale = build(cart([line()]), [{ method: "cash", amount_minor: 1000 }]);
    expect(sale.low_stock_alerts).toEqual([]);
    expect(sale.delivery).toBeUndefined();
  });
});
