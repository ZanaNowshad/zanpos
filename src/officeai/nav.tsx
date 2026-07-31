import type React from "react";
import {
  Activity,
  Archive,
  BarChart3,
  Bot,
  Boxes,
  ClipboardCheck,
  ClipboardList,
  GitCompareArrows,
  HeartPulse,
  History,
  Inbox,
  LineChart,
  LayoutDashboard,
  Medal,
  Package,
  PackagePlus,
  PanelLeftClose,
  PanelLeftOpen,
  RefreshCw,
  Settings,
  ShieldCheck,
  ShoppingBag,
  Truck,
  UserCog,
  Users,
} from "lucide-react";
import type { OfficePrimarySpace, OfficeTab } from "./officeAiTypes";

export interface OfficeNavItem {
  id: OfficeTab;
  label: string;
  description?: string;
  managerOnly?: boolean;
  ownerOnly?: boolean;
}

export interface OfficeNavSection {
  id: "command" | "operations" | "growth" | "system";
  label: string;
  items: OfficeNavItem[];
}

// ─── Operation sub-tabs (consolidated single source of truth) ─────────────────
export interface OfficeSubTab {
  id: OfficeTab;
  label: string;
  description: string;
  manager?: boolean;
  owner?: boolean;
}

export interface OfficeSubNavGroup {
  id: string;
  label: string;
  tabs: OfficeTab[];
}

export const OPERATION_TABS: OfficeSubTab[] = [
  { id: "products", label: "Products", description: "Catalog and pricing" },
  { id: "categories", label: "Categories", description: "Product groups" },
  { id: "inventory", label: "Inventory", description: "Stock levels" },
  { id: "purchasing", label: "Purchasing", description: "Suppliers and POs", manager: true },
  { id: "reports", label: "Reports", description: "Sales and refunds" },
  { id: "cashier", label: "Cashiers", description: "Cashier reports" },
  { id: "eod", label: "End of Day", description: "Cashup workflow" },
  { id: "deliveries", label: "Deliveries", description: "Delivery queue" },
  { id: "customers", label: "Customers", description: "Customer records" },
  { id: "users", label: "Users", description: "Staff access", manager: true },
];

export const CONTROL_TABS: OfficeSubTab[] = [
  { id: "actions", label: "Action Review", description: "Approve AI changes", manager: true },
  { id: "workflows", label: "Inbox", description: "WhatsApp, bills, and payments", manager: true },
  { id: "health", label: "Health", description: "Database, hub, sync, AI", manager: true },
  { id: "conflicts", label: "Conflict Inbox", description: "Resolve cross-device differences", manager: true },
  { id: "insights", label: "Insights", description: "Sales and product signals", manager: true },
  { id: "loyalty", label: "Loyalty", description: "Customer retention base", manager: true },
  { id: "settings", label: "Settings", description: "App configuration" },
  { id: "audit", label: "Audit", description: "Audit log", owner: true },
  { id: "devices", label: "Devices", description: "Device management", owner: true },
];

export const OPERATION_GROUPS: OfficeSubNavGroup[] = [
  { id: "catalog", label: "Catalog", tabs: ["products", "categories", "inventory", "purchasing"] },
  { id: "sales", label: "Sales", tabs: ["reports", "cashier", "eod", "deliveries"] },
  { id: "people", label: "People", tabs: ["customers", "users"] },
];

export const CONTROL_GROUPS: OfficeSubNavGroup[] = [
  { id: "review", label: "Review", tabs: ["actions", "audit"] },
  { id: "inbox", label: "Inbox", tabs: ["workflows"] },
  { id: "system", label: "System", tabs: ["health", "conflicts", "devices"] },
  { id: "growth", label: "Growth", tabs: ["insights", "loyalty"] },
  { id: "settings", label: "Settings", tabs: ["settings"] },
];

export const OPERATION_TAB_IDS: OfficeTab[] = OPERATION_TABS.map(t => t.id);
export const CONTROL_TAB_IDS: OfficeTab[] = CONTROL_TABS.map(t => t.id);

// ─── Primary space mapping ────────────────────────────────────────────────────
export function primarySpaceForTab(tab: OfficeTab): OfficePrimarySpace {
  if (tab === "overview") return "home";
  if (tab === "assistant") return "ask-ai";
  if (tab === "operations" || OPERATION_TAB_IDS.includes(tab)) return "operations";
  return "control";
}

export function isOperationTab(tab: OfficeTab): boolean {
  return tab === "operations" || OPERATION_TAB_IDS.includes(tab);
}

export function isControlTab(tab: OfficeTab): boolean {
  return CONTROL_TAB_IDS.includes(tab);
}

