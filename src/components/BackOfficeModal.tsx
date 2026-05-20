import { useState } from "react";
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

type Tab =
  | "products" | "categories" | "users" | "reports"
  | "inventory" | "customers" | "settings" | "audit" | "devices"
  | "cashier" | "eod" | "deliveries";

interface NavItem { id: Tab; icon: string; label: string; }
interface NavSection { label: string; items: NavItem[]; }

const BASE_SECTIONS: NavSection[] = [
  {
    label: "Catalog",
    items: [
      { id: "products",   icon: "📦", label: "Products"   },
      { id: "categories", icon: "🏷️", label: "Categories" },
      { id: "inventory",  icon: "🗃️", label: "Inventory"  },
    ],
  },
  {
    label: "Sales",
    items: [
      { id: "reports",    icon: "📈", label: "Reports"     },
      { id: "cashier",    icon: "👤", label: "By Cashier"  },
      { id: "eod",        icon: "💰", label: "EOD Cash-Up" },
      { id: "deliveries", icon: "🛵", label: "Deliveries"  },
    ],
  },
  {
    label: "People",
    items: [
      { id: "customers", icon: "👥", label: "Customers" },
      { id: "users",     icon: "🧑‍💼", label: "Users"     },
    ],
  },
  {
    label: "System",
    items: [
      { id: "settings", icon: "⚙️", label: "Settings" },
    ],
  },
];

interface Props {
  sessionUser: SessionUser;
  onClose: () => void;
}

export default function BackOfficeModal({ sessionUser, onClose }: Props) {
  const [tab, setTab]                       = useState<Tab>("products");
  const [showBulkStockTake, setShowBulkStockTake] = useState(false);
  const [showSyncQueue, setShowSyncQueue]   = useState(false);

  const isOwner   = sessionUser.role_name === "owner";
  const isManager = isOwner || sessionUser.role_name === "manager";

  // Inject owner-only items into System section
  const sections: NavSection[] = BASE_SECTIONS.map(s => {
    if (s.label === "System" && isOwner) {
      return {
        ...s,
        items: [
          ...s.items,
          { id: "audit" as Tab,   icon: "📋", label: "Audit Log" },
          { id: "devices" as Tab, icon: "🖥️", label: "Devices"   },
        ],
      };
    }
    return s;
  });

  const allItems = sections.flatMap(s => s.items);
  const activeItem = allItems.find(i => i.id === tab);

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
          <aside className="bo-sidebar">

            {/* Logo */}
            <div className="bo-sidebar-logo">
              <span className="bo-sidebar-brand">ZAN<span>POS</span></span>
              <span className="bo-sidebar-subtitle">Back Office</span>
            </div>

            {/* Navigation */}
            <nav className="bo-nav">
              {sections.map(section => (
                <div key={section.label} className="bo-nav-section">
                  <div className="bo-nav-section-label">{section.label}</div>
                  {section.items.map(item => (
                    <button
                      key={item.id}
                      className={`bo-nav-item${tab === item.id ? " bo-nav-item-active" : ""}`}
                      onClick={() => setTab(item.id)}
                    >
                      <span className="bo-nav-icon">{item.icon}</span>
                      <span className="bo-nav-label">{item.label}</span>
                    </button>
                  ))}
                </div>
              ))}
            </nav>

            {/* Footer */}
            <div className="bo-sidebar-footer">
              {/* Utility actions */}
              <div className="bo-sidebar-actions">
                {isManager && (
                  <button className="bo-sidebar-action-btn" onClick={() => setShowBulkStockTake(true)}>
                    <span className="bo-nav-icon">📋</span> Stock-Take
                  </button>
                )}
                {isOwner && (
                  <button className="bo-sidebar-action-btn" onClick={() => setShowSyncQueue(true)}>
                    <span className="bo-nav-icon">⚡</span> Sync Queue
                  </button>
                )}
              </div>

              {/* User card */}
              <div className="bo-user-card">
                <div className="bo-user-avatar">{initials}</div>
                <div className="bo-user-info">
                  <div className="bo-user-name">{sessionUser.display_name}</div>
                  <div className="bo-user-role">{roleLabel}</div>
                </div>
              </div>

              {/* Close */}
              <button className="bo-close-sidebar-btn" onClick={onClose}>
                ✕ Close Back Office
              </button>
            </div>
          </aside>

          {/* ── Main Content ─────────────────────────────────────────── */}
          <main className="bo-main">
            <div className="bo-page-header">
              <h1 className="bo-page-title">
                <span>{activeItem?.icon}</span>
                {activeItem?.label ?? tab}
              </h1>
            </div>

            <div className="bo-content">
              {tab === "products"   && <ProductsTab      sessionUserId={sessionUser.user_id} />}
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
        <SyncQueueModal onClose={() => setShowSyncQueue(false)} />
      )}
    </>
  );
}
