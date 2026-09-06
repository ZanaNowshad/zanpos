import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import ProductGrid from "../components/ProductGrid";
import SyncChip from "../components/SyncChip";
import { buildJoinSyncChecklist } from "../pages/SetupWizard";
import type { PullSummary } from "../tauri/commands";
import type { SyncStatus } from "../types";
import { asSessionToken } from "../types";

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

    const html = renderToStaticMarkup(<SyncChip status={status} sessionToken={asSessionToken("tok-1")} />);

    expect(html).toContain("Sales saved locally");
    expect(html).toContain("sync resumes automatically");
  });

  const CATALOGUE = ["products", "categories", "product_prices", "product_barcodes", "tax_rules"];
  const summaryOf = (over: Partial<PullSummary>): PullSummary => ({
    ok: false, rows_pulled: 0, error: null,
    products: 20, categories: 5, product_barcodes: 20, product_prices: 20,
    users: 3, devices: 2, stock_levels: 20, suppliers: 0, settings: 8,
    pending_sync: 0, consistency_score: 100, schema_match: true, hub_truth_ok: true,
    mismatched_tables: [], catalogue_ready: true,
    matched_tables: [...CATALOGUE, "users", "devices", "stock_levels", "suppliers"],
    ...over,
  });

  /**
   * The reported incident, as a test.
   *
   * A till that had been selling arrived at this screen holding 6,637 local
   * changes — sales and payments among them — that the hub had never seen. Its
   * sales count therefore differed, so parity was never 100, so the old gate
   * (`score === 100 && pending_sync === 0`) refused to open the POS. The only
   * control on screen was Retry, which pulled and never pushed, so it could not
   * produce the condition it demanded. The terminal was stranded with the one
   * copy of that day's takings on it.
   *
   * Local work waiting to upload is what a local-first till looks like after
   * time offline. It must never hold the till shut.
   */
  it("opens the POS when only local uploads are outstanding", () => {
    const checklist = buildJoinSyncChecklist(summaryOf({
      pending_sync: 6637,
      consistency_score: 75,
      hub_truth_ok: false,
      mismatched_tables: ["sales", "sale_items", "payments", "stock_movements"],
    }));

    expect(checklist.canEnterPos).toBe(true);
    expect(checklist.summary).toContain("Ready to sell");
    expect(checklist.summary).toContain("6637");
    // Retry stays available for the cases it does help, but is no longer the
    // only way off this screen.
    expect(checklist.showRetry).toBe(true);
  });

  it("refuses the POS while the catalogue is short", () => {
    // Nothing to scan and no price to charge — this is what the gate is for.
    const checklist = buildJoinSyncChecklist(summaryOf({
      catalogue_ready: false,
      hub_truth_ok: false,
      consistency_score: 40,
      mismatched_tables: ["products", "product_prices"],
      matched_tables: ["users", "devices"],
    }));

    expect(checklist.canEnterPos).toBe(false);
    expect(checklist.showRetry).toBe(true);
    expect(checklist.summary).toContain("Catalogue incomplete");
    expect(checklist.summary).toContain("products");
  });

  it("refuses the POS when the schema does not match the hub", () => {
    const checklist = buildJoinSyncChecklist(summaryOf({
      catalogue_ready: false, schema_match: false, hub_truth_ok: false,
    }));

    expect(checklist.canEnterPos).toBe(false);
    expect(checklist.summary).toContain("Schema version differs");
  });

  /* A store with no suppliers has nothing to download. Reading zero as an
     unfinished download left a "…" that could never clear, which during the
     incident read as missing data and sent the operator looking for a fault
     that did not exist. */
  it("does not report a genuinely empty table as still downloading", () => {
    const checklist = buildJoinSyncChecklist(summaryOf({ suppliers: 0 }));
    const suppliers = checklist.items.find(item => item.label === "Suppliers");

    expect(suppliers).toMatchObject({ count: 0, status: "ok" });
  });

  it("still reports a table that has not arrived", () => {
    const checklist = buildJoinSyncChecklist(summaryOf({
      product_barcodes: 0,
      mismatched_tables: ["product_barcodes"],
      matched_tables: ["products", "categories", "product_prices", "tax_rules"],
    }));
    const barcodes = checklist.items.find(item => item.label === "Barcodes");

    expect(barcodes).toMatchObject({ count: 0, status: "pending" });
  });

  it("confirms a terminal that is fully in step", () => {
    const checklist = buildJoinSyncChecklist(summaryOf({ ok: true, rows_pulled: 120 }));

    expect(checklist.showRetry).toBe(false);
    expect(checklist.canEnterPos).toBe(true);
    expect(checklist.blockingTables).toEqual([]);
    expect(checklist.summary).toBe("This terminal matches the hub truth snapshot.");
    expect(checklist.items.at(-1)).toMatchObject({ label: "Hub Truth", count: 100, status: "ok" });
  });

  it("treats an unknown state as not ready rather than assuming", () => {
    expect(buildJoinSyncChecklist(null).canEnterPos).toBe(false);
    expect(buildJoinSyncChecklist(null).summary).toBe("Waiting for the hub snapshot.");
  });
});
