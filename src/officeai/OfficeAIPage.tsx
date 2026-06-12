import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { GhostSummary, ProductPrefill, ProviderConfig, SessionUser } from "../types";
import { DEVICE } from "../types";
import { adminGetProviderConfig, ghostSummary, settingsGetBranch } from "../tauri/commands";
import type { OfficeTab } from "./officeAiTypes";
import { BASE_SECTIONS, TAB_ICON, IcoStocktake, IcoSync, IcoClose, visibleTabsFor } from "./nav";
import { useChatController } from "./useChatController";
import ChatPanel from "./ChatPanel";
import CopilotDock from "./CopilotDock";
import ProviderSetup from "./ProviderSetup";
import { KpiSidebar } from "./KpiSidebar";
import ConfirmActionModal from "../components/ConfirmActionModal";

// Reused back-office tabs — same components, same props as the old BackOfficeModal
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

interface Props {
  sessionUser: SessionUser;
  onBackToPOS: () => void;
}

export default function OfficeAIPage({ sessionUser, onBackToPOS }: Props) {
  const [tab, setTab]             = useState<OfficeTab>("products");
  const [tabEpoch, setTabEpoch]   = useState(0);
  const [collapsed, setCollapsed] = useState(false);
  const [dockOpen, setDockOpen]   = useState(() => localStorage.getItem("zanpos_oa_dock") !== "0");
  const [businessName, setBusinessName] = useState("");
  const [config, setConfig]       = useState<ProviderConfig | null>(null);
  const [configLoaded, setConfigLoaded] = useState(false);
  const [showBulkStockTake, setShowBulkStockTake] = useState(false);
  const [showSyncQueue, setShowSyncQueue]         = useState(false);
  const [ghostSum, setGhostSum] = useState<GhostSummary>({ pending: 0, found: 0, not_found: 0 });
  const [productPrefill, setProductPrefill] = useState<ProductPrefill | null>(null);
  const composerRef = useRef<HTMLTextAreaElement>(null);

  const isOwner   = sessionUser.role_name === "owner";
  const isManager = isOwner || sessionUser.role_name === "manager";
  const visibleTabs = useMemo(() => visibleTabsFor(sessionUser.role_name), [sessionUser.role_name]);
  const configured = Boolean(config?.provider);

  // ── Branch name + provider config ───────────────────────────────────────────
  useEffect(() => {
    settingsGetBranch(sessionUser.user_id).then(b => { if (b?.name) setBusinessName(b.name); }).catch(() => {});
    adminGetProviderConfig(sessionUser.user_id)
      .then(cfg => setConfig(cfg))
      .catch(() => setConfig(null))
      .finally(() => setConfigLoaded(true));
  }, [sessionUser.user_id]);

  const providerLabel = config?.provider === "anthropic"
    ? "◆ Claude"
    : config?.provider === "openai"
    ? `⬡ ${config.openai_model}`
    : config?.provider === "gemini"
    ? `✦ ${config.gemini_model}`
    : "";

  // ── Ghost barcode summary (Products badge) ──────────────────────────────────
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

  // ── Chat controller — single instance shared by dock + Assistant tab ────────
  const tabRef = useRef(tab);
  useEffect(() => { tabRef.current = tab; }, [tab]);
  const getUiContext = useCallback(
    () => `The admin is in the ZANPOS OfficeAI workspace, currently viewing the "${tabRef.current}" tab.`,
    []
  );
  const onNavigate = useCallback((t: string) => {
    // RBAC gate: the AI cannot steer a manager into owner-only tabs.
    if ((visibleTabsFor(sessionUser.role_name) as string[]).includes(t)) {
      setTab(t as OfficeTab);
    }
  }, [sessionUser.role_name]);
  const onMutationApplied = useCallback(() => setTabEpoch(e => e + 1), []);

  const ctrl = useChatController({ sessionUser, getUiContext, onNavigate, onMutationApplied });

  // ── Keyboard: Esc → POS, Ctrl+/ → dock, Ctrl+1-9 → data tabs ───────────────
  useEffect(() => { localStorage.setItem("zanpos_oa_dock", dockOpen ? "1" : "0"); }, [dockOpen]);

  const dataTabs = useMemo(() => visibleTabs.filter(t => t !== "assistant"), [visibleTabs]);
  const chatStateRef = useRef(ctrl.chatState);
  useEffect(() => { chatStateRef.current = ctrl.chatState; }, [ctrl.chatState]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // While a mutation confirm is pending, ConfirmActionModal owns the keyboard.
      if (chatStateRef.current === "confirm") return;
      if (e.key === "Escape") {
        const active = document.activeElement as HTMLElement | null;
        // First Escape blurs the composer / a form field; second one exits.
        if (active && (active.tagName === "TEXTAREA" || active.tagName === "INPUT" || active.tagName === "SELECT")) {
          active.blur();
          return;
        }
        e.preventDefault();
        onBackToPOS();
        return;
      }
      if (e.ctrlKey && e.key === "/") {
        e.preventDefault();
        setDockOpen(d => {
          const next = !d;
          if (next) setTimeout(() => composerRef.current?.focus(), 120);
          return next;
        });
        return;
      }
      if ((e.ctrlKey || e.metaKey) && e.key >= "1" && e.key <= "9") {
        e.preventDefault();
        const idx = parseInt(e.key, 10) - 1;
        if (idx < dataTabs.length) setTab(dataTabs[idx]);
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [dataTabs, onBackToPOS]);

  // ── Nav sections (owner-only items injected into System) ────────────────────
  const sections = useMemo(() => BASE_SECTIONS.map(s => {
    if (s.label === "System" && isOwner) {
      return {
        ...s,
        items: [
          ...s.items,
          { id: "audit"   as OfficeTab, label: "Audit Log" },
          { id: "devices" as OfficeTab, label: "Devices"   },
        ],
      };
    }
    return s;
  }), [isOwner]);

  const allItems   = useMemo(() => sections.flatMap(s => s.items), [sections]);
  const activeItem = allItems.find(i => i.id === tab);

  const initials = sessionUser.display_name
    .split(" ").map(w => w[0]).slice(0, 2).join("").toUpperCase();
  const roleLabel =
    sessionUser.role_name.charAt(0).toUpperCase() + sessionUser.role_name.slice(1);

  const userName = sessionUser.display_name;
  const storeName = businessName || "your store";
  const tabKey = `${tab}:${tabEpoch}`;

  // ── Tab routing — back-office tabs remount on epoch bump after AI mutations ──
  const renderDataTab = () => {
    switch (tab) {
      case "products": return (
        <ProductsTab
          key={tabKey}
          sessionUserId={sessionUser.user_id}
          ghostSummary={ghostSum}
          ghostPrefill={productPrefill}
          onGhostPrefillConsumed={() => setProductPrefill(null)}
          onGhostCountChange={refreshGhostSummary}
          onCreateProductFromGhost={(prefill) => setProductPrefill(prefill)}
        />
      );
      case "categories": return <CategoriesTab    key={tabKey} sessionUserId={sessionUser.user_id} />;
      case "users":      return <UsersTab         key={tabKey} sessionUserId={sessionUser.user_id} />;
      case "reports":    return <ReportsTab       key={tabKey} sessionUserId={sessionUser.user_id} />;
      case "cashier":    return <CashierReportTab key={tabKey} sessionUserId={sessionUser.user_id} />;
      case "eod":        return <EodCashupTab     key={tabKey} sessionUserId={sessionUser.user_id} />;
      case "inventory":  return <InventoryTab     key={tabKey} sessionUserId={sessionUser.user_id} />;
      case "customers":  return <CustomersTab     key={tabKey} sessionUserId={sessionUser.user_id} />;
      case "settings":   return <SettingsTab      key={tabKey} sessionUserId={sessionUser.user_id} sessionRole={sessionUser.role_name} />;
      case "audit":      return isOwner ? <AuditLogTab key={tabKey} sessionUserId={sessionUser.user_id} /> : null;
      case "devices":    return isOwner ? <DevicesTab  key={tabKey} sessionUserId={sessionUser.user_id} /> : null;
      case "deliveries": return <DeliveriesTab    key={tabKey} sessionUser={sessionUser} />;
      default: return null;
    }
  };

  return (
    <div className="oa-page">
      {/* ── Left Sidebar — ported verbatim from BackOfficeModal ──────────── */}
      <aside className={`bo-sidebar${collapsed ? " bo-sidebar-collapsed" : ""}`}>

        <div className="bo-sidebar-logo">
          {!collapsed && <span className="bo-sidebar-brand">Office<span>AI</span></span>}
          <button
            className="bo-sidebar-toggle"
            onClick={() => setCollapsed(c => !c)}
            title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          >
            {collapsed ? "☰" : "◀"}
          </button>
          {!collapsed && <span className="bo-sidebar-subtitle">{storeName}</span>}
        </div>

        <nav className="bo-nav">
          {sections.map(section => (
            <div key={section.label || "assistant"} className="bo-nav-section">
              {!collapsed && section.label && <div className="bo-nav-section-label">{section.label}</div>}
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

        <div className="bo-sidebar-footer">
          {isManager && (
            <div className="bo-sidebar-actions">
              <button className="bo-sidebar-action-btn" onClick={() => setShowBulkStockTake(true)} title="Stock Take">
                <IcoStocktake /> {!collapsed && "Stock-Take"}
              </button>
              {isOwner && (
                <button className="bo-sidebar-action-btn" onClick={() => setShowSyncQueue(true)} title="Sync Queue">
                  <IcoSync /> {!collapsed && "Sync Queue"}
                </button>
              )}
            </div>
          )}

          {!collapsed && (
            <div className="bo-user-card">
              <div className="bo-user-avatar">{initials}</div>
              <div className="bo-user-info">
                <div className="bo-user-name">{sessionUser.display_name}</div>
                <div className="bo-user-role">{roleLabel}</div>
              </div>
            </div>
          )}

          <button className="bo-close-sidebar-btn" onClick={onBackToPOS} title="Back to POS (Esc)">
            <IcoClose size={13} />
            {!collapsed && "Back to POS"}
          </button>
        </div>
      </aside>

      {/* ── Main Content ───────────────────────────────────────────────────── */}
      <main className="bo-main">
        {tab === "assistant" ? (
          <div className="oa-assistant-host">
            {!configLoaded ? null : configured ? (
              <>
                <ChatPanel
                  ctrl={ctrl}
                  variant="full"
                  userName={userName}
                  businessName={storeName}
                  composerRef={composerRef}
                />
                <KpiSidebar kpi={ctrl.kpi} currencyExp={DEVICE.currency_exponent} onRefresh={ctrl.fetchKpi} />
              </>
            ) : (
              <ProviderSetup
                actorUserId={sessionUser.user_id}
                onDone={(cfg) => { setConfig(cfg); ctrl.fetchKpi(); }}
                onBack={() => setTab("products")}
              />
            )}
          </div>
        ) : (
          <>
            <div className="bo-page-header">
              <h1 className="bo-page-title">
                <span className="bo-page-title-icon">{TAB_ICON[tab]}</span>
                {activeItem?.label ?? tab}
              </h1>
            </div>
            <div className="bo-content">
              {renderDataTab()}
            </div>
          </>
        )}
      </main>

      {/* ── Copilot dock — hidden on the Assistant tab (no duplicate composers) ── */}
      {dockOpen && tab !== "assistant" && (
        <CopilotDock
          ctrl={ctrl}
          userName={userName}
          businessName={storeName}
          providerLabel={providerLabel}
          configured={configured}
          onSetup={() => setTab("assistant")}
          onExpand={() => setTab("assistant")}
          onClose={() => setDockOpen(false)}
          composerRef={composerRef}
        />
      )}

      {/* ── Utility modals + AI confirm ────────────────────────────────────── */}
      {showBulkStockTake && (
        <BulkStockTakeModal user={sessionUser} onClose={() => setShowBulkStockTake(false)} />
      )}
      {showSyncQueue && (
        <SyncQueueModal sessionUserId={sessionUser.user_id} onClose={() => setShowSyncQueue(false)} />
      )}
      {ctrl.chatState === "confirm" && ctrl.pendingAction && (
        <ConfirmActionModal
          preview={ctrl.pendingAction.preview}
          onConfirm={ctrl.handleConfirm}
          onCancel={ctrl.handleCancel}
        />
      )}
    </div>
  );
}
