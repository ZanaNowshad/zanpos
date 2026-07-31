import { Activity, Bot, ClipboardList, ImagePlus, Settings, ShieldCheck, X } from "lucide-react";
import { useState } from "react";
import type { RefObject } from "react";
import { DEVICE, type ProviderConfig, type SessionUser } from "../types";
import ChatPanel from "./ChatPanel";
import { KpiSidebar } from "./KpiSidebar";
import ProviderSetup from "./ProviderSetup";
import { useLanguage } from "../hooks/useLanguage";
import { officeAiTranslator } from "../i18n/officeAiStrings";
import type { ChatController } from "./useChatController";

interface Props {
  ctrl: ChatController;
  sessionUser: SessionUser;
  storeName: string;
  configured: boolean;
  configLoaded: boolean;
  composerRef: RefObject<HTMLTextAreaElement | null>;
  onProviderConfigured: (cfg: ProviderConfig) => void;
  onBackFromSetup: () => void;
  onOpenHealth: () => void;
  onOpenActions: () => void;
  canUseManagerTools: boolean;
}

export default function OfficeAIAssistantWorkspace({
  ctrl,
  sessionUser,
  storeName,
  configured,
  configLoaded,
  composerRef,
  onProviderConfigured,
  onBackFromSetup,
  onOpenHealth,
  onOpenActions,
  canUseManagerTools,
}: Props) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  const [contextOpen, setContextOpen] = useState(false);

  if (!configLoaded) {
    return <div className="oa-empty-state oa-empty-large">{t("aiProviderLoading")}</div>;
  }

  if (!configured) {
    return (
      <ProviderSetup
        sessionToken={sessionUser.session_token}
        onDone={(cfg) => {
          onProviderConfigured(cfg);
          void ctrl.fetchKpi();
        }}
        onBack={onBackFromSetup}
      />
    );
  }

  return (
    <div className="oa-assistant-workspace">
      <div className="oa-assistant-toolbar">
        <div className="oa-assistant-safety">
          <Bot size={15} />
          <span>{t("managerApprovalReminder")}</span>
        </div>
        <button
          className="oa-tool-btn"
          onClick={() => setContextOpen(true)}
          aria-expanded={contextOpen}
          aria-controls="oa-business-context"
        >
          <Activity size={15} />
          <span>{t("businessContext")}</span>
        </button>
      </div>
      <div className="oa-assistant-main">
        <ChatPanel
          ctrl={ctrl}
          variant="full"
          userName={sessionUser.display_name}
          businessName={storeName}
          composerRef={composerRef}
        />
      </div>
      {contextOpen && (
        <div className="oa-sheet-backdrop" onMouseDown={() => setContextOpen(false)}>
          <aside
            id="oa-business-context"
            className="oa-context-sheet"
            aria-label={t("businessContext")}
            onMouseDown={(event) => event.stopPropagation()}
          >
            <div className="oa-sheet-header">
              <div>
                <strong>{t("businessContext")}</strong>
                <span>{t("businessContextHint")}</span>
              </div>
              <button className="oa-icon-btn" onClick={() => setContextOpen(false)} aria-label={t("closeBusinessContext")}>
                <X size={16} />
              </button>
            </div>
            <div className="oa-context-sheet-scroll">
              <div className="oa-context-list">
                {canUseManagerTools && (
                  <>
                    <button onClick={onOpenActions}><ClipboardList size={15} /> {t("reviewAiActions")}</button>
                    <button onClick={onOpenHealth}><ShieldCheck size={15} /> {t("runSystemHealth")}</button>
                  </>
                )}
                <button onClick={() => ctrl.setInput("Analyze the latest WhatsApp purchase bill or payment proof and tell me what needs review.")}>
                  <ImagePlus size={15} /> Prepare image workflow
                </button>
                <button onClick={() => ctrl.setInput("Show me AI settings that affect tool execution, timeouts, and inventory operations.")}>
                  <Settings size={15} /> Inspect AI settings
                </button>
              </div>
              <div className="oa-context-note">
                <Bot size={15} />
                <span>{t("aiMutationNotice")}</span>
              </div>
              <KpiSidebar
                kpi={ctrl.kpi}
                currencyExp={DEVICE.currency_exponent}
                onRefresh={ctrl.fetchKpi}
                onDismissAlert={ctrl.dismissAlert}
              />
            </div>
          </aside>
        </div>
      )}
    </div>
  );
}
