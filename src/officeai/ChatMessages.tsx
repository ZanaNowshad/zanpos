import React, { useEffect, useRef, useState } from "react";
import type { ChatState, DisplayMessage, ToolCallEntry } from "./officeAiTypes";
import { QUICK_ACTIONS } from "./officeAiTypes";
import { MarkdownContent } from "./markdown";
import { ToolCallCard, toolMeta } from "./toolCards";
import { useLanguage } from "../hooks/useLanguage";
import {
  officeAiFormat,
  officeAiTranslator,
  type OfficeAiStringKey,
} from "../i18n/officeAiStrings";

// ─── Quick chips bar ──────────────────────────────────────────────────────────

export function QuickChipsBar({ onSelect }: { onSelect: (text: string) => void }) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  const labelKeys: OfficeAiStringKey[] = [
    "todaysSales", "lowStock", "cashDrawer", "topProducts",
    "recentRefunds", "shiftHistory", "auditChain", "syncStatus",
  ];
  return (
    <div className="quick-chips-bar">
      {QUICK_ACTIONS.map((a, index) => (
        <button key={a.prompt} className="quick-chip" onClick={() => onSelect(a.prompt)}>
          {t(labelKeys[index])}
        </button>
      ))}
    </div>
  );
}

// ─── Welcome panel ────────────────────────────────────────────────────────────

const WELCOME_CHIPS = [
  { icon: "📊", labelKey: "todaysSalesSummary", prompt: "Give me today's sales summary" },
  { icon: "📦", labelKey: "lowStockAlerts", prompt: "Which products are low on stock?" },
  { icon: "💵", labelKey: "cashDrawerStatus", prompt: "Show me the current cash drawer status" },
  { icon: "🏆", labelKey: "topProductsThisWeek", prompt: "What are the top selling products this week?" },
  { icon: "↩", labelKey: "recentRefunds", prompt: "Show me recent refunds" },
  { icon: "👥", labelKey: "todaysTransactions", prompt: "How many transactions were made today?" },
] satisfies Array<{ icon: string; labelKey: OfficeAiStringKey; prompt: string }>;

