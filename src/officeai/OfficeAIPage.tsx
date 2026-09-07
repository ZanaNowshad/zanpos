import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { AiHandoff, DiagnosticReport, ProductPrefill, ProviderConfig, SessionUser } from "../types";
import { DEVICE } from "../types";
import { adminGetProviderConfig, adminRunDiagnostics, settingsGetBranch } from "../tauri/commands";
import type { OfficeAiOverviewSnapshot, OfficeTab } from "./officeAiTypes";
import { officeAiActionQueue, officeAiOverview } from "./officeAiData";
import { useLanguage } from "../hooks/useLanguage";
import { useTheme } from "../hooks/useTheme";
import { officeAiTranslator } from "../i18n/officeAiStrings";
import { buildOfficePagePulse, EMPTY_OFFICE_OVERVIEW } from "./officeAiPagePresentation";
import { useZanAi } from "../zanai/useZanAi";
// Phase 4: new Today dashboard replaces OfficeAIOverview
import TodayDashboard from "../command/pages/TodayDashboard";
import OfficeAICommandPalette from "./OfficeAICommandPalette";
import type { OfficePulseItem } from "./officeAiTypes";
// Canonical navigation — the single source of truth (src/navigation/config).
import {
  domainsForRole,
  entryTabForDomain,
  paletteEntriesForRole,
  visibleTabsForRole,
} from "../navigation/config";
import SectionNav from "../navigation/SectionNav";
import { PageHeadingProvider } from "../navigation/PageHeadingContext";
import { useActiveNavigation } from "./useActiveNavigation";
import CommandShell from "../command/CommandShell";
import type { CommandDomain } from "../command/CommandSidebar";
import type { HeaderStatusPill } from "../command/CommandHeader";
import OfficeAIAssistantWorkspace from "./OfficeAIAssistantWorkspace";
import ReviewWorkspace from "../command/pages/review/ReviewWorkspace";
import OfficeAISystemHealth from "./OfficeAISystemHealth";
import OfficeAIWorkflowInbox from "./OfficeAIWorkflowInbox";
import OfficeAIConflictInbox from "./OfficeAIConflictInbox";
import OfficeAIGrowthWorkspace from "./OfficeAIGrowthWorkspace";
import CopilotDock from "./CopilotDock";
import ConfirmActionModal from "../components/ConfirmActionModal";
import { DegradedBanner } from "../components/templates";

// Customers: directory + loyalty are one workspace, replacing CustomersTab and
// the Growth workspace's loyalty mode.
import LoyaltyPage from "../command/pages/customers/LoyaltyPage";
// Phase 7: consolidated settings replaces SettingsTab
import { renderDataTab } from "./officeAiTabRouter";
import { usePersistedAiActions } from "./usePersistedAiActions";
import { useOfficeAiShortcuts } from "./useOfficeAiShortcuts";


interface Props {
  sessionUser: SessionUser; onBackToPOS: () => void;
  initialProductPrefill?: ProductPrefill | null; initialAiMessage?: AiHandoff | null;
  initialTab?: OfficeTab; initialMaintenancePane?: boolean;
  onStartPractice?: () => void;
}

