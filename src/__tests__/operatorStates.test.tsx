import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import ProductGrid from "../components/ProductGrid";
import SyncChip from "../components/SyncChip";
import { buildJoinSyncChecklist } from "../pages/SetupWizard";
import type { SyncStatus } from "../types";

describe("operator-facing POS states", () => {
  it("guides managers from an empty product grid", () => {
    const html = renderToStaticMarkup(
      <ProductGrid products={[]} onSelect={() => {}} />,
    );

    expect(html).toContain("Ready to start selling?");
    expect(html).toContain("Back Office");
    expect(html).toContain("Products");
  });

  it("reassures cashiers that offline sales are safe", () => {
    const status: SyncStatus = {
      online: false,
      hub_configured: true,
      mode: "terminal",
      hub_url: "http://127.0.0.1:8923",
      pending_events: 3,
      last_successful_sync_at: null,
      days_since_last_sync: 0,
      last_error: null,
      device_id: "POS01",
      consecutive_failure_count: 0,
    };

    const html = renderToStaticMarkup(<SyncChip status={status} userId="user-1" />);

    expect(html).toContain("Sales saved locally");
    expect(html).toContain("sync resumes automatically");
  });

  it("shows a retryable join-store checklist when initial sync is incomplete", () => {
    const checklist = buildJoinSyncChecklist({
      ok: false,
      rows_pulled: 4,
      error: "network timeout",
      products: 2,
      categories: 1,
      product_barcodes: 2,
      product_prices: 2,
      users: 1,
      devices: 0,
      stock_levels: 0,
      suppliers: 0,
      settings: 1,
      pending_sync: 3,
      consistency_score: 93,
      schema_match: true,
      hub_truth_ok: false,
      mismatched_tables: ["stock_levels"],
    });

    expect(checklist.showRetry).toBe(true);
    expect(checklist.canEnterPos).toBe(false);
    expect(checklist.blockingTables).toEqual(["stock_levels"]);
    expect(checklist.summary).toContain("stock_levels");
    expect(checklist.items.map(i => `${i.label}:${i.status}`)).toEqual([
      "Products:ok",
      "Prices:ok",
      "Barcodes:ok",
      "Users:ok",
      "Stock:pending",
      "Devices:pending",
      "Suppliers:pending",
      "Settings:ok",
      "Pending sync:pending",
      "Hub Truth:pending",
    ]);
  });

  it("allows POS entry only after a 100% Hub Truth snapshot", () => {
    const checklist = buildJoinSyncChecklist({
      ok: true,
      rows_pulled: 120,
      error: null,
      products: 20,
      categories: 5,
      product_barcodes: 20,
      product_prices: 20,
      users: 3,
      devices: 2,
      stock_levels: 20,
      suppliers: 1,
      settings: 8,
      pending_sync: 0,
      consistency_score: 100,
      schema_match: true,
      hub_truth_ok: true,
      mismatched_tables: [],
    });

    expect(checklist.showRetry).toBe(false);
    expect(checklist.canEnterPos).toBe(true);
    expect(checklist.blockingTables).toEqual([]);
    expect(checklist.summary).toBe("This terminal matches the hub truth snapshot.");
    expect(checklist.items.at(-1)).toMatchObject({ label: "Hub Truth", count: 100, status: "ok" });
  });
});
