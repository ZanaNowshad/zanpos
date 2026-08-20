import { describe, expect, it } from "vitest";
import { buildOfficePulseModel } from "../officeai/officeAiData";
import { officeAiTranslator } from "../i18n/officeAiStrings";
import type { OfficeAiOverviewSnapshot } from "../officeai/officeAiTypes";
import type { ProactiveAlert } from "../types";

/**
 * The proactive detector's findings have to reach Today.
 *
 * `proactive.rs` has been running a thirteen-detector loop every five minutes
 * since the app started — margin erosion, dead stock, refund spikes, cash
 * discrepancies — and its only consumer was a sidebar inside the ZanAI chat.
 * The landing page's attention list was built from plumbing: sync lag,
 * WhatsApp login, stock counts. So the subsystem that knew about money was the
 * one nobody saw.
 *
 * Every detector type is asserted individually rather than a sample, because
 * the failure mode is one unrouted type rendering a row that goes nowhere.
 */

const t = officeAiTranslator("en");

const alert = (alert_type: string, severity = "warning"): ProactiveAlert => ({
  alert_id: `alr_${alert_type}`,
  branch_id: "br_test",
  alert_type,
  severity,
  title: `${alert_type} title`,
  description: `${alert_type} description`,
  detail_json: null,
  detected_at: "2026-08-20T09:00:00Z",
  dismissed_at: null,
  dismissed_by_user_id: null,
  created_at: "2026-08-20T09:00:00Z",
});

const snapshot = (alerts: ProactiveAlert[]): OfficeAiOverviewSnapshot => ({
  loading: false,
  refreshedAt: "2026-08-20T10:00:00Z",
  errors: [],
  today: null,
  lowStockCount: 0,
  outOfStockCount: 0,
  sync: null,
  whatsapp: null,
  whatsappUnread: 0,
  paymentConfirmations: [],
  provider: null,
  aiEnabled: true,
  aiConfig: null,
  featureToggles: null,
  health: null,
  alerts,
  benefitNumber: null,
});

const model = (alerts: ProactiveAlert[]) => buildOfficePulseModel(snapshot(alerts), 0, 3, t);

/** Every type proactive.rs emits, and where each one has to send the operator. */
const ROUTES: [string, string][] = [
  ["margin_erosion", "products"],
  ["negative_margin", "products"],
  ["high_discounts", "reports"],
  ["refund_spike", "reports"],
  ["sales_drop", "reports"],
  ["dead_stock", "inventory"],
  ["overstock", "inventory"],
  ["near_expiry", "inventory"],
  ["low_stock", "inventory"],
  ["stock_out", "inventory"],
  ["cash_discrepancy", "cashier"],
  ["shift_too_long", "cashier"],
  ["sync_stuck", "health"],
];

describe("proactive alerts reach the Today attention list", () => {
  it("surfaces an alert the detector raised", () => {
    const rows = model([alert("margin_erosion", "critical")]).attention;
    const row = rows.find(r => r.id === "alr_margin_erosion" || r.id.endsWith("alr_margin_erosion"));
    expect(row, "the alert must produce an attention row").toBeDefined();
    expect(row!.title).toBe("margin_erosion title");
    expect(row!.detail).toBe("margin_erosion description");
  });

  it("sends every detector type somewhere the operator can act", () => {
    for (const [type, destination] of ROUTES) {
      const rows = model([alert(type)]).attention;
      const row = rows.find(r => r.id.includes(type));
      expect(row, `${type} must produce a row`).toBeDefined();
      expect(row!.destination, `${type} destination`).toBe(destination);
      // A row whose button has no words is a row that cannot be pressed.
      expect(row!.actionLabel.trim().length, `${type} action label`).toBeGreaterThan(0);
    }
  });

  it("never renders a dead row for a detector added later", () => {
    // proactive.rs can grow a fourteenth type without this file knowing.
    const rows = model([alert("some_future_detector")]).attention;
    const row = rows.find(r => r.id.includes("some_future_detector"));
    expect(row).toBeDefined();
    expect(row!.destination).toBe("health");
    expect(row!.actionLabel.trim().length).toBeGreaterThan(0);
  });

  it("maps the detector's severity words onto the three the UI draws", () => {
    const severityOf = (raw: string) => model([alert("dead_stock", raw)]).attention[0].severity;
    expect(severityOf("critical")).toBe("critical");
    expect(severityOf("high")).toBe("critical");
    expect(severityOf("warning")).toBe("warning");
    expect(severityOf("info")).toBe("info");
    expect(severityOf("low")).toBe("info");
    // Anything unrecognised is a warning, never dropped and never critical.
    expect(severityOf("weird")).toBe("warning");
  });

  it("puts money findings above plumbing", () => {
    // A margin collapse and a sync notice are not the same news. The alert
    // rows are pushed before the infrastructure rows are appended.
    const withSync: OfficeAiOverviewSnapshot = {
      ...snapshot([alert("margin_erosion", "critical")]),
      sync: { online: false, hub_configured: true, mode: "hub", hub_url: "", pending_events: 4,
        last_successful_sync_at: null, days_since_last_sync: null, last_error: null, device_id: "d1" } as never,
    };
    const rows = buildOfficePulseModel(withSync, 0, 3, t).attention;
    const alertIndex = rows.findIndex(r => r.id.includes("margin_erosion"));
    const syncIndex = rows.findIndex(r => r.id.startsWith("sync:"));
    expect(alertIndex).toBeGreaterThanOrEqual(0);
    expect(syncIndex).toBeGreaterThanOrEqual(0);
    expect(alertIndex).toBeLessThan(syncIndex);
  });

  it("shows nothing extra when the detector has found nothing", () => {
    expect(model([]).attention.filter(r => r.id.startsWith("alert:"))).toEqual([]);
  });
});
