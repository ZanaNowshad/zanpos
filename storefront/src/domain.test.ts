import { describe, expect, it } from "vitest";
import {
  buildWhatsAppOrder,
  formatMoney,
  normalizeQuantity,
  refreshCartLines,
} from "./domain";

describe("storefront domain", () => {
  it("formats currencies using their configured decimal exponent", () => {
    expect(formatMoney(1250, "BHD", 3, "en")).toContain("1.250");
    expect(formatMoney(1250, "SAR", 2, "en")).toContain("12.50");
  });

  it("accepts bounded decimal quantities without floating point drift", () => {
    expect(normalizeQuantity("1.275", 3)).toBe(1.275);
    expect(normalizeQuantity("-1", 3)).toBe(0);
    expect(normalizeQuantity("2.9999", 3)).toBe(2.999);
  });

  it("builds an exact bounded ZANPOS:v1 block and human summary", () => {
    const message = buildWhatsAppOrder({
      orderId: "550e8400-e29b-41d4-a716-446655440000",
      currency: "BHD",
      locale: "en",
      decimals: 3,
      lines: [
        { id: "p-1", name: "Dates", quantity: 1.5, priceMinor: 2200 },
        { id: "p-2", name: "Coffee", quantity: 2, priceMinor: 1750 },
      ],
    });
    expect(message).toContain("Order 550e8400-e29b-41d4-a716-446655440000");
    expect(message).toContain(
      "[ZANPOS:v1]\norder_id=550e8400-e29b-41d4-a716-446655440000\ncurrency=BHD\nitems=p-1:1.5,p-2:2\n[/ZANPOS]",
    );
    expect(message.match(/\[ZANPOS:v1\]/g)).toHaveLength(1);
  });

  it("refreshes persisted cart names and prices from the current localized catalog", () => {
    const lines = refreshCartLines(
      [{ id: "p-1", name: "Old name", quantity: 2, priceMinor: 100 }],
      [{
        id: "p-1",
        categoryId: "pantry",
        name: { en: "Coffee", ar: "قهوة" },
        priceMinor: 1750,
        available: true,
      }],
      "ar",
    );

    expect(lines).toEqual([
      { id: "p-1", name: "قهوة", quantity: 2, priceMinor: 1750 },
    ]);
  });
});
