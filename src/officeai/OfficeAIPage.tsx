import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { AiHandoff, DiagnosticReport, ProductPrefill, ProviderConfig, SessionUser } from "../types";
import { DEVICE } from "../types";
import { adminGetProviderConfig, adminRunDiagnostics, settingsGetBranch } from "../tauri/commands";
import type { OfficeAiOverviewSnapshot, OfficeTab } from "./officeAiTypes";
import { CONTROL_TABS, OPERATION_TABS, primarySpaceForTab, visibleTabsFor } from "./nav";
import type { OfficeSubTab } from "./nav";
import { officeAiActionQueue, officeAiOverview } from "./officeAiData";
import { useLanguage } from "../hooks/useLanguage";
import { officeAiTranslator } from "../i18n/officeAiStrings";
import {
  controlGroups as localizedControlGroups, controlTabs as buildControlTabs,
  operationGroups as localizedOperationGroups, operationTabs as buildOperationTabs,
} from "./officeAiNavigation";
import { buildOfficePagePulse, EMPTY_OFFICE_OVERVIEW } from "./officeAiPagePresentation";
import { useChatController } from "./useChatController";
import OfficeAIShell from "./OfficeAIShell";
import type { OfficePulseItem } from "./OfficeAIShell";
import OfficeAIPrimaryNav, { PRIMARY_ENTRIES } from "./OfficeAIPrimaryNav";
import OfficeAISecondaryNav from "./OfficeAISecondaryNav";
import OfficeAICommandPalette from "./OfficeAICommandPalette";
import OfficeAIOverview from "./OfficeAIOverview";
import OfficeAIAssistantWorkspace from "./OfficeAIAssistantWorkspace";
import OfficeAIActionReview from "./OfficeAIActionReview";
import OfficeAISystemHealth from "./OfficeAISystemHealth";
import OfficeAIWorkflowInbox from "./OfficeAIWorkflowInbox";
import OfficeAIConflictInbox from "./OfficeAIConflictInbox";
import OfficeAIGrowthWorkspace from "./OfficeAIGrowthWorkspace";
import OfficeAIPurchasingWorkspace from "./OfficeAIPurchasingWorkspace";
import CopilotDock from "./CopilotDock";
import ConfirmActionModal from "../components/ConfirmActionModal";
import { X } from "lucide-react";

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

interface Props {
  sessionUser: SessionUser; onBackToPOS: () => void;
  initialProductPrefill?: ProductPrefill | null; initialAiMessage?: AiHandoff | null;
  initialTab?: OfficeTab; initialMaintenancePane?: boolean;
}

