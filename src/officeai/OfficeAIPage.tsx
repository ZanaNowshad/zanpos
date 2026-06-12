import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { SessionUser } from "../types";
import type { OfficeTab } from "./officeAiTypes";
import { useChatController } from "./useChatController";
import { BASE_SECTIONS, visibleTabsFor } from "./nav";
import ChatPanel from "./ChatPanel";
import CopilotDock from "./CopilotDock";
import ProviderSetup from "./ProviderSetup";
import ConfirmActionModal from "../components/ConfirmActionModal";
import { settingsGetBranch } from "../tauri/commands";
import { adminGetProviderConfig } from "../tauri/commands";

// Reuse existing tab components
import ProductsTab from "../components/ProductsTab";
import CategoriesTab from "../components/CategoriesTab";
import UsersTab from "../components/UsersTab";
import ReportsTab from "../components/ReportsTab";
import InventoryTab from "../components/InventoryTab";
import CustomersTab from "../components/CustomersTab";
import SettingsTab from "../components/SettingsTab";
import AuditLogTab from "../components/AuditLogTab";
import DevicesTab from "../components/DevicesTab";
import CashierReportTab from "../components/CashierReportTab";
import EodCashupTab from "../components/EodCashupTab";
import DeliveriesTab from "../components/DeliveriesTab";
import BulkStockTakeModal from "../components/BulkStockTakeModal";
import SyncQueueModal from "../components/SyncQueueModal";

interface Props { sessionUser: SessionUser; onBackToPOS: () => void; }

