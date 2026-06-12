import { useState } from "react";
import type { SessionUser } from "../types";
import type { OfficeTab } from "./officeAiTypes";
import { BASE_SECTIONS, visibleTabsFor } from "./nav";

interface Props { sessionUser: SessionUser; onBackToPOS: () => void; }

export default function OfficeAIPage({ sessionUser, onBackToPOS }: Props) {
  const [tab, setTab] = useState<OfficeTab>("products");
  const [collapsed, setCollapsed] = useState(false);

  const visibleTabs = visibleTabsFor(sessionUser.role_name);
  const isOwner = sessionUser.role_name === "owner";
  const initials = sessionUser.display_name.split(" ").map(w => w[0]).slice(0, 2).join("").toUpperCase();
  const roleLabel = sessionUser.role_name.charAt(0).toUpperCase() + sessionUser.role_name.slice(1);

  const sections = BASE_SECTIONS.map(s => {
    if (s.label === "System" && isOwner) {
      return { ...s, items: [...s.items, { id: "audit" as OfficeTab, label: "Audit Log" }, { id: "devices" as OfficeTab, label: "Devices" }] };
    }
    return s;
  });
  const allItems = sections.flatMap(s => s.items);
  const activeItem = allItems.find(i => i.id === tab);

  return (
    <div className="oa-page">
      <aside className={`bo-sidebar ${collapsed ? "bo-sidebar-collapsed" : ""}`}>
        <div className="bo-sidebar-header">
          <span className="bo-sidebar-brand">OfficeAI</span>
          <button className="bo-sidebar-toggle" onClick={() => setCollapsed(c => !c)}>{collapsed ? "☰" : "◀"}</button>
        </div>
        <nav className="bo-sidebar-nav">
          {sections.map((section, si) => (
            <div key={si} className="bo-sidebar-section">
              {!collapsed && section.label && <span className="bo-sidebar-section-label">{section.label}</span>}
              {section.items.filter(i => visibleTabs.includes(i.id)).map(item => (
                <button key={item.id} className={`bo-sidebar-item ${tab === item.id ? "bo-sidebar-item-active" : ""}`}
                  onClick={() => setTab(item.id)}>
                  <span className="bo-sidebar-label">{!collapsed ? item.label : item.label[0]}</span>
                </button>
              ))}
            </div>
          ))}
        </nav>
        <div className="bo-sidebar-footer">
          {!collapsed && (
            <div className="bo-user-card">
              <div className="bo-user-avatar">{initials}</div>
              <div className="bo-user-info"><div className="bo-user-name">{sessionUser.display_name}</div><div className="bo-user-role">{roleLabel}</div></div>
            </div>
          )}
          <button className="bo-close-sidebar-btn" onClick={onBackToPOS}>✕ Close</button>
        </div>
      </aside>

      <main className="bo-main">
        <div className="bo-page-header">
          <h1 className="bo-page-title">{activeItem?.label ?? tab}</h1>
        </div>
        <div className="bo-content" style={{ display: "flex", alignItems: "center", justifyContent: "center", height: "60vh", color: "var(--text-dim)" }}>
          <div style={{ textAlign: "center" }}>
            <p style={{ fontSize: "1.2rem", marginBottom: 8 }}>OfficeAI Workspace</p>
            <p style={{ fontSize: "0.85rem" }}>Tab content loading… Select a tab from the sidebar.</p>
          </div>
        </div>
      </main>
    </div>
  );
}