export function WelcomePanel({ userName, businessName, onChip }: {
  userName: string;
  businessName: string;
  onChip: (text: string) => void;
}) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  const hour = new Date().getHours();
  const greeting = hour < 12 ? t("goodMorning") : hour < 17 ? t("goodAfternoon") : t("goodEvening");
  return (
    <div className="chat-welcome-advanced">
      <div className="welcome-header">
        <div className="welcome-avatar">✦</div>
        <div>
          <div className="welcome-title">{greeting}, {userName} 👋</div>
          <div className="welcome-subtitle">{officeAiFormat(t("welcomeSubtitleNamed"), { business: businessName })}</div>
        </div>
      </div>
      <div className="welcome-suggested-label">{t("suggestedQuestions")}</div>
      <div className="welcome-suggested-grid">
        {WELCOME_CHIPS.map(s => (
          <button key={s.prompt} className="welcome-suggested-card" onClick={() => onChip(s.prompt)}>
            <span className="welcome-suggested-icon">{s.icon}</span>
            <span className="welcome-suggested-text">{t(s.labelKey)}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

// ─── Completed tool pill (collapsible, ChatGPT-style) ────────────────────────

function ToolPill({ entry }: { entry: ToolCallEntry }) {
  const { language } = useLanguage();
  const [open, setOpen] = useState(false);
  const meta = toolMeta(entry.name, language);
  return (
    <div className="tool-pill">
      <button className="tool-pill-header" onClick={() => setOpen(o => !o)}>
        <meta.Icon size={13} style={{ color: meta.color, flexShrink: 0 }} />
        <span className="tool-pill-name">{meta.label}</span>
        {entry.duration !== undefined && (
          <span className="tool-pill-time">{entry.duration}ms</span>
        )}
        <span className="tool-pill-chevron">{open ? "▴" : "▾"}</span>
      </button>
      {open && (
        <div className="tool-pill-details">
          <code className="tool-pill-raw">{entry.name}</code>
        </div>
      )}
    </div>
  );
}

// ─── Chat bubble ──────────────────────────────────────────────────────────────

export function ChatBubble({
  msg,
  onUndo,
  onFeedback,
  onSuggestedPrompt,
  isStreaming = false,
}: {
  msg: DisplayMessage;
  onUndo?: () => void;
  onFeedback?: (messageId: string, rating: "up" | "down", aiSessionId?: string) => Promise<void>;
  onSuggestedPrompt?: (prompt: string) => void;
  isStreaming?: boolean;
}) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  const [copied, setCopied] = useState(false);
  const [feedback, setFeedback] = useState<"up" | "down" | null>(null);
  const [feedbackPending, setFeedbackPending] = useState(false);
  const [feedbackError, setFeedbackError] = useState(false);

  const submitFeedback = async (rating: "up" | "down") => {
    if (feedbackPending) return;
    setFeedbackPending(true);
    setFeedbackError(false);
    try {
      await onFeedback?.(msg.id, rating, msg.aiSessionId);
      setFeedback(rating);
    } catch {
      setFeedbackError(true);
    } finally {
      setFeedbackPending(false);
    }
  };

  const handleCopy = () => {
    navigator.clipboard.writeText(msg.text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  };

  const timeStr = msg.timestamp.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const isEmpty  = isStreaming && msg.role === "assistant" && msg.text === "";

  return (
    <div className={`chat-bubble-wrap chat-bubble-wrap-${msg.role}`}>
      {msg.role === "assistant" && (
        <div className="bubble-ai-label">
          <span className="bubble-ai-dot" />
          ZanAI
        </div>
      )}
      <div className={`chat-bubble chat-bubble-v2 ${msg.role}${isStreaming ? " chat-bubble--streaming" : ""}`}>
        <div className="bubble-body">
          {msg.role === "assistant" ? (
            isEmpty ? (
              <div className="bubble-thinking">
                <div className="bubble-skeleton"><span /><span /><span /></div>
                <span className="bubble-thinking-text">{t("thinking")}</span>
              </div>
            ) : (
              <>
                {/* Tool pills above response text — matches ChatGPT order */}
                {msg.toolCalls && msg.toolCalls.length > 0 && (
                  <div className="bubble-tool-pills">
                    {msg.toolCalls.map(tc => <ToolPill key={tc.id} entry={tc} />)}
                  </div>
                )}
                <MarkdownContent text={msg.text} />
                {isStreaming && <span className="stream-cursor" aria-hidden="true" />}
              </>
            )
          ) : (
            <div className="bubble-text-plain">
              {msg.imagePreviewUrl && (
                <img src={msg.imagePreviewUrl} alt={t("attachedImage")} className="bubble-img-preview" />
              )}
              {msg.text}
            </div>
          )}
          {(msg.pendingAction || msg.pendingBatchActions) && (
            <div className="chat-pending-badge">
              {msg.pendingBatchActions
                ? `⏳ ${officeAiFormat(t("awaitingChanges"), { count: msg.pendingBatchActions.length })}`
                : `⏳ ${t("awaitingConfirmation")}`}
            </div>
          )}
          {onUndo && (
            <button className="chat-undo-btn" onClick={onUndo}>↩ {t("undoChange")}</button>
          )}
          {msg.suggestedPrompt && msg.suggestedLabel && onSuggestedPrompt && (
            <button
              className="chat-undo-btn"
              onClick={() => onSuggestedPrompt(msg.suggestedPrompt!)}
            >
              {msg.suggestedLabel}
            </button>
          )}
        </div>

        {!isEmpty && (
          <div className="bubble-footer">
            <span className="bubble-timestamp">{timeStr}</span>
            {isStreaming && <span className="bubble-streaming-badge">● {t("live")}</span>}
            {msg.role === "assistant" && !isStreaming && msg.feedbackReady && onFeedback && (
              <>
                <button
                  className={`bubble-feedback-btn${feedback === "up" ? " active" : ""}`}
                  onClick={() => void submitFeedback("up")}
                  disabled={feedbackPending}
                  title={t("goodResponse")}
                >👍</button>
                <button
                  className={`bubble-feedback-btn${feedback === "down" ? " active" : ""}`}
                  onClick={() => void submitFeedback("down")}
                  disabled={feedbackPending}
                  title={t("badResponse")}
                >👎</button>
                {feedbackError && <span className="bubble-feedback-error" title={t("feedbackNotSaved")}>{t("notSaved")}</span>}
              </>
            )}
            {msg.role !== "system" && !isStreaming && (
              <button className="bubble-copy-btn" onClick={handleCopy} title={t("copyMessage")}>
                {copied ? "✓" : "⎘"}
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

// ─── Message list ─────────────────────────────────────────────────────────────

export function ChatMessageList({
  messages, chatState, streamingMsgId, liveToolCalls, onUndo, onFeedback, onChip, userName, businessName,
}: {
  messages: DisplayMessage[];
  chatState: ChatState;
  streamingMsgId: string | null;
  liveToolCalls: ToolCallEntry[];
  onUndo: (undoId: string, msgId: string) => void;
  onFeedback?: (messageId: string, rating: "up" | "down", aiSessionId?: string) => Promise<void>;
  onChip: (text: string) => void;
  userName: string;
  businessName: string;
}) {
  const bottomRef = useRef<HTMLDivElement>(null);

  // Use "auto" (instant) during streaming to avoid queuing hundreds of smooth-
  // scroll animations per token which can hold DOM references and leak memory.
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: chatState === "idle" ? "smooth" : "auto" });
  }, [messages, chatState]);

  return (
    <div className="chat-messages" role="log" aria-live="polite" aria-relevant="additions">
      {messages.length === 0 && (
        <WelcomePanel userName={userName} businessName={businessName} onChip={onChip} />
      )}

      {messages.map((msg, i) => {
        const msgDate = msg.timestamp.toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" });
        const prevDate = i > 0 ? messages[i - 1].timestamp.toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" }) : null;
        const showSep  = i === 0 || msgDate !== prevDate;
        const isStreaming = msg.id === streamingMsgId;
        return (
          <React.Fragment key={msg.id}>
            {showSep && <div className="ai-date-sep">{msgDate}</div>}
            <ChatBubble
              msg={msg}
              isStreaming={isStreaming}
              onUndo={msg.undoId ? () => onUndo(msg.undoId!, msg.id) : undefined}
              onFeedback={onFeedback}
              onSuggestedPrompt={onChip}
            />
          </React.Fragment>
        );
      })}

      {/* Live tool call cards during streaming */}
      {liveToolCalls.length > 0 && (
        <div className="live-tool-calls">
          {liveToolCalls.map(tc => <ToolCallCard key={tc.id} entry={tc} />)}
        </div>
      )}

      <div ref={bottomRef} />
    </div>
  );
}
