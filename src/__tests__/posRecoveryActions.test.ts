import { describe, expect, it } from "vitest";
import { isPriceChangeRefusal } from "../hooks/usePosRecoveryActions";

/**
 * The banner offers a remedy chosen from the error text, so the match has to
 * track the message checkout actually produces. When those two drift the
 * cashier gets a wall with no way through it — which is the whole reported
 * problem: the refusal named a price change and offered only "Focus scan".
 */
describe("recognising a price-change refusal", () => {
  it("matches the message checkout produces", () => {
    // Kept byte-identical to sale_repo.rs's format string.
    const real = "Price changed: BHD 0.200 → BHD 0.250. Tap Update prices, "
      + "then take payment. Item: 'Rainbow Original Full Cream Evaporated Milk'.";
    expect(isPriceChangeRefusal(real)).toBe(true);
  });

  it("is not fooled by other checkout refusals", () => {
    expect(isPriceChangeRefusal(
      "Payment BHD 1.000 does not match the total BHD 1.250.",
    )).toBe(false);
    expect(isPriceChangeRefusal("Printer not responding")).toBe(false);
  });

  it("treats no error as nothing to offer", () => {
    expect(isPriceChangeRefusal(null)).toBe(false);
    expect(isPriceChangeRefusal("")).toBe(false);
  });

  it("does not depend on casing", () => {
    expect(isPriceChangeRefusal("PRICE CHANGED: BHD 1.000 → BHD 2.000.")).toBe(true);
  });
});