export default function OfficeAIPage({
  sessionUser,
  onBackToPOS,
  initialProductPrefill,
  initialAiMessage,
  initialTab,
  initialMaintenancePane,
  onStartPractice,
}: Props) {
  const { language, toggle: toggleLanguage } = useLanguage();
  const { theme, toggle: toggleTheme } = useTheme();
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
  const [productPrefill, setProductPrefill] = useState<ProductPrefill | null>(initialProductPrefill ?? null);
  const [diagRunning, setDiagRunning] = useState(false);
  const [diagResult, setDiagResult] = useState<DiagnosticReport | null>(null);
  const composerRef = useRef<HTMLTextAreaElement>(null);
  const {
    ctrl,
    navigationRequest,
    consumeNavigationRequest,
    registerSurfaceContext,
    dataEpoch,
  } = useZanAi();

  const isOwner = sessionUser.role_name === "owner";
  const isManager = isOwner || sessionUser.role_name === "manager";
  const configured = Boolean(config?.provider);

  useEffect(() => {
    settingsGetBranch(sessionUser.session_token).then(b => { if (b?.name) setBusinessName(b.name); }).catch(() => {});
    adminGetProviderConfig(sessionUser.session_token)
      .then(cfg => setConfig(cfg))
      .catch(() => setConfig(null))
      .finally(() => setConfigLoaded(true));
  }, [sessionUser.user_id, sessionUser.session_token]);

  const refreshOverview = useCallback(async () => {
    setOverview(prev => ({ ...prev, loading: true }));
    try {
      const next = await officeAiOverview(sessionUser.session_token, sessionUser.branch_id, t);
      setOverview(next);
      if (next.provider) setConfig(next.provider);
    } catch (e) {
      setOverview(prev => ({
        ...prev,
        loading: false,
        errors: [`${t("officeAiOverview")}: ${typeof e === "string" ? e : String(e)}`],
      }));
    }
  }, [sessionUser.branch_id, sessionUser.session_token, t]);

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
    () => `The admin is in the ZANPOS Command center, currently viewing the "${tabRef.current}" workspace.`,
    [],
  );
  const openVisibleTab = useCallback((nextTab: OfficeTab) => {
    if ((visibleTabsForRole(sessionUser.role_name) as string[]).includes(nextTab)) {
      setTab(nextTab);
    }
  }, [sessionUser.role_name]);
  useEffect(() => {
    registerSurfaceContext({ surface: "office", summary: getUiContext() });
  }, [getUiContext, registerSurfaceContext, tab]);

  useEffect(() => {
    if (!navigationRequest) return;
    if ((visibleTabsForRole(sessionUser.role_name) as string[]).includes(navigationRequest.tab)) {
      setTab(navigationRequest.tab as OfficeTab);
    }
    consumeNavigationRequest(navigationRequest.id);
  }, [consumeNavigationRequest, navigationRequest, sessionUser.role_name]);

  const observedDataEpochRef = useRef(dataEpoch);
  useEffect(() => {
    if (observedDataEpochRef.current === dataEpoch) return;
    observedDataEpochRef.current = dataEpoch;
    setTabEpoch(epoch => epoch + 1);
    void refreshOverview();
  }, [dataEpoch, refreshOverview]);

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

  /** Rail domains for this role, sections already permission-filtered. */
  const navDomains = useMemo(
    () => domainsForRole(sessionUser.role_name),
    [sessionUser.role_name],
  );
  /** Palette entries are derived from the nav config — never hand-listed. */
  const paletteItems = useMemo(
    () => paletteEntriesForRole(sessionUser.role_name).map(entry => ({
      id: entry.id,
      label: t(entry.labelKey),
      description: t(entry.domainLabelKey),
    })),
    [sessionUser.role_name, t],
  );

  const chatStateRef = useRef(ctrl.chatState);
  useEffect(() => { chatStateRef.current = ctrl.chatState; }, [ctrl.chatState]);
  useOfficeAiShortcuts({
    chatStateRef, commandOpen, dockOpen, navDomains,
    setCommandOpen, setDockOpen, onOpenTab: openVisibleTab, onBackToPOS,
    focusComposer: () => composerRef.current?.focus(),
  });

  // ── Canonical navigation state ────────────────────────────────────────────
  // One model: rail domain (L1) + contextual sections (L2). Both derive from
  // src/navigation/config, so they can never disagree about what is active.
  const { confirmPersistedAction, cancelPersistedAction } = usePersistedAiActions({
    sessionToken: sessionUser.session_token,
    onApplied: () => { setTabEpoch(e => e + 1); void refreshOverview(); },
  });

  const { activeDomainId, activeSections, activeDomain, activeSectionTab, activeSectionLabel } =
    useActiveNavigation(tab, sessionUser.role_name, navDomains, t);

  const commandDomains = useMemo<CommandDomain[]>(
    () => navDomains.map(d => ({
      id: d.id,
      labelKey: d.labelKey,
      icon: d.icon,
      defaultTab: d.defaultTab,
    })),
    [navDomains],
  );
  const activeItem = paletteItems.find(item => item.id === tab)
    ?? { id: tab, label: activeDomain ? t(activeDomain.labelKey) : tab, description: "ZanAI" };
  const storeName = businessName || t("yourStore");
  const tabKey = `${tab}:${tabEpoch}`;
  const actionItems = officeAiActionQueue(ctrl, t);

  const pulseItems: OfficePulseItem[] = useMemo(
    () => buildOfficePagePulse({ overview, actionItems, configured, isManager, t }),
    [overview, actionItems, configured, isManager, t],
  );

  const headerStatuses = useMemo<HeaderStatusPill[]>(() =>
    pulseItems.map(p => ({
      id: p.id,
      label: p.label,
      level: p.level,
      icon: p.icon,
    })), [pulseItems]);


  const renderWorkspaceContent = () => {
    if (tab === "overview") {
      return (
        <TodayDashboard
          snapshot={overview}
          storeName={businessName}
          currencyExp={DEVICE.currency_exponent}
          canUseManagerTools={isManager}
          pendingActionCount={actionItems.filter(item => item.status === "pending").length}
          onOpenTab={openVisibleTab}
          onRefresh={refreshOverview}
          onSendPrompt={sendPrompt}
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
      // Persisted actions are the authority here, not chat state: a manager in
      // a fresh session must still see everything awaiting a decision.
      return (
        <ReviewWorkspace
          sessionToken={sessionUser.session_token}
          canApprove={isManager}
          conflictsSlot={<OfficeAIConflictInbox sessionToken={sessionUser.session_token} />}
          onConfirm={confirmPersistedAction}
          onCancelAction={cancelPersistedAction}
        />
      );
    }
    if (tab === "health") {
      return <OfficeAISystemHealth sessionToken={sessionUser.session_token} initialReport={overview.health} onReport={(health) => setOverview(prev => ({ ...prev, health }))} />;
    }
    if (tab === "conflicts") {
      return <OfficeAIConflictInbox sessionToken={sessionUser.session_token} />;
    }
    if (tab === "workflows") {
      return <OfficeAIWorkflowInbox sessionToken={sessionUser.session_token} onSendPrompt={sendPrompt} />;
    }
    if (tab === "loyalty") {
      return (
        <LoyaltyPage
          key={tabKey}
          sessionToken={sessionUser.session_token}
          onOpenDirectory={() => openVisibleTab("customers")}
          onOpenSettings={isManager ? () => openVisibleTab("settings") : undefined}
        />
      );
    }
    if (tab === "insights") {
      return (
        <OfficeAIGrowthWorkspace
          mode="insights"
          sessionToken={sessionUser.session_token}
          snapshot={overview}
          currencyExp={DEVICE.currency_exponent}
          onOpenTab={openVisibleTab}
          onSendPrompt={sendPrompt}
        />
      );
    }
    // No wrapper: pages render their own PageTemplate, which already provides
    // the content panel. Wrapping here produced a bordered card inside a
    // bordered card and stopped the page claiming the available height.
    return renderDataTab({
      tab, tabKey, sessionUser, isOwner, isManager,
      productPrefill,
      maintenancePane: initialMaintenancePane,
      onPrefillConsumed: () => setProductPrefill(null),
      onOpenTab: openVisibleTab,
      onSendPrompt: sendPrompt,
      onStartPractice,
    });
  };

  /**
   * One layout for every domain: contextual L2 nav (when the domain has two
   * or more sections) beside the workspace. No per-space special-casing.
   */
  const renderWorkspace = () => {
    // Settings brings its own group rail, which is that page's level-2
    // navigation. Rendering the domain section nav as well would stack three
    // navigation columns — the exact nesting this shell exists to remove.
    const ownsItsNav = tab === "settings";
    const hasSections = activeSections.length >= 2 && !ownsItsNav;
    return (
      <div className={`zp-domain-layout${hasSections ? "" : " zp-domain-layout-flat"}`}>
        {hasSections && activeDomain && (
          <SectionNav
            domainLabelKey={activeDomain.labelKey}
            sections={activeSections}
            activeTab={activeSectionTab}
            onSelect={setTab}
          />
        )}
        <div className="zp-domain-content">
          <PageHeadingProvider
            value={{
              domain: activeDomain ? t(activeDomain.labelKey) : null,
              section: hasSections ? activeSectionLabel : null,
            }}
          >
            {renderWorkspaceContent()}
          </PageHeadingProvider>
        </div>
      </div>
    );
  };

  return (
    <>
      <CommandShell
        activeDomainId={activeDomainId}
        collapsed={collapsed}
        hasSectionNav={activeSections.length >= 2 && tab !== "settings"}
        storeName={storeName}
        roleName={sessionUser.role_name}
        domains={commandDomains}
        onSelectDomain={(d) => {
          const domain = navDomains.find(n => n.id === d.id);
          setTab(domain ? entryTabForDomain(domain) : (d.defaultTab as OfficeTab));
        }}
        onToggleCollapsed={() => setCollapsed(c => !c)}
        onBackToPOS={onBackToPOS}
        notificationCount={actionItems.filter(i => i.status === "pending").length}
        onOpenAskBar={() => setCommandOpen(true)}
        onToggleDock={() => setDockOpen(d => !d)}
        dockOpen={dockOpen}
        onOpenNotifications={() => openVisibleTab("workflows")}
        statusPills={headerStatuses}
        userName={sessionUser.display_name}
        userRole={sessionUser.role_name}
        themeLabel={theme === "light" ? "☀" : "☾"}
        onToggleTheme={toggleTheme ?? (() => {})}
        languageLabel={language === "ar" ? "EN" : "ع"}
        onToggleLanguage={toggleLanguage}
      >
        {(ctrl.errorMessage || diagResult) && (
          <DegradedBanner
            severity={diagResult?.ok ? "ok" : "warning"}
            message={diagResult ? diagResult.note : ctrl.errorMessage!}
            action={
              ctrl.errorMessage && !diagResult
                ? { label: t(diagRunning ? "fixingEllipsis" : "autoFix"), onClick: handleRunDiagnostics }
                : undefined
            }
            onDismiss={() => { ctrl.dismissError(); setDiagResult(null); }}
          />
        )}
        {renderWorkspace()}
      </CommandShell>

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
    </>
  );
}
