import { useEffect, type RefObject, type KeyboardEvent } from "react";
import type { ChatController } from "./useChatController";
import { ChatMessageList, QuickChipsBar } from "./ChatMessages";
import { LiveActivityBar } from "./toolCards";

interface Props {
  ctrl: ChatController;
  /** "full" = Assistant tab; "docked" = right-side copilot dock. */
  variant: "docked" | "full";
  userName: string;
  businessName: string;
  composerRef: RefObject<HTMLTextAreaElement | null>;
}

/**
 * The reusable chat surface. Both variants render the SAME controller
 * instance, so the conversation (and any in-flight stream) survives moving
 * between the dock and the fullscreen Assistant tab.
 */
export default function ChatPanel({ ctrl, variant, userName, businessName, composerRef }: Props) {
  // Auto-expand composer up to 160px
  useEffect(() => {
    const el = composerRef.current;
    if (el) {
      el.style.height = "auto";
      el.style.height = Math.min(el.scrollHeight, 160) + "px";
    }
  }, [ctrl.input, composerRef]);

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); ctrl.handleSend(); }
  };

  return (
    <div className={`chat-panel chat-panel-${variant}`}>
      <ChatMessageList
        messages={ctrl.messages}
        chatState={ctrl.chatState}
        streamingMsgId={ctrl.streamingMsgId}
        liveToolCalls={ctrl.liveToolCalls}
        onUndo={ctrl.handleUndo}
        onChip={(text) => ctrl.handleSend(text)}
        userName={userName}
        businessName={businessName}
      />

      <LiveActivityBar
        chatState={ctrl.chatState}
        liveToolCalls={ctrl.liveToolCalls}
        tokenCount={ctrl.tokenCount}
        streamStartTime={ctrl.streamStartTime}
      />

      <div className="chat-footer-area">
        {ctrl.messages.length === 0 && ctrl.chatState === "idle" && variant === "full" && (
          <QuickChipsBar onSelect={(text) => ctrl.handleSend(text)} />
        )}

        <div className="chat-input-row">
          <textarea
            ref={composerRef}
            className="chat-input-v2"
            placeholder={ctrl.chatState === "confirm" ? "Confirm or cancel the action above…" : "Ask ZanAI anything about your business…"}
            value={ctrl.input}
            onChange={e => ctrl.setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            disabled={ctrl.chatState !== "idle"}
            rows={1}
          />
          <button
            className="chat-send-btn-v2"
            onClick={() => ctrl.handleSend()}
            disabled={!ctrl.input.trim() || ctrl.chatState !== "idle"}
          >
            ↑
          </button>
        </div>
        <div className="chat-input-hint">
          Enter to send · Shift+Enter for new line
          {ctrl.messages.length > 0 && (
            <button className="chat-clear-link" onClick={ctrl.handleClearChat}>
              · Clear chat
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
