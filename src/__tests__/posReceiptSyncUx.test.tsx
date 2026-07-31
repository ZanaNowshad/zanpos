import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import ReceiptActionCenter from "../components/ReceiptActionCenter";
import { summarizeSyncStats } from "../components/SyncConfidenceDrawer";
import { performReceiptPrint } from "../utils/receiptLines";
import type { SaleResult, SyncTableStats } from "../types";

const sale: SaleResult = {
  sale_id: "sale-1",
  receipt_number: "R-1001",
  branch_name: "Main",
  cashier_name: "Owner",
  sold_at: "2026-07-09T00:00:00Z",
  business_date: "2026-07-09",
  created_offline: false,
  currency: "BHD",
  discount_total_minor: 0,
  tax_total_minor: 0,
  net_total_minor: 1000,
  items: [],
  payments: [{ method: "cash", amount_minor: 1000, change_minor: 0 }],
  low_stock_alerts: [],
};

describe("POS receipt and sync confidence UX", () => {
  it("summarizes pending sale sync and failed tables", () => {
    const stats: SyncTableStats[] = [
      { table: "sales", pending: 2, failed: 1, max_attempts: 4, attempts_dist: "1:1,4:1" },
      { table: "products", pending: 3, failed: 0, max_attempts: 1, attempts_dist: "1:3" },
    ];

    expect(summarizeSyncStats(stats)).toEqual({
      pendingSales: 2,
      totalPending: 5,
      totalFailed: 1,
      failedTables: ["sales"],
    });
  });

  it("renders direct receipt actions and statuses after a sale", () => {
    const html = renderToStaticMarkup(
      <ReceiptActionCenter
        sale={sale}
        receiptStatus="printed"
        whatsappStatus="ready"
        onPrint={() => {}}
        onSendWhatsApp={() => {}}
        onNewSale={() => {}}
        onViewDetails={() => {}}
        onDismiss={() => {}}
      />,
    );

    expect(html).toContain("Sale #R-1001");
    expect(html).toContain("Print");
    expect(html).toContain("Send WhatsApp");
    expect(html).toContain("New sale");
    expect(html).toContain("View details");
    expect(html).toContain("Printed");
    expect(html).toContain("Ready to send");
  });

  it("auto-prints a completed sale only when the setting is enabled", async () => {
    const print = vi.fn().mockResolvedValue("Printed");

    await expect(performReceiptPrint({
      sale,
      settings: null,
      trigger: "auto",
      autoPrintEnabled: false,
      thermalEnabled: true,
      print,
    })).resolves.toBe("skipped");
    expect(print).not.toHaveBeenCalled();

    await expect(performReceiptPrint({
      sale,
      settings: null,
      trigger: "auto",
      autoPrintEnabled: true,
      thermalEnabled: true,
      print,
    })).resolves.toBe("printed");
    expect(print).toHaveBeenCalledTimes(1);
  });

  it("allows a first manual print even when auto-print is off", async () => {
    const print = vi.fn().mockResolvedValue("Printed");

    await expect(performReceiptPrint({
      sale,
      settings: null,
      trigger: "manual",
      autoPrintEnabled: false,
      thermalEnabled: true,
      print,
    })).resolves.toBe("printed");
    expect(print).toHaveBeenCalledWith("Main", expect.arrayContaining(["Receipt: #R-1001"]));
  });
});
