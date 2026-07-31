import type { RefObject } from "react";
import { Maximize2, Sparkles, X } from "lucide-react";
import type { ChatController } from "./useChatController";
import ChatPanel from "./ChatPanel";
import { useLanguage } from "../hooks/useLanguage";
import { officeAiTranslator } from "../i18n/officeAiStrings";

interface Props {
  ctrl: ChatController;
  userName: string;
  businessName: string;
  providerLabel: string;
  activeWorkspace?: string;
  /** false until an AI provider is configured — dock then shows a setup CTA. */
  configured: boolean;
  onSetup: () => void;
  onExpand: () => void;
  onClose: () => void;
  composerRef: RefObject<HTMLTextAreaElement | null>;
}

/**
 * Right-side copilot dock. Pure chrome around ChatPanel — all chat state
 * lives in the controller owned by OfficeAIPage, so collapsing the dock or
 * expanding to the Assistant tab never interrupts an in-flight stream.
 */
export default function CopilotDock({
  ctrl, userName, businessName, providerLabel, configured, onSetup, onExpand, onClose, composerRef,
  activeWorkspace,
}: Props) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  return (
    <aside className="oa-dock" aria-label={t("zanAiCopilot")}>
      <div className="oa-dock-header">
        <span className="oa-dock-brand">
          <Sparkles size={15} strokeWidth={1.75} aria-hidden="true" />
          ZanAI
        </span>
        <span className="oa-dock-provider">
          {activeWorkspace ? `${activeWorkspace} context` : providerLabel}
        </span>
        <div className="oa-dock-actions">
          <button className="oa-dock-btn" onClick={onExpand} title={t("openFullscreenAssistant")}>
            <Maximize2 size={14} strokeWidth={1.75} aria-hidden="true" />
          </button>
          <button className="oa-dock-btn" onClick={onClose} title={t("collapseCopilot")}>
            <X size={15} strokeWidth={1.75} aria-hidden="true" />
          </button>
        </div>
      </div>

      {configured ? (
        <ChatPanel
          ctrl={ctrl}
          variant="docked"
          userName={userName}
          businessName={businessName}
          composerRef={composerRef}
        />
      ) : (
        <div className="oa-dock-setup">
          <p>{t("providerNeeded")}</p>
          <button className="btn-primary" onClick={onSetup}>{t("setupAiProvider")}</button>
        </div>
      )}
    </aside>
  );
}
