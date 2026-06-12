import type { ChatController } from "./useChatController";
import { ChatMessageList, QuickChipsBar } from "./ChatMessages";

interface Props { ctrl: ChatController; variant: "docked" | "full"; composerRef?: React.RefObject<HTMLTextAreaElement | null>; }

export default function ChatPanel({ ctrl, variant, composerRef }: Props) {
  const isFull = variant === "full";
  return (
    <div className={`chat-panel ${isFull ? "chat-panel-full" : "chat-panel-docked"}`}>
      <ChatMessageList
        messages={ctrl.messages} streamingMsgId={null} liveToolCalls={ctrl.liveToolCalls}
        onUndo={ctrl.handleUndo} onChip={(text) => ctrl.handleSend(text)}
        userName="" kpi={ctrl.kpi} currencyExp={3}
      />
      <div className="chat-footer-area">
        {ctrl.messages.length === 0 && ctrl.chatState === "idle" && <QuickChipsBar onSelect={(text) => ctrl.handleSend(text)} />}
        <div className="chat-input-row">
          <textarea ref={composerRef} className="chat-input-v2" placeholder={ctrl.chatState === "confirm" ? "Confirm or cancel the action above…" : "Ask anything about your business…"}
            value={ctrl.input} onChange={e => ctrl.setInput(e.target.value)}
            onKeyDown={e => { if (e.key === "Enter" && !e.shiftKey && ctrl.chatState === "idle") { e.preventDefault(); ctrl.handleSend(); } }}
            disabled={ctrl.chatState !== "idle"} rows={1} />
          <button className="chat-send-btn-v2" onClick={() => ctrl.handleSend()} disabled={!ctrl.input.trim() || ctrl.chatState !== "idle"}>↑</button>
        </div>
        <div className="chat-input-hint">Enter to send · Shift+Enter for new line
          {ctrl.messages.length > 0 && <button className="chat-clear-link" onClick={ctrl.handleClear}>· Clear chat</button>}
        </div>
      </div>
    </div>
  );
}
