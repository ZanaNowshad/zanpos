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

type Tab = "products" | "categories" | "users" | "reports" | "inventory" | "customers" | "settings" | "audit" | "devices";

interface Props {
  sessionUser: SessionUser;
  onClose: () => void;
}

export default function BackOfficeModal({ sessionUser, onClose }: Props) {
  const [tab, setTab] = useState<Tab>("products");
  const isOwner = sessionUser.role_name === "owner";

  const tabs: Tab[] = ["products", "categories", "users", "reports", "inventory", "customers", "settings"];
  if (isOwner) { tabs.push("audit"); tabs.push("devices"); }

  const tabLabel = (t: Tab) => {
    if (t === "audit") return "Audit";
    if (t === "devices") return "Devices";
    if (t === "customers") return "Customers";
    return t.charAt(0).toUpperCase() + t.slice(1);
  };

  return (
    <div className="bo-overlay">
      <div className="bo-modal">
        {/* ── Header ── */}
        <div className="bo-header">
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
          <button className="bo-close-btn" onClick={onClose}>Close</button>
        </div>

        {/* ── Tab content ── */}
        <div className="bo-content">
          {tab === "products"   && <ProductsTab   sessionUserId={sessionUser.user_id} />}
          {tab === "categories" && <CategoriesTab sessionUserId={sessionUser.user_id} />}
          {tab === "users"      && <UsersTab sessionUserId={sessionUser.user_id} />}
          {tab === "reports"    && <ReportsTab sessionUserId={sessionUser.user_id} />}
          {tab === "inventory"  && <InventoryTab sessionUserId={sessionUser.user_id} />}
          {tab === "customers"  && <CustomersTab sessionUserId={sessionUser.user_id} />}
          {tab === "settings"   && <SettingsTab sessionUserId={sessionUser.user_id} />}
          {tab === "audit"      && isOwner && <AuditLogTab sessionUserId={sessionUser.user_id} />}
          {tab === "devices"    && isOwner && <DevicesTab sessionUserId={sessionUser.user_id} />}
        </div>
      </div>
    </div>
  );
}