export const OFFICE_SECTIONS: OfficeNavSection[] = [
  {
    id: "command",
    label: "Command",
    items: [
      { id: "overview", label: "Overview", description: "Live store command view" },
      { id: "assistant", label: "Assistant", description: "Ask, inspect, and act with AI" },
      { id: "actions", label: "Action Review", description: "Approve AI changes", managerOnly: true },
      { id: "workflows", label: "Inbox", description: "WhatsApp, bills, and payments", managerOnly: true },
    ],
  },
  {
    id: "operations",
    label: "Run",
    items: [
      { id: "operations", label: "Store Operations", description: "Catalog, stock, orders, sales" },
    ],
  },
  {
    id: "growth",
    label: "Growth",
    items: [
      { id: "insights", label: "Insights", description: "Sales and product signals", managerOnly: true },
      { id: "loyalty", label: "Loyalty", description: "Customer retention base", managerOnly: true },
    ],
  },
  {
    id: "system",
    label: "System",
    items: [
      { id: "health", label: "Health", description: "Database, hub, sync, AI", managerOnly: true },
      { id: "conflicts", label: "Conflict Inbox", description: "Resolve cross-device differences", managerOnly: true },
      { id: "settings", label: "Settings" },
      { id: "devices", label: "Devices", ownerOnly: true },
      { id: "audit", label: "Audit", ownerOnly: true },
    ],
  },
];

export function visibleTabsFor(role: string): OfficeTab[] {
  const manager = role === "owner" || role === "manager";
  const operations: OfficeTab[] = ["operations"];
  const catalog: OfficeTab[] = ["products", "categories", "inventory"];
  const purchasing: OfficeTab[] = manager ? ["purchasing"] : [];
  const sales: OfficeTab[] = ["reports", "cashier", "eod", "deliveries"];
  const people: OfficeTab[] = ["customers", "users"];
  const growth: OfficeTab[] = manager ? ["insights", "loyalty"] : [];
  const command: OfficeTab[] = manager
    ? ["overview", "assistant", "actions", "workflows", "health", "conflicts"]
    : ["overview", "assistant"];

  switch (role) {
    case "owner":
      return [...command, ...operations, ...catalog, ...purchasing, ...sales, ...people, ...growth, "settings", "audit", "devices"];
    case "manager":
      return [...command, ...operations, ...catalog, ...purchasing, ...sales, ...people, ...growth, "settings"];
    case "accountant":
      return ["overview", "assistant", "operations", "reports", "eod", "cashier", "customers", "insights", "audit"];
    case "cashier":
      return ["overview", "assistant", "operations", "products", "categories", "inventory", "reports", "cashier", "eod", "deliveries", "customers"];
    default:
      return [...command, ...operations, ...catalog, ...sales, ...people, "settings"];
  }
}

export function sectionsForRole(role: string): OfficeNavSection[] {
  const visible = new Set(visibleTabsFor(role));
  return OFFICE_SECTIONS
    .map(section => ({
      ...section,
      items: section.items.filter(item => visible.has(item.id)),
    }))
    .filter(section => section.items.length > 0);
}

export const TAB_ICON: Record<OfficeTab, React.ReactNode> = {
  overview: <LayoutDashboard size={17} strokeWidth={1.85} />,
  assistant: <Bot size={17} strokeWidth={1.85} />,
  actions: <ClipboardCheck size={17} strokeWidth={1.85} />,
  workflows: <Inbox size={17} strokeWidth={1.85} />,
  health: <HeartPulse size={17} strokeWidth={1.85} />,
  conflicts: <GitCompareArrows size={17} strokeWidth={1.85} />,
  insights: <LineChart size={17} strokeWidth={1.85} />,
  loyalty: <Medal size={17} strokeWidth={1.85} />,
  operations: <Boxes size={17} strokeWidth={1.85} />,
  products: <Package size={17} strokeWidth={1.85} />,
  categories: <Archive size={17} strokeWidth={1.85} />,
  inventory: <Boxes size={17} strokeWidth={1.85} />,
  purchasing: <PackagePlus size={17} strokeWidth={1.85} />,
  reports: <BarChart3 size={17} strokeWidth={1.85} />,
  cashier: <UserCog size={17} strokeWidth={1.85} />,
  eod: <ShoppingBag size={17} strokeWidth={1.85} />,
  deliveries: <Truck size={17} strokeWidth={1.85} />,
  customers: <Users size={17} strokeWidth={1.85} />,
  users: <ShieldCheck size={17} strokeWidth={1.85} />,
  settings: <Settings size={17} strokeWidth={1.85} />,
  audit: <History size={17} strokeWidth={1.85} />,
  devices: <Activity size={17} strokeWidth={1.85} />,
};

export const IcoStocktake = ClipboardList;
export const IcoSync = RefreshCw;
export const IcoClose = PanelLeftClose;
export const IcoOpen = PanelLeftOpen;
