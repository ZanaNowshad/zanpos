import { describe, expect, it } from "vitest";
import { buildReceiptLines } from "../utils/receiptLines";
import { DEFAULT_DESIGN } from "../components/ReceiptDesignEditor";
import type { SaleResult, BranchSettings } from "../types";

const sale = {
  sale_id: "s1",
  receipt_number: "MAIN-POS01-00000001",
  net_total_minor: 1100,
  tax_total_minor: 100,
  discount_total_minor: 0,
  currency: "BHD",
  payments: [{ method: "cash", amount_minor: 1100, change_minor: 0 }],
  items: [
    {
      product_name: "Cola 330ml",
      quantity: "1",
      unit_price_minor: 1000,
      line_total_minor: 1100,
      tax_amount_minor: 100,
    },
  ],
  cashier_name: "A Cashier",
  branch_name: "Main",
  sold_at: "2026-09-01T09:00:00Z",
  business_date: "2026-09-01",
  created_offline: false,
  low_stock_alerts: [],
  delivery: null,
} as unknown as SaleResult;

const settings = {
  name: "Amwaj AlDair",
  address: "Budaiya Highway",
  phone: "+973 1234 5678",
  tax_number: "TRN-1",
  cr_number: "CR-1",
  receipt_header: "Welcome",
  receipt_footer: "Thank you",
} as unknown as BranchSettings;

describe("what the receipt actually prints", () => {
  it("prints the shop's details by default", () => {
    const text = buildReceiptLines(sale, settings, false, DEFAULT_DESIGN).join("\n");
    expect(text).toContain("Amwaj AlDair");
    expect(text).toContain("Budaiya Highway");
    expect(text).toContain("+973 1234 5678");
    expect(text).toContain("TRN-1");
  });

  it("leaves out what the owner turned off", () => {
    // These toggles were saved to localStorage and never read: the editor's
    // preview honoured them and the printer ignored every one, so unchecking
    // "Show Phone" appeared to work and changed nothing on the paper.
    const text = buildReceiptLines(sale, settings, false, {
      ...DEFAULT_DESIGN,
      show_phone: false,
      show_address: false,
      show_tax_number: false,
    }).join("\n");

    expect(text).toContain("Amwaj AlDair");
    expect(text).not.toContain("+973 1234 5678");
    expect(text).not.toContain("Budaiya Highway");
    expect(text).not.toContain("TRN-1");
    expect(text).not.toContain("CR-1");
  });

  it("still prints the sale itself whatever is hidden", () => {
    const text = buildReceiptLines(sale, settings, false, {
      ...DEFAULT_DESIGN,
      show_store_name: false,
      show_address: false,
      show_phone: false,
      show_tax_number: false,
      show_header: false,
      show_footer: false,
    }).join("\n");

    // The header is decoration; the transaction is the receipt.
    expect(text).toContain("MAIN-POS01-00000001");
    expect(text).toContain("Cola 330ml");
  });
});
