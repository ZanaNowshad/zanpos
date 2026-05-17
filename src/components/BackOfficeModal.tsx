import { useState } from "react";
import type { SessionUser } from "../types";
import ProductsTab from "./ProductsTab";
import CategoriesTab from "./CategoriesTab";
import UsersTab from "./UsersTab";
import ReportsTab from "./ReportsTab";
import InventoryTab from "./InventoryTab";
import SettingsTab from "./SettingsTab";
import AuditLogTab from "./AuditLogTab";

type Tab = "products" | "categories" | "users" | "reports" | "inventory" | "settings" | "audit";

interface Props {
  sessionUser: SessionUser;
  onClose: () => void;
}

export default function BackOfficeModal({ sessionUser, onClose }: Props) {
  const [tab, setTab] = useState<Tab>("products");
  const isOwner = sessionUser.role_name === "owner";

  const tabs: Tab[] = ["products", "categories", "users", "reports", "inventory", "settings"];
  if (isOwner) tabs.push("audit");

  const tabLabel = (t: Tab) => {
    if (t === "audit") return "Audit";
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
          {tab === "categories" && <CategoriesTab />}
          {tab === "users"      && <UsersTab />}
          {tab === "reports"    && <ReportsTab sessionUserId={sessionUser.user_id} />}
          {tab === "inventory"  && <InventoryTab sessionUserId={sessionUser.user_id} />}
          {tab === "settings"   && <SettingsTab />}
          {tab === "audit"      && isOwner && <AuditLogTab />}
        </div>
      </div>
    </div>
  );
}