export default function OfficeAIPage({ sessionUser, onBackToPOS }: Props) {
  const [tab, setTab] = useState<OfficeTab>("products");
  const [tabEpoch, setTabEpoch] = useState(0);
  const [collapsed, setCollapsed] = useState(false);
  const [dockOpen, setDockOpen] = useState(() => localStorage.getItem("zanpos_oa_dock") !== "0");
  const [businessName, setBusinessName] = useState("your store");
  const [providerSetup, setProviderSetup] = useState(false);
  const [showBulkStockTake, setShowBulkStockTake] = useState(false);
  const [showSyncQueue, setShowSyncQueue] = useState(false);
  const composerRef = useRef<HTMLTextAreaElement>(null);

  const isOwner = sessionUser.role_name === "owner";
  const isManager = isOwner || sessionUser.role_name === "manager";
  const visibleTabs = useMemo(() => visibleTabsFor(sessionUser.role_name), [sessionUser.role_name]);

  useEffect(() => {
    settingsGetBranch(sessionUser.user_id).then(b => { if (b?.name) setBusinessName(b.name); }).catch(() => {});
    adminGetProviderConfig(sessionUser.user_id).then(cfg => { if (!cfg?.provider) setProviderSetup(true); }).catch(() => {});
  }, [sessionUser.user_id]);

  const getUiContext = useCallback(() => `Active tab: ${tab}`, [tab]);
  const onNavigate = useCallback((t: string) => { if (visibleTabs.includes(t as OfficeTab)) setTab(t as OfficeTab); }, [visibleTabs]);
  const onMutationApplied = useCallback(() => { setTabEpoch(e => e + 1); }, []);

  const ctrl = useChatController({ sessionUser, getUiContext, onNavigate, onMutationApplied });

  // Keyboard shortcuts
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (ctrl.chatState === "confirm") return;
      const active = document.activeElement as HTMLElement | null;
      if (e.key === "Escape") {
        if (active?.tagName === "TEXTAREA" || active?.tagName === "INPUT") { (active as HTMLElement).blur(); return; }
        onBackToPOS(); return;
      }
      if (e.ctrlKey && e.key === "/") { e.preventDefault(); setDockOpen(d => !d); if (!dockOpen) setTimeout(() => composerRef.current?.focus(), 100); return; }
      if ((e.ctrlKey || e.metaKey) && e.key >= "1" && e.key <= "9") {
        e.preventDefault(); const idx = parseInt(e.key) - 1;
        const tabs = visibleTabs.filter(t => t !== "assistant"); if (idx < tabs.length) setTab(tabs[idx]);
      }
    };
    document.addEventListener("keydown", onKey); return () => document.removeEventListener("keydown", onKey);
  }, [ctrl.chatState, dockOpen, onBackToPOS, visibleTabs]);

  useEffect(() => { localStorage.setItem("zanpos_oa_dock", dockOpen ? "1" : "0"); }, [dockOpen]);

  const sections = useMemo(() => BASE_SECTIONS.map(s => {
    if (s.label === "System" && isOwner) return { ...s, items: [...s.items, { id: "audit" as OfficeTab, label: "Audit Log" }, { id: "devices" as OfficeTab, label: "Devices" }] };
    return s;
  }), [isOwner]);
  const allItems = sections.flatMap(s => s.items);
  const activeItem = allItems.find(i => i.id === tab);
  const initials = sessionUser.display_name.split(" ").map(w => w[0]).slice(0, 2).join("").toUpperCase();
  const roleLabel = sessionUser.role_name.charAt(0).toUpperCase() + sessionUser.role_name.slice(1);
  const tabKey = `${tab}:${tabEpoch}`;

  const renderTab = () => {
    switch (tab) {
      case "assistant": return <ChatPanel ctrl={ctrl} variant="full" composerRef={composerRef} />;
      case "products":   return <ProductsTab sessionUserId={sessionUser.user_id} key={tabKey} />;
      case "categories": return <CategoriesTab sessionUserId={sessionUser.user_id} key={tabKey} />;
      case "users":      return <UsersTab sessionUserId={sessionUser.user_id} key={tabKey} />;
      case "reports":    return <ReportsTab sessionUserId={sessionUser.user_id} key={tabKey} />;
      case "cashier":    return <CashierReportTab sessionUserId={sessionUser.user_id} key={tabKey} />;
      case "eod":        return <EodCashupTab sessionUserId={sessionUser.user_id} key={tabKey} />;
      case "inventory":  return <InventoryTab sessionUserId={sessionUser.user_id} key={tabKey} />;
      case "customers":  return <CustomersTab sessionUserId={sessionUser.user_id} key={tabKey} />;
      case "settings":   return <SettingsTab sessionUserId={sessionUser.user_id} sessionRole={sessionUser.role_name} key={tabKey} />;
      case "audit":      return isOwner ? <AuditLogTab sessionUserId={sessionUser.user_id} key={tabKey} /> : null;
      case "devices":    return isOwner ? <DevicesTab sessionUserId={sessionUser.user_id} key={tabKey} /> : null;
      case "deliveries": return <DeliveriesTab sessionUser={sessionUser} key={tabKey} />;
      default: return null;
    }
  };

  if (providerSetup) return <ProviderSetup actorUserId={sessionUser.user_id} onDone={() => { setProviderSetup(false); ctrl.fetchKpi(); }} onBack={onBackToPOS} />;

  return (
    <div className="oa-page">
      <aside className={`bo-sidebar ${collapsed ? "bo-sidebar-collapsed" : ""}`}>
        <div className="bo-sidebar-header"><span className="bo-sidebar-brand">OfficeAI</span><button className="bo-sidebar-toggle" onClick={() => setCollapsed(c => !c)}>{collapsed ? "☰" : "◀"}</button></div>
        <nav className="bo-sidebar-nav">{sections.map((section, si) => (<div key={si} className="bo-sidebar-section">{!collapsed && section.label && <span className="bo-sidebar-section-label">{section.label}</span>}{section.items.filter(i => visibleTabs.includes(i.id)).map(item => (<button key={item.id} className={`bo-sidebar-item ${tab === item.id ? "bo-sidebar-item-active" : ""}`} onClick={() => setTab(item.id)}><span className="bo-sidebar-label">{!collapsed ? item.label : item.label[0]}</span></button>))}</div>))}</nav>
        <div className="bo-sidebar-footer">
          {isManager && (<div className="bo-sidebar-actions"><button className="bo-sidebar-action-btn" onClick={() => setShowBulkStockTake(true)}>📦 Stock-Take</button>{isOwner && <button className="bo-sidebar-action-btn" onClick={() => setShowSyncQueue(true)}>🔄 Sync Queue</button>}</div>)}
          {!collapsed && (<div className="bo-user-card"><div className="bo-user-avatar">{initials}</div><div className="bo-user-info"><div className="bo-user-name">{sessionUser.display_name}</div><div className="bo-user-role">{roleLabel}</div></div></div>)}
          <button className="bo-close-sidebar-btn" onClick={onBackToPOS}>✕ Close</button>
        </div>
      </aside>
      <main className="bo-main"><div className="bo-page-header"><h1 className="bo-page-title">{activeItem?.label ?? tab}</h1></div><div className="bo-content">{renderTab()}</div></main>
      <CopilotDock ctrl={ctrl} kpi={ctrl.kpi} open={dockOpen} onToggle={() => setDockOpen(false)} onExpand={() => setTab("assistant")} businessName={businessName} />
      {showBulkStockTake && <BulkStockTakeModal user={sessionUser} onClose={() => setShowBulkStockTake(false)} />}
      {showSyncQueue && <SyncQueueModal sessionUserId={sessionUser.user_id} onClose={() => setShowSyncQueue(false)} />}
      {ctrl.pendingAction && <ConfirmActionModal preview={ctrl.pendingAction.preview} onConfirm={ctrl.handleConfirm} onCancel={ctrl.handleCancel} />}
    </div>
  );
}
