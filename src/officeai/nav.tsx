/**
 * Tab iconography.
 *
 * This file previously also declared three navigation models — OPERATION_TABS /
 * CONTROL_TABS (the two secondary sidebars), OFFICE_SECTIONS (Command / Run /
 * Growth / System), and the primarySpaceForTab projection that gated which
 * secondary sidebar rendered. All three were removed when navigation was
 * consolidated into src/navigation/config.tsx.
 *
 * What remains is the icon map, which is genuinely shared: the rail, the
 * contextual section nav, and the command palette all render the same glyph
 * for a given destination.
 */
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
  Globe,
  HeartPulse,
  History,
  Inbox,
  LayoutDashboard,
  LineChart,
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
  Bike,
} from "lucide-react";
import type { OfficeTab } from "./officeAiTypes";

export const TAB_ICON: Record<OfficeTab, React.ReactNode> = {
  overview: <LayoutDashboard size={17} strokeWidth={1.75} />,
  assistant: <Bot size={17} strokeWidth={1.75} />,
  actions: <ClipboardCheck size={17} strokeWidth={1.75} />,
  workflows: <Inbox size={17} strokeWidth={1.75} />,
  health: <HeartPulse size={17} strokeWidth={1.75} />,
  conflicts: <GitCompareArrows size={17} strokeWidth={1.75} />,
  insights: <LineChart size={17} strokeWidth={1.75} />,
  loyalty: <Medal size={17} strokeWidth={1.75} />,
  operations: <Boxes size={17} strokeWidth={1.75} />,
  products: <Package size={17} strokeWidth={1.75} />,
  categories: <Archive size={17} strokeWidth={1.75} />,
  inventory: <Boxes size={17} strokeWidth={1.75} />,
  purchasing: <PackagePlus size={17} strokeWidth={1.75} />,
  zanshop: <Globe size={17} strokeWidth={1.75} />,
  reports: <BarChart3 size={17} strokeWidth={1.75} />,
  cashier: <UserCog size={17} strokeWidth={1.75} />,
  eod: <ShoppingBag size={17} strokeWidth={1.75} />,
  deliveries: <Truck size={17} strokeWidth={1.75} />,
  customers: <Users size={17} strokeWidth={1.75} />,
  users: <ShieldCheck size={17} strokeWidth={1.75} />,
  riders: <Bike size={17} strokeWidth={1.75} />,
  settings: <Settings size={17} strokeWidth={1.75} />,
  audit: <History size={17} strokeWidth={1.75} />,
  devices: <Activity size={17} strokeWidth={1.75} />,
};

export const IcoStocktake = ClipboardList;
export const IcoSync = RefreshCw;
export const IcoClose = PanelLeftClose;
export const IcoOpen = PanelLeftOpen;
