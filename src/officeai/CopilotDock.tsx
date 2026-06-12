import { Maximize2, Sparkles, X } from "lucide-react";
import type { ChatController } from "./useChatController";
import ChatPanel from "./ChatPanel";

interface Props {
  ctrl: ChatController;
  userName: string;
  businessName: string;
  providerLabel: string;
  /** false until an AI provider is configured — dock then shows a setup CTA. */
  configured: boolean;
  onSetup: () => void;
  onExpand: () => void;
  onClose: () => void;
  composerRef: React.RefObject<HTMLTextAreaElement | null>;
}

/**
 * Right-side copilot dock. Pure chrome around ChatPanel — all chat state
 * lives in the controller owned by OfficeAIPage, so collapsing the dock or
 * expanding to the Assistant tab never interrupts an in-flight stream.
 */
export default function CopilotDock({
  ctrl, userName, businessName, providerLabel, configured, onSetup, onExpand, onClose, composerRef,
}: Props) {
  return (
    <aside className="oa-dock" aria-label="ZanAI copilot">
      <div className="oa-dock-header">
        <span className="oa-dock-brand">
          <Sparkles size={15} strokeWidth={1.75} aria-hidden="true" />
          ZanAI
        </span>
        {providerLabel && <span className="oa-dock-provider">{providerLabel}</span>}
        <div className="oa-dock-actions">
          <button className="oa-dock-btn" onClick={onExpand} title="Open fullscreen Assistant">
            <Maximize2 size={14} strokeWidth={1.75} aria-hidden="true" />
          </button>
          <button className="oa-dock-btn" onClick={onClose} title="Collapse copilot (Ctrl+/)">
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
          <p>ZanAI needs an AI provider before it can help.</p>
          <button className="btn-primary" onClick={onSetup}>Set up AI provider</button>
        </div>
      )}
    </aside>
  );
}
