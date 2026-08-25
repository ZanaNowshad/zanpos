import React, { useEffect, useRef, useState } from "react";
import type { ChatState, DisplayMessage, ToolCallEntry } from "./officeAiTypes";
import { QUICK_ACTIONS } from "./officeAiTypes";
import { MarkdownContent } from "./markdown";
import AiFormCard from "./AiFormCard";
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

// ─── Suggested prompts ────────────────────────────────────────────────────────

/**
 * One list, two presentations. The welcome panel draws these as cards on an
 * empty chat; [`QuickActionBar`] draws the same six as a compact row that stays
 * put for the rest of the conversation. Kept as a single array because two
 * overlapping lists is how the fifth one ends up saying something different
 * from the other four.
 */
export const WELCOME_CHIPS = [
  { icon: "📊", labelKey: "todaysSalesSummary", prompt: "Give me today's sales summary" },
  { icon: "📦", labelKey: "lowStockAlerts", prompt: "Which products are low on stock?" },
  { icon: "💵", labelKey: "cashDrawerStatus", prompt: "Show me the current cash drawer status" },
  { icon: "🏆", labelKey: "topProductsThisWeek", prompt: "What are the top selling products this week?" },
  { icon: "↩", labelKey: "recentRefunds", prompt: "Show me recent refunds" },
  { icon: "👥", labelKey: "todaysTransactions", prompt: "How many transactions were made today?" },
] satisfies Array<{ icon: string; labelKey: OfficeAiStringKey; prompt: string }>;

/**
 * The suggestions, kept within reach for the whole conversation.
 *
 * They used to be part of the welcome panel and vanished the moment anything
 * was sent — so the one-tap route to "today's sales" existed only before you
 * had asked anything, which is the least likely moment to want it. On a till
 * especially, the value is asking again tomorrow without typing.
 *
 * One scrolling row rather than a wrapping grid: the floating widget is about
 * 360px wide and a grid of six would eat a third of the message area. Continue
 * rides in the same row instead of claiming one of its own, so making these
 * permanent costs the till a single line.
 */
export function QuickActionBar({ onSelect, onContinue, onShowProcedures }: {
  onSelect: (text: string) => void;
  onContinue?: () => void;
  onShowProcedures?: () => void;
}) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  return (
    <div className="chat-quick-bar" role="group" aria-label={t("suggestedQuestions")}>
      {onContinue && (
        <button
          className="chat-quick-chip chat-quick-chip-continue"
          onClick={onContinue}
          title={t("continueTask")}
        >
          {t("continueAction")}
          <span className="icon-directional" aria-hidden="true">▸</span>
        </button>
      )}
      {onShowProcedures && (
        <button className="chat-quick-chip" onClick={onShowProcedures}>
          <span aria-hidden="true">🗂</span>
          {t("showProcedures")}
        </button>
      )}
      {WELCOME_CHIPS.map(chip => (
        <button
          key={chip.prompt}
          className="chat-quick-chip"
          onClick={() => onSelect(chip.prompt)}
        >
          <span aria-hidden="true">{chip.icon}</span>
          {t(chip.labelKey)}
        </button>
      ))}
    </div>
  );
}

// ─── Welcome panel ────────────────────────────────────────────────────────────

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

interface ChatBubbleProps {
  msg: DisplayMessage;
  onUndo?: (undoId: string, msgId: string) => void | Promise<void>;
  onFeedback?: (messageId: string, rating: "up" | "down", aiSessionId?: string) => Promise<void>;
  onSuggestedPrompt?: (prompt: string) => void;
  onFormSubmit?: (msgId: string, text: string) => void;
  onFormDismiss?: (msgId: string) => void;
  formsDisabled?: boolean;
  isStreaming?: boolean;
}

