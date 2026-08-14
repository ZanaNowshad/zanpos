import { describe, expect, it } from "vitest";
import type { Cart, CartLine, SessionUser, Shift, SyncStatus } from "../types";
import { buildPosAiContext, summarizePosAiContext } from "../zanai/posContext";

function line(overrides: Partial<CartLine> = {}): CartLine {
  return {
    cart_line_id: "line-1",
    product_id: "p1",
    product_name: "Milk",
    sku: "MILK",
    barcode: "6280001",
    quantity: "2",
    unit_price_minor: 650,
    line_discount_minor: 0,
    line_discount_reason: null,
    tax_rule_id: "vat",
    tax_rate_basis_points: 0,
    tax_inclusive: true,
    tax_amount_minor: 0,
    line_total_minor: 1300,
    note: null,
    voided: false,
    ...overrides,
  };
}

const cart: Cart = {
  cart_id: "cart-1",
  branch_id: "branch-1",
  device_id: "device-1",
  shift_id: "shift-1",
  cashier_user_id: "user-1",
  lines: [line(), line({ cart_line_id: "line-2", product_id: "p2", product_name: "Bread", quantity: "1", unit_price_minor: 500, line_total_minor: 500 })],
  bill_discount_minor: 0,
  bill_discount_reason: null,
};

const shift: Shift = {
  shift_id: "shift-1",
  branch_id: "branch-1",
  device_id: "device-1",
  cashier_user_id: "user-1",
  cashier_name: "Cashier One",
  opened_at: "2026-08-14T08:00:00Z",
  closed_at: null,
  opening_cash_minor: 10000,
  status: "open",
};

const user: SessionUser = {
  user_id: "user-1",
  branch_id: "branch-1",
  display_name: "Cashier One",
  username: "cashier",
  role_id: "cashier-role",
  role_name: "cashier",
  session_token: "must-never-appear",
  session_expires_at: "2099-01-01T00:00:00Z",
};

const sync: SyncStatus = {
  online: true,
  hub_configured: true,
  mode: "terminal",
  hub_url: "https://private.example",
  pending_events: 2,
  last_successful_sync_at: "2026-08-14T11:59:00Z",
  days_since_last_sync: 0,
  last_error: null,
  device_id: "device-1",
};

function build(overrides: Partial<Parameters<typeof buildPosAiContext>[0]> = {}) {
  return buildPosAiContext({
    cart,
    shift,
    user,
    branchName: "Main Branch",
    deviceId: "device-1",
    netTotalMinor: 1800,
    taxTotalMinor: 0,
    syncStatus: sync,
    capturedAt: "2026-08-14T12:00:00Z",
    ...overrides,
  });
}

describe("POS ZanAI context", () => {
  it("copies only bounded operational fields and integer money", () => {
    const context = build();

    expect(context.cart.total_minor).toBe(1800);
    expect(context.cart.lines[0]).toEqual({
      product_id: "p1",
      name: "Milk",
      barcode: "6280001",
      quantity: "2",
      unit_price_minor: 650,
      line_total_minor: 1300,
    });
    expect(context.connection).toEqual({ online: true, pending_sync_count: 2 });
    expect(summarizePosAiContext(context)).toBe("Till · 2 lines · BHD 1.800");
  });

  it("never spreads session, PIN, hub, or payment secrets into context", () => {
    const serialized = JSON.stringify(build());

    expect(serialized).not.toContain("session_token");
    expect(serialized).not.toContain("must-never-appear");
    expect(serialized).not.toContain("pin");
    expect(serialized).not.toContain("private.example");
  });

  it("omits voided lines and marks deterministic truncation", () => {
    const manyLines = Array.from({ length: 60 }, (_, index) => line({
      cart_line_id: `line-${index}`,
      product_id: `product-${index}`,
      product_name: `Product ${index}`,
    }));
    manyLines.splice(10, 0, line({ cart_line_id: "void", voided: true }));

    const context = build({ cart: { ...cart, lines: manyLines } });

    expect(context.cart.lines).toHaveLength(40);
    expect(context.cart.truncated_line_count).toBe(20);
    expect(context.cart.lines[0]?.product_id).toBe("product-0");
    expect(context.cart.lines[39]?.product_id).toBe("product-39");
  });
});
