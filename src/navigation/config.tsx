/**
 * ZANPOS Command — canonical navigation model.
 *
 * THIS IS THE ONLY PLACE NAVIGATION IS DECLARED.
 *
 * It replaces five competing models that previously coexisted:
 *   1. PRIMARY_ENTRIES      (officeai/OfficeAIPrimaryNav) — 4 "spaces"
 *   2. ALL_DOMAINS          (command/CommandSidebar)      — 8 domains
 *   3. OPERATION_/CONTROL_* (officeai/nav)                — 2 secondary sidebars
 *   4. OFFICE_SECTIONS      (officeai/nav)                — Command/Run/Growth/System
 *   5. COMMAND_ROUTES       (command/routes)              — 7 route domains
 *
 * Navigation has exactly two levels:
 *   L1  domain   — the persistent rail. Always visible, always one active.
 *   L2  section  — contextual, rendered ONLY for the active domain, and only
 *                  when that domain has more than one section.
 *
 * Level 3 (tabs) is reserved for different views of the SAME entity or
 * workflow (e.g. the purchasing workspace's Suppliers/Orders/Receiving), and
 * is owned by the page component — never by this file.
 *
 * Invariant: every OfficeTab appears in exactly one domain. The unit test in
 * src/__tests__/navigationConfig.test.ts enforces this.
 */
import type { ReactNode } from "react";
import {
  BarChart3,
  Gauge,
  LayoutDashboard,
  Package,
  PackagePlus,
  ShieldCheck,
  Users,
} from "lucide-react";
import type { OfficeTab } from "../officeai/officeAiTypes";
import type { OfficeAiStringKey } from "../i18n/officeAiStrings";

export type DomainId =
  | "today"
  | "catalogue"
  | "purchasing"
  | "customers"
  | "team"
  | "insights"
  | "operations";

export interface NavSection {
  /** The OfficeTab this section renders. */
  id: OfficeTab;
  labelKey: OfficeAiStringKey;
  /** Stable URL path used for deep links, palette results and breadcrumbs. */
  path: string;
  managerOnly?: boolean;
  ownerOnly?: boolean;
}

export interface NavDomain {
  id: DomainId;
  labelKey: OfficeAiStringKey;
  icon: ReactNode;
  path: string;
  /** Tab shown when the domain is selected from the rail. */
  defaultTab: OfficeTab;
  /**
   * Contextual L2 entries. When fewer than two are visible to the current
   * role, no secondary navigation is rendered at all — this is what stops a
   * one-item sidebar from ever appearing.
   */
  sections: NavSection[];
  managerOnly?: boolean;
  ownerOnly?: boolean;
}

const ICON = { size: 19, strokeWidth: 1.7 } as const;

