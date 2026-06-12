import type React from "react";
import type { OfficeTab } from "./officeAiTypes";

export const BASE_SECTIONS: { label: string; items: { id: OfficeTab; label: string }[] }[] = [
  {
    label: "",
    items: [{ id: "assistant", label: "Assistant" }],
  },
  {
    label: "Catalog",
    items: [
      { id: "products", label: "Products" },
      { id: "categories", label: "Categories" },
      { id: "inventory", label: "Inventory" },
    ],
  },
  {
    label: "Sales",
    items: [
      { id: "reports", label: "Reports" },
      { id: "cashier", label: "Cashiers" },
      { id: "eod", label: "End of Day" },
      { id: "deliveries", label: "Deliveries" },
    ],
  },
  {
    label: "People",
    items: [
      { id: "customers", label: "Customers" },
      { id: "users", label: "Users" },
    ],
  },
  {
    label: "System",
    items: [
      { id: "settings", label: "Settings" },
    ],
  },
];

export function visibleTabsFor(role: string): OfficeTab[] {
  const all = BASE_SECTIONS.flatMap(s => s.items.map(i => i.id));
  if (role === "owner") return [...all, "audit", "devices"];
  return all;
}

// SVG icon set (ported from BackOfficeModal)
function Svg({ children, size = 16 }: { children: React.ReactNode; size?: number }) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{children}</svg>;
}
type IcoProps = { size?: number };
export const IcoAssistant = (p: IcoProps) => <Svg {...p}><circle cx="12" cy="12" r="10"/><path d="M12 6v6l4 2"/></Svg>;
export const IcoProducts = (p: IcoProps) => <Svg {...p}><path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/><polyline points="3.27 6.96 12 12.01 20.73 6.96"/><line x1="12" y1="22.08" x2="12" y2="12"/></Svg>;
export const IcoCategories = (p: IcoProps) => <Svg {...p}><path d="M20.59 13.41l-7.17 7.17a2 2 0 0 1-2.83 0L2 12V2h10l8.59 8.59a2 2 0 0 1 0 2.82z"/><line x1="7" y1="7" x2="7.01" y2="7" strokeWidth={2.5}/></Svg>;
export const IcoInventory = (p: IcoProps) => <Svg {...p}><path d="M12 2L2 7l10 5 10-5-10-5z"/><path d="M2 17l10 5 10-5"/><path d="M2 12l10 5 10-5"/></Svg>;
export const IcoReports = (p: IcoProps) => <Svg {...p}><line x1="18" y1="20" x2="18" y2="10"/><line x1="12" y1="20" x2="12" y2="4"/><line x1="6" y1="20" x2="6" y2="14"/></Svg>;
export const IcoCashier = (p: IcoProps) => <Svg {...p}><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></Svg>;
export const IcoEod = (p: IcoProps) => <Svg {...p}><line x1="12" y1="1" x2="12" y2="23"/><path d="M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6"/></Svg>;
export const IcoDeliveries = (p: IcoProps) => <Svg {...p}><rect x="1" y="3" width="15" height="13" rx="1"/><path d="M16 8h4l3 3v5h-7V8z"/><circle cx="5.5" cy="18.5" r="2.5"/><circle cx="18.5" cy="18.5" r="2.5"/></Svg>;
export const IcoCustomers = (p: IcoProps) => <Svg {...p}><path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M23 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/></Svg>;
export const IcoUsers = (p: IcoProps) => <Svg {...p}><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/><polyline points="16 11 18 13 22 9"/></Svg>;
export const IcoSettings = (p: IcoProps) => <Svg {...p}><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></Svg>;

export const TAB_ICON: Record<string, React.ReactNode> = {
  assistant: <IcoAssistant />, products: <IcoProducts />, categories: <IcoCategories />,
  inventory: <IcoInventory />, reports: <IcoReports />, cashier: <IcoCashier />,
  eod: <IcoEod />, deliveries: <IcoDeliveries />, customers: <IcoCustomers />,
  users: <IcoUsers />, settings: <IcoSettings />, audit: <IcoReports />,
  devices: <IcoCategories />,
};
