import { useState } from "react";
import type { SessionUser } from "../types";
import ProductsTab from "./ProductsTab";
import CategoriesTab from "./CategoriesTab";
import UsersTab from "./UsersTab";
import ReportsTab from "./ReportsTab";

type Tab = "products" | "categories" | "users" | "reports";

interface Props {
  sessionUser: SessionUser;
  onClose: () => void;
}

export default function BackOfficeModal({ sessionUser, onClose }: Props) {
  const [tab, setTab] = useState<Tab>("products");

  return (
    <div className="bo-overlay">
      <div className="bo-modal">
        {/* ── Header ── */}
        <div className="bo-header">
          <span className="bo-header-title">⚙ Back Office</span>
          <div className="bo-tabs">
            {(["products", "categories", "users", "reports"] as Tab[]).map(t => (
              <button
                key={t}
                className={`bo-tab ${tab === t ? "bo-tab-active" : ""}`}
                onClick={() => setTab(t)}
              >
                {t.charAt(0).toUpperCase() + t.slice(1)}
              </button>
            ))}
          </div>
          <button className="bo-close-btn" onClick={onClose}>✕ Close</button>
        </div>

        {/* ── Tab content ── */}
        <div className="bo-content">
          {tab === "products"   && <ProductsTab   sessionUserId={sessionUser.user_id} />}
          {tab === "categories" && <CategoriesTab />}
          {tab === "users"      && <UsersTab />}
          {tab === "reports"    && <ReportsTab />}
        </div>
      </div>
    </div>
  );
}
