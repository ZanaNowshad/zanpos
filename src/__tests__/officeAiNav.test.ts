import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import {
  NAVIGATION,
  breadcrumbForTab,
  domainForTab,
  domainsForRole,
  paletteEntriesForRole,
  pathForTab,
  sectionsForDomain,
  tabForPath,
  visibleTabsForRole,
} from "../navigation/config";
import OfficeAIOverview from "../officeai/OfficeAIOverview";
import { buildOfficePulseModel } from "../officeai/officeAiData";
import { buildSystemCommandCenterModel } from "../officeai/OfficeAISystemHealth";
import { officeAiTranslator } from "../i18n/officeAiStrings";
import { buildPurchasingCommandModel } from "../officeai/OfficeAIPurchasingWorkspace";
import { conflictActionsFor } from "../officeai/OfficeAIConflictInbox";
import type { OfficeAiOverviewSnapshot } from "../officeai/officeAiTypes";
import type { MarginSummary, PurchaseOrderRow, SupplierRow, SyncDiagnostics, SystemHealthReport } from "../types";

describe("ZANPOS Command navigation", () => {
  it("declares nine domains in one canonical model", () => {
    expect(NAVIGATION.map(d => d.id)).toEqual([
      // Sell is gone: shift takings and end-of-day are reporting, so they moved
      // to Insights, and deliveries are handled at the till where the person
      // handling them stands. Review and System folded into one Operations
      // domain — both are monthly work, and splitting them made "where do I
      // check sync?" a guess.
      "today", "catalogue", "purchasing",
      "customers", "team", "insights", "operations",
    ]);
  });

  // The invariant that stops duplicate ownership from returning. Reports,
  // Customers, Users and Health each used to have two independent homes.
  it("gives every destination exactly one home", () => {
    const seen = new Map<string, string>();
    for (const domain of NAVIGATION) {
      const ids = domain.sections.length ? domain.sections.map(s => s.id) : [domain.defaultTab];
      for (const id of ids) {
        expect(seen.has(id), `${id} is claimed by both ${seen.get(id)} and ${domain.id}`).toBe(false);
        seen.set(id, domain.id);
      }
    }
  });

  it("gives every route path exactly one destination", () => {
    const paths = NAVIGATION.flatMap(d => (d.sections.length ? d.sections.map(s => s.path) : [d.path]));
    expect(new Set(paths).size).toBe(paths.length);
  });

  it("round-trips tabs through their paths", () => {
    for (const domain of NAVIGATION) {
      const ids = domain.sections.length ? domain.sections.map(s => s.id) : [domain.defaultTab];
      for (const id of ids) expect(tabForPath(pathForTab(id))).toBe(id);
    }
  });

  it("resolves every destination back to its rail domain", () => {
    expect(domainForTab("products")).toBe("catalogue");
    expect(domainForTab("purchasing")).toBe("purchasing");
    expect(domainForTab("settings")).toBe("operations");
    expect(domainForTab("loyalty")).toBe("customers");
    expect(domainForTab("zanshop")).toBe("customers");
    expect(domainForTab("audit")).toBe("operations");
    // Legacy alias must keep resolving so old deep links do not break.
    expect(domainForTab("operations")).toBe("catalogue");
    // The assistant is global, not a silo — it keeps Today lit in the rail.
    expect(domainForTab("assistant")).toBe("today");
  });

  it("never renders a one-item contextual sidebar", () => {
    for (const domain of NAVIGATION) {
      const sections = sectionsForDomain(domain.id, "owner");
      expect(sections.length === 0 || sections.length >= 2).toBe(true);
    }
    expect(sectionsForDomain("today", "owner")).toEqual([]);
    expect(sectionsForDomain("purchasing", "owner")).toEqual([]);
    // Quick POS chooses the till's one-tap row; it belongs with the catalogue
    // because it is a merchandising decision about products, not a setting.
    expect(sectionsForDomain("catalogue", "owner").map(s => s.id))
      .toEqual(["products", "categories", "inventory", "quickpos"]);
  });

  it("keeps manager-only workspaces out of cashier navigation", () => {
    const visible = visibleTabsForRole("cashier");
    expect(visible).not.toContain("actions");
    expect(visible).not.toContain("workflows");
    expect(visible).not.toContain("health");
    expect(visible).not.toContain("insights");
    expect(visible).not.toContain("loyalty");
    expect(visible).not.toContain("purchasing");
    expect(visible).not.toContain("conflicts");
    expect(visible).not.toContain("audit");
    // but the everyday work is still reachable
    expect(visible).toContain("products");
    expect(visible).toContain("customers");
  });

  it("hides owner-only sections from a manager", () => {
    const ops = sectionsForDomain("operations", "manager").map(s => s.id);
    expect(ops).not.toContain("audit");
    expect(ops).not.toContain("devices");
    expect(sectionsForDomain("operations", "owner").map(s => s.id)).toContain("audit");
    expect(domainsForRole("cashier").map(d => d.id)).not.toContain("operations");
  });

  it("derives the command palette from the config rather than a hand list", () => {
    const owner = paletteEntriesForRole("owner");
    expect(owner.find(e => e.id === "products")?.domainLabelKey).toBe("catalogue");
    expect(owner.find(e => e.id === "audit")?.domainLabelKey).toBe("operations");
    expect(paletteEntriesForRole("cashier").some(e => e.id === "audit")).toBe(false);
  });

  it("builds a breadcrumb so the user can see where they are", () => {
    expect(breadcrumbForTab("products").map(c => c.labelKey)).toEqual(["catalogue", "products"]);
    // A domain whose default tab is its only view needs no second crumb.
    expect(breadcrumbForTab("purchasing").map(c => c.labelKey)).toEqual(["purchasing"]);
  });

  it("renders an exception-first home with no command tile wall", () => {
    const snapshot: OfficeAiOverviewSnapshot = {
      loading: false,
      refreshedAt: "2026-07-08T10:00:00Z",
      errors: [],
      today: null,
      lowStockCount: 2,
      outOfStockCount: 1,
      sync: null,
      whatsapp: null,
      whatsappUnread: 0,
      paymentConfirmations: [],
      provider: null,
      aiEnabled: true,
      aiConfig: null,
      featureToggles: null,
      health: null,
      alerts: [],
      benefitNumber: null,
    };

    const html = renderToStaticMarkup(createElement(OfficeAIOverview, {
      snapshot,
      currencyExp: 3,
      canUseManagerTools: true,
      onOpenTab: () => {},
      onRefresh: () => {},
      pendingActionCount: 2,
    }));

    expect(html).toContain("Store pulse");
    expect(html).toContain("Needs attention");
    expect(html).toContain("Pending approvals");
    expect(html).not.toContain("AI command shortcuts");
    expect(html).not.toContain("Open purchasing");
  });

  it("builds four signals and prioritizes critical exceptions", () => {
    const snapshot: OfficeAiOverviewSnapshot = {
      loading: false,
      refreshedAt: "2026-07-08T10:00:00Z",
      errors: [],
      today: null,
      lowStockCount: 3,
      outOfStockCount: 1,
      sync: {
        online: false,
        hub_configured: true,
        mode: "terminal",
        hub_url: "http://hub.local",
        pending_events: 4,
        last_successful_sync_at: null,
        days_since_last_sync: null,
        last_error: "offline",
        device_id: "D2",
      },
      whatsapp: null,
      whatsappUnread: 0,
      paymentConfirmations: [],
      provider: null,
      aiEnabled: true,
      aiConfig: null,
      featureToggles: null,
      health: null,
      alerts: [],
      benefitNumber: null,
    };

    const model = buildOfficePulseModel(snapshot, 2, 3, officeAiTranslator("en"));
    expect(model.signals).toHaveLength(4);
    expect(model.signals.map(signal => signal.label)).toEqual([
      "Net sales", "Transactions", "Stock exceptions", "Pending approvals",
    ]);
    expect(model.attention[0].severity).toBe("critical");
    expect(model.attention.some(item => item.destination === "actions")).toBe(true);
  });

  it("models purchasing command signals from suppliers, open POs, and margin", () => {
    const suppliers: SupplierRow[] = [
      { supplier_id: "S1", name: "A", phone: null, email: null, contact_name: null, address: null, notes: null, is_active: true, product_count: 3, open_po_count: 1, updated_at: "2026-07-08" },
      { supplier_id: "S2", name: "B", phone: null, email: null, contact_name: null, address: null, notes: null, is_active: false, product_count: 0, open_po_count: 0, updated_at: "2026-07-08" },
    ];
    const orders: PurchaseOrderRow[] = [
      { po_id: "PO1", supplier_id: "S1", supplier_name: "A", status: "ordered", expected_date: null, received_date: null, notes: null, line_count: 1, ordered_total_minor: 1000, received_total_minor: 250, updated_at: "2026-07-08" },
      { po_id: "PO2", supplier_id: "S1", supplier_name: "A", status: "received", expected_date: null, received_date: null, notes: null, line_count: 1, ordered_total_minor: 500, received_total_minor: 500, updated_at: "2026-07-08" },
    ];
    const margin: MarginSummary = {
      from_date: "2026-07-01",
      to_date: "2026-07-08",
      transaction_count: 4,
      revenue_minor: 1000,
      cogs_minor: 820,
      gross_margin_minor: 180,
      margin_basis_points: 1800,
      unknown_cost_line_count: 1,
    };

    const model = buildPurchasingCommandModel(suppliers, orders, margin);

    expect(model.activeSupplierCount).toBe(1);
    expect(model.openPoCount).toBe(1);
    expect(model.receivingValueMinor).toBe(750);
    expect(model.marginWarning).toBe(true);
  });

  it("models system command center actions for stuck sync and hub-side fixes", () => {
    const health: SystemHealthReport = {
      summary: {
        ok: false,
        db_integrity: "ok",
        migration_count: 22,
        pending_sync_rows: 4,
        stuck_sync_rows: 2,
        device_count: 3,
        hub_mode: "terminal",
        last_successful_sync_at: null,
        last_heartbeat_at: null,
        schema_version: 22,
        checked_at: "2026-07-08T10:00:00Z",
      },
      findings: [
        {
          code: "hub.sync.stuck_rows",
          severity: "warning",
          area: "Hub Sync",
          title: "2 hub rows are stuck",
          detail: "Run this fix on the hub terminal.",
          fix_action: null,
        },
        {
          code: "sync.pending_rows",
          severity: "info",
          area: "Sync",
          title: "4 rows are pending",
          detail: "Waiting for hub sync.",
          fix_action: "trigger_sync_now",
        },
      ],
      devices: [],
      tables: [{ table: "products", pending: 4, stuck: 2, max_attempts: 12 }],
    };
    const sync: SyncDiagnostics = {
      hub_configured: true,
      pending_events: 4,
      stuck_events: 2,
      last_sync_at: null,
      last_error: "timeout",
      online: false,
      consistency_score: 67,
      conflicts_open: 1,
      quarantined_rows: 0,
      tables: [{
        table: "products",
        pending: 4,
        stuck: 2,
        max_attempts: 12,
        avg_attempts: 8.5,
        last_push_success_at: null,
        last_pull_success_at: null,
        last_error_at: "2026-07-10T00:00:00Z",
        last_error: "timeout",
        last_failed_row_id: "P1",
        retry_count: 3,
        table_checksum: null,
      }],
    };

    const model = buildSystemCommandCenterModel(health, sync, officeAiTranslator("en"));

    expect(model.criticalCount).toBe(0);
    expect(model.warningCount).toBe(1);
    expect(model.stuckSyncRows).toBe(2);
    expect(model.hasLocalStuckFix).toBe(true);
    expect(model.hubSideFindings).toHaveLength(1);
    expect(model.syncStateLabel).toBe("Offline");
  });

  it("offers stock reconciliation only for inventory conflicts", () => {
    expect(conflictActionsFor("stock_drift", "stock_levels")).toEqual([
      "retry",
      "pull_hub_truth",
      "reconcile_stock",
      "dismiss",
    ]);
    expect(conflictActionsFor("duplicate_barcode", "products")).toEqual([
      "retry",
      "pull_hub_truth",
      "dismiss",
    ]);
  });

});