export const NAVIGATION: NavDomain[] = [
  {
    id: "today",
    labelKey: "today",
    icon: <LayoutDashboard {...ICON} />,
    path: "/today",
    defaultTab: "overview",
    sections: [],
  },
  {
    id: "catalogue",
    labelKey: "catalogue",
    icon: <Package {...ICON} />,
    path: "/catalogue",
    defaultTab: "products",
    sections: [
      { id: "products",   labelKey: "products",   path: "/catalogue/products" },
      { id: "categories", labelKey: "categories", path: "/catalogue/categories" },
      { id: "inventory",  labelKey: "inventory",  path: "/catalogue/inventory" },
      { id: "quickpos",   labelKey: "quickPos",   path: "/catalogue/quick-pos" },
    ],
  },
  {
    id: "purchasing",
    labelKey: "purchasing",
    icon: <PackagePlus {...ICON} />,
    path: "/purchasing",
    defaultTab: "purchasing",
    // Suppliers / Orders / Receiving / Cost exceptions are L3 tabs owned by
    // OfficeAIPurchasingWorkspace — one workflow, several views of it.
    sections: [],
    managerOnly: true,
  },
  {
    id: "customers",
    labelKey: "customers",
    icon: <Users {...ICON} />,
    path: "/customers",
    defaultTab: "customers",
    sections: [
      // "Directory", not "Customers" — a "Customers › Customers" breadcrumb
      // reads as a bug even though it is technically correct.
      { id: "customers", labelKey: "directory",  path: "/customers/directory" },
      { id: "loyalty",   labelKey: "loyalty",    path: "/customers/loyalty",  managerOnly: true },
      { id: "zanshop",   labelKey: "storefront", path: "/customers/shop",     managerOnly: true },
    ],
  },
  {
    id: "team",
    labelKey: "team",
    icon: <ShieldCheck {...ICON} />,
    path: "/team",
    defaultTab: "users",
    // Riders sit beside Staff rather than inside it. A rider has no login, no
    // role and no PIN, so putting them in the staff list would either invent
    // credentials nobody uses or put them on the cashier-selection screen.
    sections: [
      { id: "users",  labelKey: "staff",  path: "/team/staff" },
      { id: "riders", labelKey: "riders", path: "/team/riders" },
    ],
    managerOnly: true,
  },
  {
    id: "insights",
    labelKey: "insights",
    icon: <BarChart3 {...ICON} />,
    path: "/insights",
    defaultTab: "reports",
    sections: [
      { id: "reports",  labelKey: "reports",  path: "/insights/reports" },
      // Shift takings and the end-of-day cash-up are reporting, not selling —
      // they answer "what happened", which is what this domain is for.
      { id: "cashier",  labelKey: "cashiers", path: "/insights/shifts" },
      { id: "eod",      labelKey: "endOfDay", path: "/insights/end-of-day" },
      { id: "insights", labelKey: "insights", path: "/insights/signals", managerOnly: true },
    ],
  },
  {
    // Review and System were separate rail slots for work done a handful of
    // times a month, which is how "where do I check sync?" became a guess.
    // One place for the machinery: what needs approving, what disagrees, what
    // is broken, and how it is configured.
    id: "operations",
    labelKey: "operations",
    icon: <Gauge {...ICON} />,
    path: "/operations",
    defaultTab: "health",
    sections: [
      { id: "health",    labelKey: "health",        path: "/operations/health" },
      { id: "actions",   labelKey: "actionReview",  path: "/operations/actions" },
      { id: "workflows", labelKey: "inbox",         path: "/operations/inbox" },
      { id: "conflicts", labelKey: "conflictInbox", path: "/operations/conflicts" },
      { id: "devices",   labelKey: "devices",       path: "/operations/devices", ownerOnly: true },
      { id: "settings",  labelKey: "settings",      path: "/operations/settings" },
      { id: "audit",     labelKey: "audit",         path: "/operations/audit",   ownerOnly: true },
    ],
    // Machinery and configuration are not a cashier's job.
    managerOnly: true,
  },
];

/**
 * Tabs that are reachable but are not rail destinations.
 *
 * `assistant` is deliberately not a domain: ZanAI is a global capability
 * available from the header and contextually on every page, not a silo.
 * `operations` is a legacy alias kept so old deep links keep working.
 */
export const ALIAS_TABS: Record<string, OfficeTab> = {
  operations: "products",
};

// ─── Role filtering ───────────────────────────────────────────────────────────

export interface RoleFlags {
  isOwner: boolean;
  isManager: boolean;
}

export function roleFlags(role: string): RoleFlags {
  const isOwner = role === "owner";
  return { isOwner, isManager: isOwner || role === "manager" };
}

function permitted(item: { managerOnly?: boolean; ownerOnly?: boolean }, r: RoleFlags): boolean {
  if (item.ownerOnly && !r.isOwner) return false;
  if (item.managerOnly && !r.isManager) return false;
  return true;
}

/** Rail entries visible to a role, with their sections already filtered. */
export function domainsForRole(role: string): NavDomain[] {
  const r = roleFlags(role);
  return NAVIGATION
    .filter(d => permitted(d, r))
    .map(d => ({ ...d, sections: d.sections.filter(s => permitted(s, r)) }));
}

/**
 * Sections to render as L2 for a domain. Returns [] when the domain has one
 * or zero visible sections, so a single-item sidebar can never be produced.
 */
