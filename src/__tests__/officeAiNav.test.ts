import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { CONTROL_GROUPS, OPERATION_GROUPS, primarySpaceForTab, sectionsForRole, visibleTabsFor } from "../officeai/nav";
import { PRIMARY_ENTRIES } from "../officeai/OfficeAIPrimaryNav";
import OfficeAIOverview from "../officeai/OfficeAIOverview";
import { buildOfficePulseModel } from "../officeai/officeAiData";
import { buildSystemCommandCenterModel } from "../officeai/OfficeAISystemHealth";
import { officeAiTranslator } from "../i18n/officeAiStrings";
import { buildPurchasingCommandModel } from "../officeai/OfficeAIPurchasingWorkspace";
import { conflictActionsFor } from "../officeai/OfficeAIConflictInbox";
import OfficeAIShell from "../officeai/OfficeAIShell";
import type { OfficeAiOverviewSnapshot } from "../officeai/officeAiTypes";
import type { MarginSummary, PurchaseOrderRow, SupplierRow, SyncDiagnostics, SystemHealthReport } from "../types";

function flattened(role: string) {
  return sectionsForRole(role).flatMap(section => section.items.map(item => item.id));
}

describe("OfficeAI workspace navigation", () => {
  it("exposes exactly four quiet primary spaces", () => {
    expect(PRIMARY_ENTRIES.map(entry => entry.label)).toEqual(["Home", "Ask AI", "Operations", "Control"]);
    expect(primarySpaceForTab("overview")).toBe("home");
    expect(primarySpaceForTab("assistant")).toBe("ask-ai");
    expect(primarySpaceForTab("purchasing")).toBe("operations");
    expect(primarySpaceForTab("settings")).toBe("control");
  });

  it("groups operational and control tools without hiding capabilities", () => {
    expect(OPERATION_GROUPS.map(group => group.label)).toEqual(["Catalog", "Sales", "People"]);
    expect(OPERATION_GROUPS.flatMap(group => group.tabs)).toContain("purchasing");
    expect(CONTROL_GROUPS.flatMap(group => group.tabs)).toEqual(expect.arrayContaining([
      "actions", "workflows", "health", "conflicts", "devices", "settings", "audit", "insights", "loyalty",
    ]));
  });
  it("uses a simplified owner sidebar without duplicating operational tables", () => {
    const tabs = flattened("owner");
    expect(tabs).toContain("operations");
    expect(tabs).toContain("insights");
    expect(tabs).toContain("loyalty");
    expect(tabs).toContain("conflicts");
    expect(tabs).not.toContain("products");
    expect(tabs).not.toContain("reports");
    expect(tabs).not.toContain("customers");
    expect(tabs).not.toContain("purchasing");
    expect(new Set(tabs).size).toBe(tabs.length);
  });

  it("keeps manager-only OfficeAI workspaces out of cashier navigation", () => {
    const visible = visibleTabsFor("cashier");
    expect(visible).not.toContain("actions");
    expect(visible).not.toContain("workflows");
    expect(visible).not.toContain("health");
    expect(visible).not.toContain("insights");
    expect(visible).not.toContain("loyalty");
    expect(visible).not.toContain("purchasing");
    expect(visible).not.toContain("conflicts");
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

  it("shows warnings in the header and omits healthy operational noise", () => {
    const html = renderToStaticMarkup(createElement(OfficeAIShell, {
      activeTab: "overview",
      activeSpace: "home",
      title: "Overview",
      subtitle: "Live store command view",
      dockOpen: false,
      onToggleDock: () => {},
      pulseItems: [
        { id: "sync", label: "Sync offline", level: "critical", icon: createElement("span") },
      ],
      children: createElement("div", null, "Workspace"),
    }));

    expect(html).toContain("Active warnings");
    expect(html).toContain("Sync offline");
    expect(html).not.toContain("Healthy");
  });
});
