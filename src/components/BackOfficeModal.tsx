import { type ReactNode, useCallback, useEffect, useMemo, useState } from "react";
import type { SessionUser } from "../types";
import ProductsTab from "./ProductsTab";
import CategoriesTab from "./CategoriesTab";
import UsersTab from "./UsersTab";
import ReportsTab from "./ReportsTab";
import InventoryTab from "./InventoryTab";
import SettingsTab from "./SettingsTab";
import AuditLogTab from "./AuditLogTab";
import CustomersTab from "./CustomersTab";
import DevicesTab from "./DevicesTab";
import CashierReportTab from "./CashierReportTab";
import EodCashupTab from "./EodCashupTab";
import BulkStockTakeModal from "./BulkStockTakeModal";
import SyncQueueModal from "./SyncQueueModal";
import DeliveriesTab from "./DeliveriesTab";
import type { GhostSummary, ProductPrefill } from "../types";
import { ghostSummary } from "../tauri/commands";

// ── SVG Icon System ───────────────────────────────────────────────────────────
type IcoProps = { size?: number };

const Svg = ({ children, size = 16 }: { children: ReactNode; size?: number }) => (
  <svg
    width={size} height={size} viewBox="0 0 24 24" fill="none"
    stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round"
    aria-hidden="true"
  >
    {children}
  </svg>
);

const IcoProducts    = (p: IcoProps) => <Svg {...p}><path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/><polyline points="3.27 6.96 12 12.01 20.73 6.96"/><line x1="12" y1="22.08" x2="12" y2="12"/></Svg>;
const IcoCategories  = (p: IcoProps) => <Svg {...p}><path d="M20.59 13.41l-7.17 7.17a2 2 0 0 1-2.83 0L2 12V2h10l8.59 8.59a2 2 0 0 1 0 2.82z"/><line x1="7" y1="7" x2="7.01" y2="7" strokeWidth={2.5}/></Svg>;
const IcoInventory   = (p: IcoProps) => <Svg {...p}><path d="M12 2L2 7l10 5 10-5-10-5z"/><path d="M2 17l10 5 10-5"/><path d="M2 12l10 5 10-5"/></Svg>;
const IcoReports     = (p: IcoProps) => <Svg {...p}><line x1="18" y1="20" x2="18" y2="10"/><line x1="12" y1="20" x2="12" y2="4"/><line x1="6" y1="20" x2="6" y2="14"/></Svg>;
const IcoCashier     = (p: IcoProps) => <Svg {...p}><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></Svg>;
const IcoEod         = (p: IcoProps) => <Svg {...p}><line x1="12" y1="1" x2="12" y2="23"/><path d="M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6"/></Svg>;
const IcoDeliveries  = (p: IcoProps) => <Svg {...p}><rect x="1" y="3" width="15" height="13" rx="1"/><path d="M16 8h4l3 3v5h-7V8z"/><circle cx="5.5" cy="18.5" r="2.5"/><circle cx="18.5" cy="18.5" r="2.5"/></Svg>;
const IcoCustomers   = (p: IcoProps) => <Svg {...p}><path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M23 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/></Svg>;
const IcoUsers       = (p: IcoProps) => <Svg {...p}><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/><polyline points="16 11 18 13 22 9"/></Svg>;
const IcoSettings    = (p: IcoProps) => <Svg {...p}><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></Svg>;
const IcoAudit       = (p: IcoProps) => <Svg {...p}><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/></Svg>;
const IcoDevices     = (p: IcoProps) => <Svg {...p}><rect x="2" y="3" width="20" height="14" rx="2"/><line x1="8" y1="21" x2="16" y2="21"/><line x1="12" y1="17" x2="12" y2="21"/></Svg>;
const IcoStocktake   = (p: IcoProps) => <Svg {...p}><path d="M9 5H7a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-2"/><rect x="9" y="3" width="6" height="4" rx="1"/><polyline points="9 12 11 14 15 10"/></Svg>;
const IcoSync        = (p: IcoProps) => <Svg {...p}><path d="M23 4v6h-6"/><path d="M1 20v-6h6"/><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/></Svg>;
const IcoClose       = (p: IcoProps) => <Svg {...p}><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></Svg>;

// ── Tab definitions ───────────────────────────────────────────────────────────
type Tab =
  | "products" | "categories" | "users" | "reports"
  | "inventory" | "customers" | "settings" | "audit" | "devices"
  | "cashier" | "eod" | "deliveries";

interface NavItem  { id: Tab; label: string; }
interface NavSection { label: string; items: NavItem[]; }

