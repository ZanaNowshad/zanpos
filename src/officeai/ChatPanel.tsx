import { useEffect, useRef, type ChangeEvent, type RefObject, type KeyboardEvent } from "react";
import type { ChatController } from "./useChatController";
import { ChatMessageList, QuickChipsBar } from "./ChatMessages";
import { LiveActivityBar } from "./toolCards";
import RunPanel from "./RunPanel";

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
const IMAGE_CHIPS = [
  { icon: "🧾", label: "Receipt → PO", prompt: "Create a purchase order from this delivery note. Extract the supplier name, products, quantities, and unit costs." },
  { icon: "🔍", label: "Scan product",  prompt: "Identify this product and show me its current stock, price, and recent sales." },
  { icon: "📸", label: "Audit shelf",   prompt: "What products are visible on this shelf? Which look low or out of stock?" },
];

export default function ChatPanel({ ctrl, variant, userName, businessName, composerRef }: Props) {
  const fileInputRef = useRef<HTMLInputElement | null>(null);

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

  const handleFileChange = (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = (ev) => {
      const dataUrl = ev.target?.result as string;
      const [header, base64] = dataUrl.split(",");
      const mediaType = header.split(":")[1]?.split(";")[0] ?? "image/jpeg";
      ctrl.setImageAttachment({ base64, mediaType, previewUrl: dataUrl });
    };
    reader.readAsDataURL(file);
    e.target.value = "";
  };

  const canSend = (!!ctrl.input.trim() || !!ctrl.imageAttachment) && ctrl.chatState === "idle";

  const handleImageChip = (prompt: string) => {
    ctrl.setInput(prompt);
    fileInputRef.current?.click();
  };

  const handleExport = () => {
    const lines: string[] = [`ZanAI Conversation — ${new Date().toLocaleString()}\n`];
    for (const msg of ctrl.messages) {
      const label = msg.role === "user" ? userName : msg.role === "assistant" ? "ZanAI" : "System";
      const time = msg.timestamp.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
      lines.push(`[${time}] ${label}:\n${msg.text}\n`);
    }
    const blob = new Blob([lines.join("\n")], { type: "text/plain" });
    const url  = URL.createObjectURL(blob);
    const a    = document.createElement("a");
    a.href     = url;
    a.download = `zanai-${new Date().toISOString().slice(0, 10)}.txt`;
    a.click();
    URL.revokeObjectURL(url);
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

      {ctrl.runState && (
        <RunPanel
          runState={ctrl.runState}
          onExecute={ctrl.handleRunExecute}
          onCancel={ctrl.handleRunCancel}
          onUndo={ctrl.handleRunUndo}
        />
      )}

      <div className="chat-footer-area">
        {ctrl.messages.length === 0 && ctrl.chatState === "idle" && variant === "full" && (
          <>
            <QuickChipsBar onSelect={(text) => ctrl.handleSend(text)} />
            <div className="chat-img-chips-bar">
              {IMAGE_CHIPS.map(c => (
                <button key={c.label} className="chat-img-chip" onClick={() => handleImageChip(c.prompt)}>
                  {c.icon} {c.label}
                </button>
              ))}
            </div>
          </>
        )}

        {ctrl.imageAttachment && (
          <div className="chat-img-preview-wrap">
            <img src={ctrl.imageAttachment.previewUrl} className="chat-img-preview-thumb" alt="Attached" />
            <button
              className="chat-img-preview-close"
              onClick={() => ctrl.setImageAttachment(null)}
              title="Remove image"
            >
              ×
            </button>
          </div>
        )}

        <div className="chat-input-row">
          <input
            ref={fileInputRef}
            type="file"
            accept="image/jpeg,image/png,image/webp,image/gif"
            className="chat-file-input-hidden"
            onChange={handleFileChange}
          />
          <button
            className="chat-img-btn"
            onClick={() => fileInputRef.current?.click()}
            disabled={ctrl.chatState !== "idle"}
            title="Attach image for AI analysis"
          >
            📷
          </button>
          <textarea
            ref={composerRef}
            className="chat-input-v2"
            placeholder={
              ctrl.chatState === "confirm" ? "Confirm or cancel the action above…" :
              ctrl.chatState === "run_confirm" ? "Confirm the bulk run above, or cancel…" :
              ctrl.chatState === "run_executing" ? "Run in progress…" :
              ctrl.imageAttachment ? "Ask about this image…" :
              "Ask ZanAI anything about your business…"
            }
            value={ctrl.input}
            onChange={e => ctrl.setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            disabled={ctrl.chatState !== "idle"}
            rows={1}
          />
          <button
            className="chat-send-btn-v2"
            onClick={() => ctrl.handleSend()}
            disabled={!canSend}
          >
            ↑
          </button>
        </div>
        <div className="chat-input-hint">
          Enter to send · Shift+Enter for new line
          {ctrl.messages.length > 0 && (
            <>
              <button className="chat-clear-link" onClick={ctrl.handleClearChat}>· Clear chat</button>
              <button className="chat-clear-link" onClick={handleExport}>· Export</button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