export default function OfficeAIPage({
  sessionUser,
  onBackToPOS,
  initialProductPrefill,
  initialAiMessage,
  initialTab,
  initialMaintenancePane,
}: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => officeAiTranslator(language), [language]);
  const [tab, setTab] = useState<OfficeTab>(
    initialProductPrefill ? "products" : initialTab ?? "overview",
  );
  const [tabEpoch, setTabEpoch] = useState(0);
  const [collapsed, setCollapsed] = useState(false);
  const [dockOpen, setDockOpen] = useState(false);
  const [commandOpen, setCommandOpen] = useState(false);
  const [businessName, setBusinessName] = useState("");
  const [config, setConfig] = useState<ProviderConfig | null>(null);
  const [configLoaded, setConfigLoaded] = useState(false);
  const [overview, setOverview] = useState<OfficeAiOverviewSnapshot>(EMPTY_OFFICE_OVERVIEW);
  const [overviewLoading, setOverviewLoading] = useState(false);
  const [productPrefill, setProductPrefill] = useState<ProductPrefill | null>(initialProductPrefill ?? null);
  const [diagRunning, setDiagRunning] = useState(false);
  const [diagResult, setDiagResult] = useState<DiagnosticReport | null>(null);
  const composerRef = useRef<HTMLTextAreaElement>(null);

  const isOwner = sessionUser.role_name === "owner";
  const isManager = isOwner || sessionUser.role_name === "manager";
  const configured = Boolean(config?.provider);

  useEffect(() => {
    settingsGetBranch(sessionUser.user_id).then(b => { if (b?.name) setBusinessName(b.name); }).catch(() => {});
    adminGetProviderConfig(sessionUser.session_token)
      .then(cfg => setConfig(cfg))
      .catch(() => setConfig(null))
      .finally(() => setConfigLoaded(true));
  }, [sessionUser.user_id, sessionUser.session_token]);

  const refreshOverview = useCallback(async () => {
    setOverviewLoading(true);
    setOverview(prev => ({ ...prev, loading: true }));
    try {
      const next = await officeAiOverview(sessionUser.user_id, sessionUser.session_token, sessionUser.branch_id, t);
      setOverview(next);
      if (next.provider) setConfig(next.provider);
    } catch (e) {
      setOverview(prev => ({
        ...prev,
        loading: false,
        errors: [`${t("officeAiOverview")}: ${typeof e === "string" ? e : String(e)}`],
      }));
    } finally {
      setOverviewLoading(false);
    }
  }, [sessionUser.branch_id, sessionUser.user_id, sessionUser.session_token, t]);

  useEffect(() => { void refreshOverview(); }, [refreshOverview]);
  useEffect(() => { if (productPrefill) setTab("products"); }, [productPrefill]);

  const providerLabel = config?.provider === "anthropic"
    ? `Claude ${config.anthropic_model || ""}`.trim()
    : config?.provider === "openai"
      ? `OpenAI ${config.openai_model || ""}`.trim()
      : config?.provider === "gemini"
        ? `Gemini ${config.gemini_model || ""}`.trim()
        : "";

  const tabRef = useRef(tab);
  useEffect(() => { tabRef.current = tab; }, [tab]);
  const getUiContext = useCallback(
    () => `The admin is in the ZANPOS OfficeAI command center, currently viewing the "${tabRef.current}" workspace.`,
    [],
  );
  const onNavigate = useCallback((t: string) => {
    if ((visibleTabsFor(sessionUser.role_name) as string[]).includes(t)) setTab(t as OfficeTab);
  }, [sessionUser.role_name]);
  const openVisibleTab = useCallback((nextTab: OfficeTab) => {
    if ((visibleTabsFor(sessionUser.role_name) as string[]).includes(nextTab)) {
      setTab(nextTab);
    }
  }, [sessionUser.role_name]);
  const onMutationApplied = useCallback(() => {
    setTabEpoch(e => e + 1);
    void refreshOverview();
  }, [refreshOverview]);
  const ctrl = useChatController({ sessionUser, getUiContext, onNavigate, onMutationApplied });

  const sendPrompt = useCallback((prompt: string) => {
    setTab("assistant");
    window.setTimeout(() => {
      if (ctrl.chatState === "idle") void ctrl.handleSend(prompt);
      else ctrl.setInput(prompt);
      composerRef.current?.focus();
    }, 80);
  }, [ctrl]);

  const handleRunDiagnostics = useCallback(async () => {
    setDiagRunning(true);
    setDiagResult(null);
    try {
      const report = await adminRunDiagnostics();
      setDiagResult(report);
      if (report.ok) ctrl.dismissError();
      void refreshOverview();
    } catch (e) {
      setDiagResult({ ok: false, db_integrity: "", issues_found: [], issues_fixed: [], note: `Diagnostics failed: ${String(e)}` });
    } finally {
      setDiagRunning(false);
    }
  }, [ctrl, refreshOverview]);

  const aiHandoffSentRef = useRef(false);
  const aiHandoffQueueRef = useRef<AiHandoff | null>(null);

  useEffect(() => {
    if (aiHandoffSentRef.current) return;
    if (!initialAiMessage || !configLoaded || !configured) return;

    if (ctrl.chatState !== "idle") {
      aiHandoffQueueRef.current = initialAiMessage;
      return;
    }

    aiHandoffSentRef.current = true;
    setTab("assistant");
    if (initialAiMessage.imageBase64) {
      const mt = initialAiMessage.imageMediaType || "image/jpeg";
      ctrl.setImageAttachment({
        base64: initialAiMessage.imageBase64,
        mediaType: mt,
        previewUrl: `data:${mt};base64,${initialAiMessage.imageBase64}`,
      });
    }
    void ctrl.handleSend(initialAiMessage.text);
  }, [initialAiMessage, configLoaded, configured, ctrl]);

  useEffect(() => {
    const q = aiHandoffQueueRef.current;
    if (!q || ctrl.chatState !== "idle") return;
    aiHandoffQueueRef.current = null;
    aiHandoffSentRef.current = true;
    setTab("assistant");
    if (q.imageBase64) {
      const mt = q.imageMediaType || "image/jpeg";
      ctrl.setImageAttachment({
        base64: q.imageBase64,
        mediaType: mt,
        previewUrl: `data:${mt};base64,${q.imageBase64}`,
      });
    }
    void ctrl.handleSend(q.text);
  }, [ctrl.chatState, ctrl]);

  const visibleTabIds = useMemo(() => {
    return new Set(visibleTabsFor(sessionUser.role_name));
  }, [sessionUser.role_name]);
  const coreTabs = useMemo<OfficeSubTab[]>(() => [
    { id: "overview", label: t("home"), description: t("whatNeedsAttention") },
    { id: "assistant", label: t("askAi"), description: t("askInspectAct") },
  ], [t]);
  const translatedOperationTabs = useMemo(() => buildOperationTabs(t), [t]);
  const translatedControlTabs = useMemo(() => buildControlTabs(t), [t]);
  const paletteItems = useMemo(
    () => [...coreTabs, ...translatedOperationTabs, ...translatedControlTabs]
      .filter(item => visibleTabIds.has(item.id)),
    [coreTabs, translatedOperationTabs, translatedControlTabs, visibleTabIds],
  );
  const primaryEntries = useMemo(() => PRIMARY_ENTRIES.flatMap(entry => {
    if (visibleTabIds.has(entry.defaultTab)) return [entry];
    const fallback = paletteItems.find(item => primarySpaceForTab(item.id) === entry.space);
    return fallback ? [{ ...entry, defaultTab: fallback.id }] : [];
  }), [paletteItems, visibleTabIds]);

  const chatStateRef = useRef(ctrl.chatState);
  useEffect(() => { chatStateRef.current = ctrl.chatState; }, [ctrl.chatState]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (chatStateRef.current === "confirm") return;
      if (e.key === "Escape") {
        if (commandOpen) {
          e.preventDefault();
          setCommandOpen(false);
          return;
        }
        if (dockOpen) {
          e.preventDefault();
          setDockOpen(false);
          return;
        }
        const active = document.activeElement as HTMLElement | null;
        if (active && ["TEXTAREA", "INPUT", "SELECT"].includes(active.tagName)) {
          active.blur();
          return;
        }
        e.preventDefault();
        onBackToPOS();
        return;
      }
      if (e.ctrlKey && e.key === "/") {
        e.preventDefault();
        setDockOpen(d => !d);
        window.setTimeout(() => composerRef.current?.focus(), 120);
        return;
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setCommandOpen(true);
        return;
      }
      if ((e.ctrlKey || e.metaKey) && e.key >= "1" && e.key <= "4") {
        e.preventDefault();
        const idx = parseInt(e.key, 10) - 1;
        if (idx < primaryEntries.length) setTab(primaryEntries[idx].defaultTab);
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [commandOpen, dockOpen, onBackToPOS, primaryEntries]);

  const activeSpace = useMemo(() => primarySpaceForTab(tab), [tab]);
  const activeItem = paletteItems.find(item => item.id === tab)
    ?? (tab === "operations" ? { id: "operations" as OfficeTab, label: t("operations"), description: t("catalogSalesPeople") } : undefined)
    ?? { id: tab, label: tab, description: "ZanAI" };
  const storeName = businessName || t("yourStore");
  const tabKey = `${tab}:${tabEpoch}`;
  const actionItems = officeAiActionQueue(ctrl, t);
  const operationTabs = useMemo(
    () => translatedOperationTabs.filter(item => visibleTabIds.has(item.id)),
    [translatedOperationTabs, visibleTabIds],
  );
  const controlTabs = useMemo(
    () => translatedControlTabs.filter(item => visibleTabIds.has(item.id)),
    [translatedControlTabs, visibleTabIds],
  );
  const operationNavGroups = useMemo(() => localizedOperationGroups(t), [t]);
  const controlNavGroups = useMemo(() => localizedControlGroups(t), [t]);
  const activeOperationTab = OPERATION_TABS.some(t => t.id === tab) ? tab : "products";
  const activeControlTab = CONTROL_TABS.some(t => t.id === tab) ? tab : "actions";

  const pulseItems: OfficePulseItem[] = useMemo(
    () => buildOfficePagePulse({ overview, actionItems, configured, isManager, t }),
    [overview, actionItems, configured, isManager, t],
  );

  const handleSelectSpace = useCallback((entry: typeof PRIMARY_ENTRIES[number]) => {
    setTab(entry.defaultTab);
  }, []);

  const renderDataTab = () => {
    switch (tab) {
      case "operations": return <ProductsTab key={tabKey} sessionUserId={sessionUser.user_id} prefill={productPrefill} onPrefillConsumed={() => setProductPrefill(null)} />;
      case "products": return <ProductsTab key={tabKey} sessionUserId={sessionUser.user_id} prefill={productPrefill} onPrefillConsumed={() => setProductPrefill(null)} />;
      case "categories": return <CategoriesTab key={tabKey} sessionUserId={sessionUser.user_id} />;
      case "users": return <UsersTab key={tabKey} sessionUserId={sessionUser.user_id} />;
      case "reports": return <ReportsTab key={tabKey} sessionUserId={sessionUser.user_id} />;
      case "cashier": return <CashierReportTab key={tabKey} sessionUserId={sessionUser.user_id} />;
      case "eod": return <EodCashupTab key={tabKey} sessionUserId={sessionUser.user_id} />;
      case "inventory": return <InventoryTab key={tabKey} sessionUserId={sessionUser.user_id} />;
      case "customers": return <CustomersTab key={tabKey} sessionUserId={sessionUser.user_id} />;
      case "settings": return (
        <SettingsTab
          key={tabKey}
          sessionUserId={sessionUser.user_id}
          sessionToken={sessionUser.session_token}
          sessionRole={sessionUser.role_name}
          initialMaintenancePane={initialMaintenancePane}
        />
      );
      case "audit": return isOwner ? <AuditLogTab key={tabKey} sessionUserId={sessionUser.user_id} /> : null;
      case "devices": return isOwner ? <DevicesTab key={tabKey} sessionUserId={sessionUser.user_id} /> : null;
      case "deliveries": return <DeliveriesTab key={tabKey} sessionUser={sessionUser} />;
      case "purchasing": return (
        <OfficeAIPurchasingWorkspace
          key={tabKey}
          actorUserId={sessionUser.user_id}
          currencyExp={DEVICE.currency_exponent}
          onSendPrompt={sendPrompt}
        />
      );
      default: return null;
    }
  };

  const renderWorkspaceContent = () => {
    if (tab === "overview") {
      return (
        <OfficeAIOverview
          snapshot={overview}
          currencyExp={DEVICE.currency_exponent}
          canUseManagerTools={isManager}
          pendingActionCount={actionItems.filter(item => item.status === "pending").length}
          onOpenTab={openVisibleTab}
          onRefresh={refreshOverview}
        />
      );
    }
    if (tab === "assistant") {
      return (
        <OfficeAIAssistantWorkspace
          ctrl={ctrl}
          sessionUser={sessionUser}
          storeName={storeName}
          configured={configured}
          configLoaded={configLoaded}
          composerRef={composerRef}
          onProviderConfigured={setConfig}
          onBackFromSetup={() => setTab("overview")}
          onOpenHealth={() => openVisibleTab("health")}
          onOpenActions={() => openVisibleTab("actions")}
          canUseManagerTools={isManager}
        />
      );
    }
    if (tab === "actions") {
      return <OfficeAIActionReview items={actionItems} ctrl={ctrl} canApprove={isManager} />;
    }
    if (tab === "health") {
      return <OfficeAISystemHealth actorUserId={sessionUser.user_id} initialReport={overview.health} onReport={(health) => setOverview(prev => ({ ...prev, health }))} />;
    }
    if (tab === "conflicts") {
      return <OfficeAIConflictInbox actorUserId={sessionUser.user_id} />;
    }
    if (tab === "workflows") {
      return <OfficeAIWorkflowInbox actorUserId={sessionUser.user_id} sessionToken={sessionUser.session_token} onSendPrompt={sendPrompt} />;
    }
    if (tab === "insights" || tab === "loyalty") {
      return (
        <OfficeAIGrowthWorkspace
          mode={tab}
          actorUserId={sessionUser.user_id}
          snapshot={overview}
          currencyExp={DEVICE.currency_exponent}
          onOpenTab={openVisibleTab}
          onSendPrompt={sendPrompt}
        />
      );
    }
    return <div className="oa-embedded-tab">{renderDataTab()}</div>;
  };

  const renderWorkspace = () => {
    if (activeSpace === "operations") {
      return (
        <div className="oa-space-layout">
          <OfficeAISecondaryNav
            label={t("operations")}
            groups={operationNavGroups}
            tabs={operationTabs}
            activeTab={activeOperationTab}
            onSelect={setTab}
          />
          <div className="oa-space-content oa-embedded-tab oa-embedded-tab-flat">{renderDataTab()}</div>
        </div>
      );
    }
    if (activeSpace === "control") {
      return (
        <div className="oa-space-layout">
          <OfficeAISecondaryNav
            label={t("control")}
            groups={controlNavGroups}
            tabs={controlTabs}
            activeTab={activeControlTab}
            onSelect={setTab}
          />
          <div className="oa-space-content">{renderWorkspaceContent()}</div>
        </div>
      );
    }
    return renderWorkspaceContent();
  };

  return (
    <div className="oa-page">
      <OfficeAIPrimaryNav
        activeSpace={activeSpace}
        collapsed={collapsed}
        storeName={storeName}
        entries={primaryEntries}
        onSelectSpace={handleSelectSpace}
        onToggleCollapsed={() => setCollapsed(c => !c)}
        onBackToPOS={onBackToPOS}
      />

      <OfficeAIShell
        activeTab={tab}
        activeSpace={activeSpace}
        title={activeItem?.label ?? tab}
        subtitle={activeItem?.description ?? t("zanAiCommandCenter")}
        dockOpen={dockOpen}
        onToggleDock={() => setDockOpen(d => !d)}
        onOpenCommand={() => setCommandOpen(true)}
        onRefresh={tab === "overview" ? refreshOverview : undefined}
        refreshing={overviewLoading}
        pulseItems={isManager ? pulseItems : pulseItems.filter(p => p.id !== "pending")}
      >
        {(ctrl.errorMessage || diagResult) && (
          <div className={`oa-error-banner${diagResult?.ok ? " oa-error-banner-ok" : ""}`}>
            <span className="oa-error-banner-text">{diagResult ? diagResult.note : ctrl.errorMessage}</span>
            <div className="oa-error-banner-actions">
              {ctrl.errorMessage && !diagResult && (
                <button className="oa-error-banner-fix" onClick={handleRunDiagnostics} disabled={diagRunning}>
                  {t(diagRunning ? "fixingEllipsis" : "autoFix")}
                </button>
              )}
              <button className="oa-error-banner-close" onClick={() => { ctrl.dismissError(); setDiagResult(null); }} title={t("dismiss")} aria-label={t("dismiss")}><X size={14} /></button>
            </div>
          </div>
        )}
        {renderWorkspace()}
      </OfficeAIShell>

      {dockOpen && tab !== "assistant" && (
        <div role="presentation"  className="oa-copilot-backdrop" onMouseDown={() => setDockOpen(false)}>
          <div role="presentation"  className="oa-copilot-sheet" onMouseDown={event => event.stopPropagation()}>
            <CopilotDock
              ctrl={ctrl}
              userName={sessionUser.display_name}
              businessName={storeName}
              providerLabel={providerLabel}
              configured={configured}
              activeWorkspace={activeItem?.label ?? tab}
              onSetup={() => setTab("assistant")}
              onExpand={() => setTab("assistant")}
              onClose={() => setDockOpen(false)}
              composerRef={composerRef}
            />
          </div>
        </div>
      )}

      {commandOpen && (
        <OfficeAICommandPalette items={paletteItems} onSelect={openVisibleTab} onDismiss={() => setCommandOpen(false)} />
      )}
      {ctrl.chatState === "confirm" && isManager && (
        <ConfirmActionModal
          preview={ctrl.pendingAction?.preview}
          previews={ctrl.pendingBatchActions ?? undefined}
          expiresAt={ctrl.pendingAction?.expires_at}
          onConfirm={ctrl.handleConfirm}
          onCancel={ctrl.handleCancel}
        />
      )}
    </div>
  );
}