export function sectionsForDomain(domainId: DomainId, role: string): NavSection[] {
  const domain = domainsForRole(role).find(d => d.id === domainId);
  if (!domain || domain.sections.length < 2) return [];
  return domain.sections;
}

/**
 * The tab a domain opens for a given role.
 *
 * For a sectioned domain this must be the first *permitted* section, not the
 * declared default — System defaults to Health, which a cashier may not see.
 */
export function entryTabForDomain(domain: NavDomain): OfficeTab {
  if (domain.sections.length === 0) return domain.defaultTab;
  const declared = domain.sections.find(s => s.id === domain.defaultTab);
  return (declared ?? domain.sections[0]).id;
}

/** Every tab a role may open. */
export function visibleTabsForRole(role: string): OfficeTab[] {
  const out = new Set<OfficeTab>(["overview", "assistant"]);
  for (const d of domainsForRole(role)) {
    out.add(entryTabForDomain(d));
    for (const s of d.sections) out.add(s.id);
  }
  return [...out];
}

// ─── Resolution ───────────────────────────────────────────────────────────────

const TAB_TO_DOMAIN = new Map<OfficeTab, DomainId>();
for (const d of NAVIGATION) {
  TAB_TO_DOMAIN.set(d.defaultTab, d.id);
  for (const s of d.sections) TAB_TO_DOMAIN.set(s.id, d.id);
}

/**
 * Which rail item should be active for a tab.
 *
 * `assistant` resolves to "today" so the rail keeps a stable active item while
 * the assistant workspace is open, rather than showing nothing selected.
 */
export function domainForTab(tab: OfficeTab): DomainId {
  if (tab === "assistant") return "today";
  const aliased = ALIAS_TABS[tab] ?? tab;
  return TAB_TO_DOMAIN.get(aliased) ?? "today";
}

export function pathForTab(tab: OfficeTab): string {
  const aliased = ALIAS_TABS[tab] ?? tab;
  for (const d of NAVIGATION) {
    if (d.defaultTab === aliased && d.sections.length === 0) return d.path;
    const section = d.sections.find(s => s.id === aliased);
    if (section) return section.path;
  }
  return "/today";
}

export function tabForPath(path: string): OfficeTab | undefined {
  const clean = path.replace(/\/+$/, "");
  for (const d of NAVIGATION) {
    if (d.path === clean) return d.defaultTab;
    const section = d.sections.find(s => s.path === clean);
    if (section) return section.id;
  }
  return undefined;
}

/** Breadcrumb trail for a tab: [domain] or [domain, section]. */
export interface Crumb { labelKey: OfficeAiStringKey; tab?: OfficeTab }

export function breadcrumbForTab(tab: OfficeTab): Crumb[] {
  const aliased = ALIAS_TABS[tab] ?? tab;
  const domainId = domainForTab(aliased);
  const domain = NAVIGATION.find(d => d.id === domainId);
  if (!domain) return [];
  const crumbs: Crumb[] = [{ labelKey: domain.labelKey, tab: domain.defaultTab }];
  // A domain with no sections is a single view — one crumb is the whole truth.
  // Otherwise the section is always named, even when it is the domain default,
  // so the user can see which of several sections they are looking at.
  const section = domain.sections.find(s => s.id === aliased);
  if (section) crumbs.push({ labelKey: section.labelKey, tab: section.id });
  return crumbs;
}

/** Flat list for the command palette — derived, never hand-maintained. */
export interface PaletteEntry {
  id: OfficeTab;
  labelKey: OfficeAiStringKey;
  domainLabelKey: OfficeAiStringKey;
  path: string;
}

export function paletteEntriesForRole(role: string): PaletteEntry[] {
  const out: PaletteEntry[] = [];
  for (const d of domainsForRole(role)) {
    if (d.sections.length === 0) {
      out.push({ id: d.defaultTab, labelKey: d.labelKey, domainLabelKey: d.labelKey, path: d.path });
    } else {
      for (const s of d.sections) {
        out.push({ id: s.id, labelKey: s.labelKey, domainLabelKey: d.labelKey, path: s.path });
      }
    }
  }
  return out;
}
