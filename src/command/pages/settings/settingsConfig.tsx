import type { ReactNode } from "react";
import {
  Bot, Cloud, CreditCard, Globe2, Printer, ShieldCheck, Store, Users, Wrench,
} from "lucide-react";

/**
 * The single declaration of the Settings information architecture.
 *
 * Every group below is backed by configuration that actually exists. Groups the
 * reference design implies but the product cannot honour — notification
 * preferences, billing, appearance themes — are deliberately absent.
 *
 * Two levels only: group in the left rail, sections inside one scrolling work
 * area. No tabs inside tabs.
 */
export type SettingsGroupId =
  | "store"
  | "sales"
  | "team"
  | "hardware"
  | "integrations"
  | "ai"
  | "data"
  | "security"
  | "advanced";

export interface SettingsGroup {
  id: SettingsGroupId;
  labelKey: string;
  /** One short line under the label in the rail. */
  descriptionKey: string;
  icon: ReactNode;
  managerOnly?: boolean;
  ownerOnly?: boolean;
  /**
   * Free-text terms the search box matches in addition to the label. These are
   * the words a shopkeeper actually types — "vat", "logo", "qr" — not internal
   * field names.
   */
  keywords: string[];
  /** Marks destructive/irreversible territory so the rail can separate it. */
  danger?: boolean;
}

const ICON = { size: 16, strokeWidth: 1.75 } as const;

export const SETTINGS_GROUPS: SettingsGroup[] = [
  {
    id: "store",
    labelKey: "setStore",
    descriptionKey: "setStoreDesc",
    icon: <Store {...ICON} />,
    keywords: ["store", "branch", "name", "address", "phone", "timezone", "vat", "tax number", "cr", "legal", "currency", "logo"],
  },
  {
    id: "sales",
    labelKey: "setSales",
    descriptionKey: "setSalesDesc",
    icon: <CreditCard {...ICON} />,
    keywords: ["receipt", "footer", "header", "prefix", "invoice", "tax rule", "vat rate", "discount", "rounding", "sales", "business rules", "practice", "training"],
  },
  {
    id: "team",
    labelKey: "setTeam",
    descriptionKey: "setTeamDesc",
    icon: <Users {...ICON} />,
    managerOnly: true,
    keywords: ["team", "users", "staff", "roles", "permissions", "access", "pin", "cashier"],
  },
  {
    id: "hardware",
    labelKey: "setHardware",
    descriptionKey: "setHardwareDesc",
    icon: <Printer {...ICON} />,
    keywords: ["printer", "thermal", "receipt printer", "paper", "port", "usb", "test print", "barcode", "scanner", "cash drawer", "hardware"],
  },
  {
    id: "integrations",
    labelKey: "setIntegrations",
    descriptionKey: "setIntegrationsDesc",
    icon: <Globe2 {...ICON} />,
    managerOnly: true,
    keywords: ["whatsapp", "storefront", "online shop", "zanshop", "cloudflare", "integration", "connect", "qr"],
  },
  {
    id: "ai",
    labelKey: "setAi",
    descriptionKey: "setAiDesc",
    icon: <Bot {...ICON} />,
    managerOnly: true,
    keywords: ["ai", "zanai", "provider", "api key", "model", "claude", "openai", "gemini", "assistant", "approval"],
  },
  {
    id: "data",
    labelKey: "setData",
    descriptionKey: "setDataDesc",
    icon: <Cloud {...ICON} />,
    managerOnly: true,
    keywords: ["sync", "hub", "cloud", "backup", "restore", "device", "data", "offline"],
  },
  {
    id: "security",
    labelKey: "setSecurity",
    descriptionKey: "setSecurityDesc",
    icon: <ShieldCheck {...ICON} />,
    managerOnly: true,
    keywords: ["security", "session", "timeout", "lock", "audit", "log", "compliance"],
  },
  {
    id: "advanced",
    labelKey: "setAdvanced",
    descriptionKey: "setAdvancedDesc",
    icon: <Wrench {...ICON} />,
    ownerOnly: true,
    danger: true,
    keywords: ["advanced", "maintenance", "reset", "rebuild", "index", "danger", "diagnostics", "version", "update"],
  },
];

export function groupsForRole(role: string): SettingsGroup[] {
  const isOwner = role === "owner";
  const isManager = isOwner || role === "manager";
  return SETTINGS_GROUPS.filter(g => {
    if (g.ownerOnly && !isOwner) return false;
    if (g.managerOnly && !isManager) return false;
    return true;
  });
}

/** True when the role may open this group at all — used for deep-link denial. */
export function canOpenGroup(id: SettingsGroupId, role: string): boolean {
  return groupsForRole(role).some(g => g.id === id);
}

/**
 * Lightweight search: label + description + keywords. The Settings surface is
 * nine groups, so a substring scan is the right amount of machinery — an index
 * would be more code than the thing it searches.
 */
export function searchGroups(
  query: string,
  role: string,
  label: (key: string) => string,
): SettingsGroup[] {
  const q = query.trim().toLowerCase();
  const visible = groupsForRole(role);
  if (!q) return visible;
  return visible.filter(g =>
    label(g.labelKey).toLowerCase().includes(q)
    || label(g.descriptionKey).toLowerCase().includes(q)
    || g.keywords.some(k => k.includes(q)));
}
