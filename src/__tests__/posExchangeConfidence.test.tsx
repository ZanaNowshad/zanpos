import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import CheckoutConfidenceStrip from "../components/CheckoutConfidenceStrip";
import { buildExchangePayments, getExchangeBalance } from "../utils/posExchange";
import { buildCheckoutConfidenceItems } from "../utils/posConfidence";

describe("POS exchange and checkout confidence", () => {
  it("calculates amount due when replacement items exceed exchange credit", () => {
    expect(getExchangeBalance(1_500, 2_100)).toEqual({
      creditMinor: 1_500,
      replacementMinor: 2_100,
      appliedCreditMinor: 1_500,
      amountDueMinor: 600,
      refundDueMinor: 0,
    });
  });

  it("calculates refund due when exchange credit exceeds replacement items", () => {
    expect(getExchangeBalance(2_500, 1_000)).toMatchObject({
      appliedCreditMinor: 1_000,
      amountDueMinor: 0,
      refundDueMinor: 1_500,
    });
  });

  it("records exchange credit as payment instead of discounting replacement sales", () => {
    expect(buildExchangePayments(
      { refundId: "REF-1", creditMinor: 1_500 },
      [{ method: "cash", amount_minor: 600, tendered_minor: 600 }],
      2_100,
    )).toEqual([
      { method: "exchange_credit", amount_minor: 1_500, external_reference: "REF-1" },
      { method: "cash", amount_minor: 600, tendered_minor: 600 },
    ]);
  });

  it("renders compact checkout confidence statuses", () => {
    const items = buildCheckoutConfidenceItems({
      stockChecked: true,
      saleSaved: true,
      syncOnline: false,
      pendingSync: 2,
      receiptStatus: "printed",
      benefitStatus: "pending",
    });
    const html = renderToStaticMarkup(<CheckoutConfidenceStrip items={items} />);

    expect(html).toContain("Stock checked");
    expect(html).toContain("Sale saved locally");
    expect(html).toContain("Sync pending");
    expect(html).toContain("Receipt printed");
    expect(html).toContain("BenefitPay pending");
  });
});
