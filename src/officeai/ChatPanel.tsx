import { useEffect, useRef, type ChangeEvent, type RefObject, type KeyboardEvent } from "react";
import { Camera, ImagePlus, ReceiptText, Search, SendHorizontal, Square, X } from "lucide-react";
import type { ChatController } from "./useChatController";
import { ChatMessageList, QuickChipsBar } from "./ChatMessages";
import { LiveActivityBar } from "./toolCards";
import RunPanel from "./RunPanel";
import { useLanguage } from "../hooks/useLanguage";
import { officeAiTranslator, type OfficeAiStringKey } from "../i18n/officeAiStrings";

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
  { Icon: ReceiptText, labelKey: "purchaseBill", prompt: "Create a purchase order from this delivery note. Extract the supplier name, products, quantities, and unit costs." },
  { Icon: Search, labelKey: "scanProduct", prompt: "Identify this product and show me its current stock, price, and recent sales." },
  { Icon: ImagePlus, labelKey: "shelfAudit", prompt: "What products are visible on this shelf? Which look low or out of stock?" },
] satisfies Array<{ Icon: typeof ReceiptText; labelKey: OfficeAiStringKey; prompt: string }>;

export default function ChatPanel({ ctrl, variant, userName, businessName, composerRef }: Props) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
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
    const lines: string[] = [`${t("zanAiConversation")} — ${new Date().toLocaleString()}\n`];
    for (const msg of ctrl.messages) {
      const label = msg.role === "user" ? userName : msg.role === "assistant" ? "ZanAI" : t("systemRole");
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
        onFeedback={ctrl.handleFeedback}
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

      {ctrl.bulkProgress && (
        <div className="chat-bulk-progress" role="status" aria-live="polite">
          <span className="chat-bulk-progress-label">
            {ctrl.bulkProgress.tool === "create_products"
              ? t("creatingProducts")
              : ctrl.bulkProgress.tool === "bulk_import_products"
                ? t("importingProducts")
                : t("processing")}
            {" "}{ctrl.bulkProgress.done} / {ctrl.bulkProgress.total}
          </span>
          <div className="chat-bulk-progress-track">
            <div
              className="chat-bulk-progress-fill"
              style={{
                width: `${Math.min(100, Math.round((ctrl.bulkProgress.done / Math.max(1, ctrl.bulkProgress.total)) * 100))}%`,
              }}
            />
          </div>
        </div>
      )}

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
            <button key={c.labelKey} className="chat-img-chip" onClick={() => handleImageChip(c.prompt)}>
                  <c.Icon size={14} /> {t(c.labelKey)}
                </button>
              ))}
            </div>
          </>
        )}

        {ctrl.chatState === "idle" &&
          ctrl.messages.length > 0 &&
          ctrl.messages[ctrl.messages.length - 1]?.role === "assistant" && (
          <div className="chat-continue-bar">
            <button
              className="chat-continue-chip"
              onClick={() => ctrl.handleSend("Continue — pick up exactly where you left off and finish the remaining work.")}
              title={t("continueTask")}
            >
              {t("continueAction")} <span className="icon-directional" aria-hidden="true">▸</span>
            </button>
          </div>
        )}

        {ctrl.imageAttachment && (
          <div className="chat-img-preview-wrap">
            <img src={ctrl.imageAttachment.previewUrl} className="chat-img-preview-thumb" alt={t("attached")} />
            <button
              className="chat-img-preview-close"
              onClick={() => ctrl.setImageAttachment(null)}
              title={t("removeImage")}
            >
              <X size={12} />
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
            title={t("attachImage")}
          >
            <Camera size={17} />
          </button>
          <textarea
            ref={composerRef}
            className="chat-input-v2"
            placeholder={
              ctrl.chatState === "confirm" ? t("confirmActionPlaceholder") :
              ctrl.chatState === "run_confirm" ? t("confirmRunPlaceholder") :
              ctrl.chatState === "run_executing" ? t("runInProgress") :
              ctrl.imageAttachment ? t("askAboutImage") :
              t("askAnything")
            }
            value={ctrl.input}
            onChange={e => ctrl.setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            disabled={ctrl.chatState !== "idle"}
            rows={1}
          />
          <button
            className="chat-send-btn-v2"
            onClick={() => ctrl.chatState === "thinking" && ctrl.canStop ? ctrl.handleStop() : ctrl.handleSend()}
            disabled={ctrl.chatState === "thinking" ? !ctrl.canStop : !canSend}
            title={ctrl.chatState === "thinking" ? (ctrl.canStop ? t("stopResponse") : t("startingResponse")) : t("sendMessage")}
          >
            {ctrl.chatState === "thinking" && ctrl.canStop
              ? <Square size={15} fill="currentColor" />
              : <SendHorizontal className="icon-directional" size={17} />}
          </button>
        </div>
        <div className="chat-input-hint">
          {t("sendHint")}
          {ctrl.messages.length > 0 && (
            <>
              <button className="chat-clear-link" onClick={ctrl.handleClearChat}>· {t("clearChat")}</button>
              <button className="chat-clear-link" onClick={handleExport}>· {t("export")}</button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
