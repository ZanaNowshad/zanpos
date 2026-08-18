/**
 * Capability status model.
 *
 * The product used to describe itself as "Offline" whenever the cloud was
 * unreachable, which is wrong and frightening: the till, the catalogue and the
 * printer all keep working from the local database. Capabilities therefore
 * report independently, and every degraded statement must say what STILL works.
 *
 * Severity uses the seven-level scale already defined in statusTypes.ts.
 * Derived purely from OfficeAiOverviewSnapshot — no new backend calls.
 */
import type { OfficeAiOverviewSnapshot, OfficeTab } from "../officeai/officeAiTypes";
import type { SeverityLevel } from "./statusTypes";

export type CapabilityId =
  | "store" | "sync" | "ai" | "whatsapp" | "storefront" | "devices";

export interface Capability {
  id: CapabilityId;
  /** i18n key for the capability name. */
  labelKey: string;
  severity: SeverityLevel;
  /** One short clause: the state. */
  state: string;
  /** What the user can still do. Required whenever severity is worse than ok. */
  stillWorks?: string;
  /** Queued/pending work held safely on this device. */
  queued?: string;
  /** Last known good moment, already formatted. */
  lastOk?: string;
  /** Where the user goes to fix it. */
  action?: { labelKey: string; tab: OfficeTab };
}

function timeAgo(iso: string | null | undefined): string | undefined {
  if (!iso) return undefined;
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return undefined;
  const mins = Math.floor((Date.now() - then) / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins} min ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs} h ago`;
  return `${Math.floor(hrs / 24)} d ago`;
}

/**
 * Local store operations. This is the floor: if the app rendered at all, the
 * local database answered, so this is only ever degraded by an explicit
 * database fault in the health report.
 */
function storeCapability(s: OfficeAiOverviewSnapshot): Capability {
  // HealthSummary.db_integrity is the authoritative local-database signal.
  const integrity = s.health?.summary?.db_integrity;
  const dbBad = Boolean(integrity) && integrity !== "ok";
  if (dbBad) {
    return {
      id: "store", labelKey: "capStore", severity: "critical",
      state: "Local database fault",
      stillWorks: "Contact support before taking payments.",
      action: { labelKey: "health", tab: "health" },
    };
  }
  return { id: "store", labelKey: "capStore", severity: "ok", state: "Selling and printing available" };
}

function syncCapability(s: OfficeAiOverviewSnapshot): Capability {
  const sync = s.sync;
  if (!sync) {
    return { id: "sync", labelKey: "capSync", severity: "info", state: "Not reported" };
  }
  if (!sync.hub_configured) {
    return {
      id: "sync", labelKey: "capSync", severity: "ok",
      state: "Standalone — this device keeps its own records",
    };
  }
  const pending = sync.pending_events ?? 0;
  const lastOk = timeAgo(sync.last_successful_sync_at);
  const stale = (sync.days_since_last_sync ?? 0) >= 1;

  if (!sync.online || sync.last_error) {
    const severity: SeverityLevel = stale || pending > 200 ? "blocked" : "degraded";
    return {
      id: "sync", labelKey: "capSync", severity,
      state: "Cloud sync unavailable",
      stillWorks: "Selling, printing and local edits are unaffected.",
      queued: pending > 0 ? `${pending} changes queued on this device` : "Nothing is waiting to send",
      lastOk, action: { labelKey: "health", tab: "health" },
    };
  }
  if (pending > 0) {
    return {
      id: "sync", labelKey: "capSync", severity: pending > 200 ? "attention" : "info",
      state: "Catching up",
      stillWorks: "Everything remains usable while changes send.",
      queued: `${pending} changes queued`, lastOk,
      action: { labelKey: "health", tab: "health" },
    };
  }
  return { id: "sync", labelKey: "capSync", severity: "ok", state: "Up to date", lastOk };
}

function aiCapability(s: OfficeAiOverviewSnapshot): Capability {
  if (!s.provider?.provider) {
    return {
      id: "ai", labelKey: "capAi", severity: "setup-required",
      state: "ZanAI is not set up",
      stillWorks: "Every part of the store works without it.",
      action: { labelKey: "settings", tab: "settings" },
    };
  }
  if (s.aiEnabled === false) {
    return {
      id: "ai", labelKey: "capAi", severity: "info",
      state: "ZanAI is turned off",
      stillWorks: "Every part of the store works without it.",
      action: { labelKey: "settings", tab: "settings" },
    };
  }
  return { id: "ai", labelKey: "capAi", severity: "ok", state: "Connected" };
}

function whatsappCapability(s: OfficeAiOverviewSnapshot): Capability {
  const wa = s.whatsapp;
  if (!wa) {
    return {
      id: "whatsapp", labelKey: "capWhatsapp", severity: "setup-required",
      state: "Not connected",
      stillWorks: "Selling and printing are unaffected.",
      action: { labelKey: "inbox", tab: "workflows" },
    };
  }
  if (!wa.connected) {
    return {
      id: "whatsapp", labelKey: "capWhatsapp", severity: "attention",
      state: "Disconnected",
      stillWorks: "Selling, printing and sync are unaffected.",
      queued: s.whatsappUnread > 0 ? `${s.whatsappUnread} messages waiting` : undefined,
      action: { labelKey: "inbox", tab: "workflows" },
    };
  }
  return { id: "whatsapp", labelKey: "capWhatsapp", severity: "ok", state: "Connected" };
}

function storefrontCapability(s: OfficeAiOverviewSnapshot): Capability {
  // The snapshot has no storefront field yet, so this reports setup-required
  // rather than inventing a state. See BACKEND note in the redesign docs.
  const enabled = (s.featureToggles as { zanshop_enabled?: boolean } | null)?.zanshop_enabled;
  if (enabled) {
    return { id: "storefront", labelKey: "capStorefront", severity: "ok", state: "Published" };
  }
  return {
    id: "storefront", labelKey: "capStorefront", severity: "setup-required",
    state: "Not published",
    stillWorks: "In-store selling is unaffected.",
    action: { labelKey: "storefront", tab: "zanshop" },
  };
}

function devicesCapability(s: OfficeAiOverviewSnapshot): Capability {
  // Device problems surface as health findings in the "device"/"printer" area.
  const finding = s.health?.findings?.find(f => /printer|device/i.test(f.area));
  if (finding) {
    return {
      id: "devices", labelKey: "capDevices", severity: "attention",
      state: finding.title,
      // A printer fault must never read as "cannot sell".
      stillWorks: "Selling continues; receipts can be reprinted later.",
      action: { labelKey: "devices", tab: "devices" },
    };
  }
  return { id: "devices", labelKey: "capDevices", severity: "ok", state: "Ready" };
}

const ORDER: SeverityLevel[] = [
  "ok", "info", "setup-required", "attention", "degraded", "blocked", "critical",
];

export function buildCapabilities(s: OfficeAiOverviewSnapshot): Capability[] {
  return [
    storeCapability(s),
    syncCapability(s),
    aiCapability(s),
    whatsappCapability(s),
    storefrontCapability(s),
    devicesCapability(s),
  ];
}

/** Worst capability, used for the compact shell summary. */
export function worstCapability(caps: Capability[]): Capability {
  return caps.reduce((worst, c) =>
    ORDER.indexOf(c.severity) > ORDER.indexOf(worst.severity) ? c : worst, caps[0]);
}

/** True when nothing needs a human. `setup-required` counts as needing one. */
export function allNormal(caps: Capability[]): boolean {
  return caps.every(c => c.severity === "ok" || c.severity === "info");
}
