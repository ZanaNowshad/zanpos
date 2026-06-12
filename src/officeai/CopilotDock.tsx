import { useRef } from "react";
import { Bot } from "lucide-react";

interface Props {
  ctrl: ChatController;
  kpi: KpiSnapshot;
  open: boolean;
  onToggle: () => void;
  onExpand: () => void;
  businessName: string;
}

export default function CopilotDock({ ctrl, open, onToggle, onExpand, businessName }: Props) {
  const composerRef = useRef<HTMLTextAreaElement>(null);

  if (!open) return null;

  return (
    <div className="oa-dock">
      <div className="oa-dock-header">
        <span className="oa-dock-brand"><Bot size={16} /> ZanAI</span>
        <span className="oa-dock-subtitle">{businessName}</span>
        <div className="oa-dock-actions">
          <button className="oa-dock-btn" onClick={onExpand} title="Fullscreen">⛶</button>
          <button className="oa-dock-btn" onClick={onToggle} title="Close">×</button>
        </div>
      </div>
      <div className="oa-dock-messages">
        {/* Messages rendered via ChatPanel docked variant */}
      </div>
      <div className="oa-dock-composer">
        <textarea
          ref={composerRef}
          className="oa-composer"
          placeholder="Ask ZanAI…"
          value={ctrl.input}
          onChange={e => ctrl.setInput(e.target.value)}
          onKeyDown={e => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              ctrl.handleSend();
            }
          }}
          rows={1}
        />
        <button className="oa-send-btn" onClick={() => ctrl.handleSend()} disabled={ctrl.chatState !== "idle"}>↑</button>
      </div>
    </div>
  );
}
