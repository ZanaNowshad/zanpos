/**
 * Unified system-wide status model.
 *
 * Severity levels follow the design brief specification:
 *   ok             — Normal operation
 *   info           — Informational, no action needed
 *   setup-required — Configuration needed before feature works
 *   attention      — Requires review but not blocking
 *   degraded       — Reduced functionality, may recover automatically
 *   blocked        — Feature unavailable, requires manual intervention
 *   critical       — Service or data integrity at risk, immediate action required
 */

export type SeverityLevel =
  | "ok"
  | "info"
  | "setup-required"
  | "attention"
  | "degraded"
  | "blocked"
  | "critical";

/** Maps to StatusPill's StatusLevel for backward compatibility. */
export type PillLevel = "ok" | "warning" | "critical" | "info";

/** A single status signal emitted by a subsystem. */
export interface StatusItem {
  /** Unique identifier, e.g. "sync", "ai", "whatsapp", "printer". */
  id: string;
  severity: SeverityLevel;
  icon?: string;
  label: string;
  description?: string;
  /** Which user-facing capability does this affect? */
  affectedCapability: string;
  /** Recommended action, if any. */
  availableAction?: string;
  /** Callback when the user clicks the action/fix button. */
  onAction?: () => void;
  /** Can the user dismiss this status? */
  dismissible: boolean;
  /** Should this status remain visible even if dismissed (persistent)? */
  persistent: boolean;
  /** If this status was escalated from a lower severity, reference the original id. */
  escalatedFrom?: string;
  /** Unix timestamp in ms when this status was created. */
  createdAt: number;
}

/**
 * Map a SeverityLevel to a PillLevel for StatusPill / DegradedBanner compatibility.
 */
export function severityToPillLevel(severity: SeverityLevel): PillLevel {
  switch (severity) {
    case "ok": return "ok";
    case "info": return "info";
    case "setup-required": return "info";
    case "attention": return "warning";
    case "degraded": return "warning";
    case "blocked": return "critical";
    case "critical": return "critical";
  }
}

/**
 * Return the most severe level from a list. Used to compute overall system status.
 */
export function worstSeverity(levels: SeverityLevel[]): SeverityLevel {
  const order: SeverityLevel[] = ["ok", "info", "setup-required", "attention", "degraded", "blocked", "critical"];
  let worst: SeverityLevel = "ok";
  for (const l of levels) {
    if (order.indexOf(l) > order.indexOf(worst)) worst = l;
  }
  return worst;
}

/** Human-readable label for severity levels. */
export function severityLabel(level: SeverityLevel): string {
  switch (level) {
    case "ok": return "Operational";
    case "info": return "Info";
    case "setup-required": return "Setup Required";
    case "attention": return "Needs Attention";
    case "degraded": return "Degraded";
    case "blocked": return "Unavailable";
    case "critical": return "Critical";
  }
}