function ChatBubbleImpl({
  msg,
  onUndo,
  onFeedback,
  onSuggestedPrompt,
  onFormSubmit,
  onFormDismiss,
  formsDisabled = false,
  isStreaming = false,
}: ChatBubbleProps) {
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
                {msg.form && onFormSubmit && onFormDismiss && (
                  <AiFormCard
                    form={msg.form}
                    disabled={formsDisabled}
                    onSubmit={text => onFormSubmit(msg.id, text)}
                    onDismiss={() => onFormDismiss(msg.id)}
                  />
                )}
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
          {onUndo && msg.undoId && (
            <button
              className="chat-undo-btn"
              onClick={() => void onUndo(msg.undoId!, msg.id)}
            >↩ {t("undoChange")}</button>
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

/**
 * Memoised because the list re-renders on every streamed token. A completed
 * message's props are stable, so only the streaming bubble actually re-renders
 * and re-parses its markdown; without this the cost is O(tokens x messages).
 */
export const ChatBubble = React.memo(ChatBubbleImpl);

/** One Intl instance for the whole list; constructing one per call is the
 *  expensive part of date formatting. */
const DAY_FORMAT = new Intl.DateTimeFormat("en-GB", {
  day: "numeric",
  month: "short",
  year: "numeric",
});
const dayLabelCache = new Map<number, string>();
function dayLabel(ts: Date): string {
  const key = ts.getTime();
  let label = dayLabelCache.get(key);
  if (label === undefined) {
    label = DAY_FORMAT.format(ts);
    dayLabelCache.set(key, label);
  }
  return label;
}

export function ChatMessageList({
  messages, chatState, streamingMsgId, liveToolCalls, onUndo, onFeedback, onChip,
  onFormSubmit, onFormDismiss, launcher, userName, businessName,
}: {
  /** The procedure launcher, when the panel wants it. Rendered inside the
   *  scroll region so it never squeezes the composer on a small surface. */
  launcher?: React.ReactNode;
  messages: DisplayMessage[];
  chatState: ChatState;
  streamingMsgId: string | null;
  liveToolCalls: ToolCallEntry[];
  onUndo?: (undoId: string, msgId: string) => void;
  onFeedback?: (messageId: string, rating: "up" | "down", aiSessionId?: string) => Promise<void>;
  onChip: (text: string) => void;
  onFormSubmit?: (msgId: string, text: string) => void;
  onFormDismiss?: (msgId: string) => void;
  userName: string;
  businessName: string;
}) {
  const bottomRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  // Use "auto" (instant) during streaming to avoid queuing hundreds of smooth-
  // scroll animations per token which can hold DOM references and leak memory.
  useEffect(() => {
    const behavior = chatState === "idle" ? "smooth" : "auto";
    /* The launcher sits above the transcript, so scrolling to the end of the
       thread lands on its twenty-first card and hides the question it is
       answering. It is the thing to read while it is up — and it is only up
       before the operator has asked anything, so there is no transcript being
       pulled away from them. */
    if (launcher) {
      listRef.current?.scrollTo({ top: 0, behavior });
      return;
    }
    /* Scrolling a form to the end of the thread lands on its last input and
       leaves the heading, the note and the first label above the fold — the
       operator gets a bare box with no question attached to it. "nearest" is
       the rule that serves both sizes: a two-field ask ends up fully visible
       with its buttons, and a bill taller than the widget aligns to its top. */
    const form = messages[messages.length - 1]?.form
      ? listRef.current?.querySelector(".aiform")
      : null;
    if (form) form.scrollIntoView({ behavior, block: "nearest" });
    else bottomRef.current?.scrollIntoView({ behavior });
  }, [messages, chatState, launcher]);

  return (
    <div ref={listRef} className="chat-messages" role="log" aria-live="polite" aria-relevant="additions">
      {messages.length === 0 && !launcher && (
        <WelcomePanel userName={userName} businessName={businessName} onChip={onChip} />
      )}
      {launcher}

      {messages.map((msg, i) => {
        const msgDate = dayLabel(msg.timestamp);
        const showSep = i === 0 || msgDate !== dayLabel(messages[i - 1].timestamp);
        const isStreaming = msg.id === streamingMsgId;
        return (
          <React.Fragment key={msg.id}>
            {showSep && <div className="ai-date-sep">{msgDate}</div>}
            <ChatBubble
              msg={msg}
              isStreaming={isStreaming}
              onUndo={onUndo}
              onFeedback={onFeedback}
              onSuggestedPrompt={onChip}
              onFormSubmit={onFormSubmit}
              onFormDismiss={onFormDismiss}
              formsDisabled={chatState !== "idle"}
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
