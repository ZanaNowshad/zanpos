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

type Tab =
  | "products" | "categories" | "users" | "reports"
  | "inventory" | "customers" | "settings" | "audit" | "devices"
  | "cashier" | "eod";

interface Props {
  sessionUser: SessionUser;
  onClose: () => void;
}

export default function BackOfficeModal({ sessionUser, onClose }: Props) {
  const [tab, setTab] = useState<Tab>("products");
  const [showBulkStockTake, setShowBulkStockTake] = useState(false);
  const [showSyncQueue, setShowSyncQueue] = useState(false);
  const isOwner   = sessionUser.role_name === "owner";
  const isManager = isOwner || sessionUser.role_name === "manager";

  const tabs: Tab[] = ["products", "categories", "users", "reports", "cashier", "eod", "inventory", "customers", "settings"];
  if (isOwner) { tabs.push("audit"); tabs.push("devices"); }

  const tabLabel = (t: Tab) => {
    const labels: Partial<Record<Tab, string>> = {
      products:   "Products",
      categories: "Categories",
      users:      "Users",
      reports:    "Reports",
      cashier:    "By Cashier",
      eod:        "EOD Cash-Up",
      inventory:  "Inventory",
      customers:  "Customers",
      settings:   "Settings",
      audit:      "Audit",
      devices:    "Devices",
    };
    return labels[t] ?? t;
  };

  return (
    <>
      <div className="bo-overlay">
        <div className="bo-modal">
          {/* ── Header ── */}
          <div className="bo-header">
            <div className="bo-header-logo">
              <span className="bo-header-logo-text">ZAN<span>POS</span></span>
            </div>
            <span className="bo-header-title">Back Office</span>
            <div className="bo-tabs">
              {tabs.map(t => (
                <button
                  key={t}
                  className={`bo-tab ${tab === t ? "bo-tab-active" : ""}`}
                  onClick={() => setTab(t)}
                >
                  {tabLabel(t)}
                </button>
              ))}
            </div>
            <div className="bo-header-actions">
              {isManager && (
                <button
                  className="bo-action-btn"
                  onClick={() => setShowBulkStockTake(true)}
                  title="Bulk Stock-Take"
                >
                  📋 Stock-Take
                </button>
              )}
              {isOwner && (
                <button
                  className="bo-action-btn"
                  onClick={() => setShowSyncQueue(true)}
                  title="Sync Queue"
                >
                  ⚡ Sync Queue
                </button>
              )}
              <button className="bo-close-btn" onClick={onClose}>✕ Close</button>
            </div>
          </div>

          {/* ── Tab content ── */}
          <div className="bo-content">
            {tab === "products"   && <ProductsTab      sessionUserId={sessionUser.user_id} />}
            {tab === "categories" && <CategoriesTab    sessionUserId={sessionUser.user_id} />}
            {tab === "users"      && <UsersTab         sessionUserId={sessionUser.user_id} />}
            {tab === "reports"    && <ReportsTab       sessionUserId={sessionUser.user_id} />}
            {tab === "cashier"    && <CashierReportTab sessionUserId={sessionUser.user_id} />}
            {tab === "eod"        && <EodCashupTab     sessionUserId={sessionUser.user_id} />}
            {tab === "inventory"  && <InventoryTab     sessionUserId={sessionUser.user_id} />}
            {tab === "customers"  && <CustomersTab     sessionUserId={sessionUser.user_id} />}
            {tab === "settings"   && <SettingsTab      sessionUserId={sessionUser.user_id} />}
            {tab === "audit"      && isOwner && <AuditLogTab  sessionUserId={sessionUser.user_id} />}
            {tab === "devices"    && isOwner && <DevicesTab   sessionUserId={sessionUser.user_id} />}
          </div>
        </div>
      </div>

      {/* ── Sub-modals ── */}
      {showBulkStockTake && (
        <BulkStockTakeModal
          user={sessionUser}
          onClose={() => setShowBulkStockTake(false)}
        />
      )}
      {showSyncQueue && (
        <SyncQueueModal
          onClose={() => setShowSyncQueue(false)}
        />
      )}
    </>
  );
}