const BASE_SECTIONS: NavSection[] = [
  {
    label: "Catalog",
    items: [
      { id: "products",   label: "Products"   },
      { id: "categories", label: "Categories" },
      { id: "inventory",  label: "Inventory"  },
    ],
  },
  {
    label: "Sales",
    items: [
      { id: "reports",    label: "Reports"    },
      { id: "cashier",    label: "By Cashier" },
      { id: "eod",        label: "EOD Cash-Up"},
      { id: "deliveries", label: "Deliveries" },
    ],
  },
  {
    label: "People",
    items: [
      { id: "customers", label: "Customers" },
      { id: "users",     label: "Users"     },
    ],
  },
  {
    label: "System",
    items: [
      { id: "settings", label: "Settings" },
    ],
  },
];

// Map tab id → icon component
const TAB_ICON: Record<Tab, ReactNode> = {
  products:   <IcoProducts />,
  categories: <IcoCategories />,
  inventory:  <IcoInventory />,
  reports:    <IcoReports />,
  cashier:    <IcoCashier />,
  eod:        <IcoEod />,
  deliveries: <IcoDeliveries />,
  customers:  <IcoCustomers />,
  users:      <IcoUsers />,
  settings:   <IcoSettings />,
  audit:      <IcoAudit />,
  devices:    <IcoDevices />,
};

// ── Component ─────────────────────────────────────────────────────────────────
interface Props {
  sessionUser: SessionUser;
  onClose: () => void;
}

