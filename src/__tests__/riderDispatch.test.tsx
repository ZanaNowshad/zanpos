import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { buildRiderMessage } from "../utils/postSaleWhatsApp";
import DeliveryForm from "../components/DeliveryForm";
import PaymentModal from "../components/PaymentModal";

describe("rider job sheet", () => {
  const base = {
    receiptNumber: "R-000241",
    customerName: "Fatima Al Sayed",
    contactNumber: "+97333050666",
    houseNumber: "12",
    area: "4B",
    addressText: "Road 2408, Block 324",
    amountLabel: "BHD 8.750",
    storeName: "Amwaj AlDair",
  };

  it("gives the rider where to go, who to call and what to collect", () => {
    const message = buildRiderMessage({ ...base, paymentMethod: "Cash", paid: false });

    expect(message).toContain("R-000241");
    expect(message).toContain("+97333050666");
    expect(message).toContain("House 12, Flat 4B, Road 2408, Block 324");
    expect(message).toContain("Collect: BHD 8.750 (Cash)");
    expect(message).toContain("Amwaj AlDair");
  });

  it("does not tell a rider to collect on an already-paid order", () => {
    // Handing a rider a figure to collect on a paid card sale is how a customer
    // gets charged twice.
    const message = buildRiderMessage({ ...base, paymentMethod: "Card", paid: true });

    expect(message).not.toContain("Collect:");
    expect(message).toContain("already paid");
  });

  it("omits address parts the cashier left blank rather than printing gaps", () => {
    const message = buildRiderMessage({
      ...base, area: null, addressText: null, paymentMethod: "Cash", paid: false,
    });

    expect(message).toContain("Address: House 12");
    expect(message).not.toContain("Flat");
    expect(message).not.toContain(", ,");
  });
});

describe("delivery address requirements", () => {
  it("marks only the house number as required", () => {
    // Flat and road were both mandatory, which blocked the sale for a villa
    // with no flat and for an address given as a landmark.
    const html = renderToStaticMarkup(
      <DeliveryForm value={{}} onChange={() => {}} expectedPaymentMethod="cash" />,
    );

    const required = html.match(/delivery-required/g) ?? [];
    expect(required).toHaveLength(1);
    expect(html).toContain("House number required");
    expect(html).toContain("(optional)");
  });

  it("lets a delivery sale complete on the house number alone", () => {
    const html = renderToStaticMarkup(
      <PaymentModal
        netTotal={8_750}
        journey="delivery"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    // The blocking reason names the house number and nothing else.
    expect(html).not.toContain("Flat is required");
    expect(html).not.toContain("Road is required");
  });
});