export default function BackOfficeModal({ sessionUser, onClose }: Props) {
  const [tab, setTab]                             = useState<Tab>("products");
  const [collapsed, setCollapsed]                 = useState(false);
  const [showBulkStockTake, setShowBulkStockTake] = useState(false);
  const [showSyncQueue, setShowSyncQueue]         = useState(false);
  const [ghostSum, setGhostSum] = useState<GhostSummary>({ pending: 0, found: 0, not_found: 0 });
  const [productPrefill, setProductPrefill] = useState<ProductPrefill | null>(null);

  const isOwner   = sessionUser.role_name === "owner";
  const isManager = isOwner || sessionUser.role_name === "manager";

  const refreshGhostSummary = useCallback(async () => {
    if (!isManager) return;
    try {
      const s = await ghostSummary(sessionUser.user_id);
      setGhostSum(s);
    } catch { /* non-fatal */ }
  }, [isManager, sessionUser.user_id]);

  useEffect(() => { refreshGhostSummary(); }, [refreshGhostSummary]);

  useEffect(() => {
    if (productPrefill) setTab("products");
  }, [productPrefill]);

  // Inject owner-only items into System section
  const sections: NavSection[] = useMemo(() => BASE_SECTIONS.map(s => {
    if (s.label === "System" && isOwner) {
      return {
        ...s,
        items: [
          ...s.items,
          { id: "audit"   as Tab, label: "Audit Log" },
          { id: "devices" as Tab, label: "Devices"   },
        ],
      };
    }
    return s;
  }), [isOwner]);

  const allItems = useMemo(() => sections.flatMap(s => s.items), [sections]);
  const activeItem = allItems.find(i => i.id === tab);

  // Keyboard shortcuts: Escape to close, Ctrl+1-9 to switch tabs
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") { e.preventDefault(); onClose(); return; }
      if ((e.ctrlKey || e.metaKey) && e.key >= "1" && e.key <= "9") {
        e.preventDefault();
        const idx = parseInt(e.key) - 1;
        const tabs = sections.flatMap(s => s.items);
        if (idx < tabs.length) setTab(tabs[idx].id);
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [sections, onClose]);

  // Avatar initials
  const initials = sessionUser.display_name
    .split(" ").map(w => w[0]).slice(0, 2).join("").toUpperCase();
  const roleLabel =
    sessionUser.role_name.charAt(0).toUpperCase() + sessionUser.role_name.slice(1);

  return (
    <>
      <div className="bo-overlay">
        <div className="bo-shell">

          {/* ── Left Sidebar ─────────────────────────────────────────── */}
          <aside className={`bo-sidebar${collapsed ? " bo-sidebar-collapsed" : ""}`}>

            {/* Brand + Toggle */}
            <div className="bo-sidebar-logo">
              {!collapsed && <span className="bo-sidebar-brand">ZAN<span>POS</span></span>}
              <button
                className="bo-sidebar-toggle"
                onClick={() => setCollapsed(c => !c)}
                title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
              >
                {collapsed ? "☰" : "◀"}
              </button>
              {!collapsed && <span className="bo-sidebar-subtitle">Back Office</span>}
            </div>

            {/* Navigation */}
            <nav className="bo-nav">
              {sections.map(section => (
                <div key={section.label} className="bo-nav-section">
                  {!collapsed && <div className="bo-nav-section-label">{section.label}</div>}
                  {section.items.map(item => (
                    <button
                      key={item.id}
                      className={`bo-nav-item${tab === item.id ? " bo-nav-item-active" : ""}`}
                      onClick={() => setTab(item.id)}
                      title={collapsed ? item.label : undefined}
                    >
                      <span className="bo-nav-icon">{TAB_ICON[item.id]}</span>
                      {!collapsed && <span className="bo-nav-label">{item.label}</span>}
                      {!collapsed && item.id === "products" && isManager && (ghostSum.pending + ghostSum.found) > 0 && (
                        <span className="bo-nav-ghost-badge">
                          {ghostSum.pending + ghostSum.found}
                        </span>
                      )}
                    </button>
                  ))}
                </div>
              ))}
            </nav>

            {/* Footer */}
            <div className="bo-sidebar-footer">
              {/* Utility actions */}
              {(isManager || isOwner) && (
                <div className="bo-sidebar-actions">
                  {isManager && (
                    <button className="bo-sidebar-action-btn" onClick={() => setShowBulkStockTake(true)} title="Stock Take">
                      <IcoStocktake /> {!collapsed && "Stock-Take"}
                    </button>
                  )}
                  {isOwner && (
                    <button className="bo-sidebar-action-btn" onClick={() => setShowSyncQueue(true)} title="Sync Queue">
                      <IcoSync /> {!collapsed && "Sync Queue"}
                    </button>
                  )}
                </div>
              )}

              {/* User card */}
              {!collapsed && (
                <div className="bo-user-card">
                  <div className="bo-user-avatar">{initials}</div>
                  <div className="bo-user-info">
                    <div className="bo-user-name">{sessionUser.display_name}</div>
                    <div className="bo-user-role">{roleLabel}</div>
                  </div>
                </div>
              )}

              {/* Close */}
              <button className="bo-close-sidebar-btn" onClick={onClose} title="Close Back Office">
                <IcoClose size={13} />
                {!collapsed && "Close Back Office"}
              </button>
            </div>
          </aside>

          {/* ── Main Content ─────────────────────────────────────────── */}
          <main className="bo-main">
            <div className="bo-page-header">
              <h1 className="bo-page-title">
                <span className="bo-page-title-icon">{TAB_ICON[tab]}</span>
                {activeItem?.label ?? tab}
              </h1>
            </div>

            <div className="bo-content">
              {tab === "products"   && (
                <ProductsTab
                  sessionUserId={sessionUser.user_id}
                  ghostSummary={ghostSum}
                  ghostPrefill={productPrefill}
                  onGhostPrefillConsumed={() => setProductPrefill(null)}
                  onGhostCountChange={refreshGhostSummary}
                  onCreateProductFromGhost={(prefill) => setProductPrefill(prefill)}
                />
              )}
              {tab === "categories" && <CategoriesTab    sessionUserId={sessionUser.user_id} />}
              {tab === "users"      && <UsersTab         sessionUserId={sessionUser.user_id} />}
              {tab === "reports"    && <ReportsTab       sessionUserId={sessionUser.user_id} />}
              {tab === "cashier"    && <CashierReportTab sessionUserId={sessionUser.user_id} />}
              {tab === "eod"        && <EodCashupTab     sessionUserId={sessionUser.user_id} />}
              {tab === "inventory"  && <InventoryTab     sessionUserId={sessionUser.user_id} />}
              {tab === "customers"  && <CustomersTab     sessionUserId={sessionUser.user_id} />}
              {tab === "settings"   && <SettingsTab      sessionUserId={sessionUser.user_id} sessionRole={sessionUser.role_name} />}
              {tab === "audit"      && isOwner && <AuditLogTab  sessionUserId={sessionUser.user_id} />}
              {tab === "devices"    && isOwner && <DevicesTab   sessionUserId={sessionUser.user_id} />}
              {tab === "deliveries" && <DeliveriesTab    sessionUser={sessionUser} />}
            </div>
          </main>

        </div>
      </div>

      {showBulkStockTake && (
        <BulkStockTakeModal user={sessionUser} onClose={() => setShowBulkStockTake(false)} />
      )}
      {showSyncQueue && (
        <SyncQueueModal sessionUserId={sessionUser.user_id} onClose={() => setShowSyncQueue(false)} />
      )}
    </>
  );
}
