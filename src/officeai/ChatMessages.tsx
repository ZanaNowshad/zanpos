import React, { useEffect, useRef, useState } from "react";
import type { ChatState, DisplayMessage, ToolCallEntry } from "./officeAiTypes";
import { QUICK_ACTIONS } from "./officeAiTypes";
import { MarkdownContent } from "./markdown";
import { ToolCallCard, toolMeta } from "./toolCards";

// ─── Quick chips bar ──────────────────────────────────────────────────────────

export function QuickChipsBar({ onSelect }: { onSelect: (text: string) => void }) {
  return (
    <div className="quick-chips-bar">
      {QUICK_ACTIONS.map((a) => (
        <button key={a.prompt} className="quick-chip" onClick={() => onSelect(a.prompt)}>
          {a.label}
        </button>
      ))}
    </div>
  );
}

// ─── Welcome panel ────────────────────────────────────────────────────────────

const WELCOME_CHIPS = [
  { icon: "📊", text: "Today's sales summary", prompt: "Give me today's sales summary" },
  { icon: "📦", text: "Low stock alerts",       prompt: "Which products are low on stock?" },
  { icon: "💵", text: "Cash drawer status",     prompt: "Show me the current cash drawer status" },
  { icon: "🏆", text: "Top products this week", prompt: "What are the top selling products this week?" },
  { icon: "↩",  text: "Recent refunds",         prompt: "Show me recent refunds" },
  { icon: "👥", text: "Today's transactions",   prompt: "How many transactions were made today?" },
];

export function WelcomePanel({ userName, businessName, onChip }: {
  userName: string;
  businessName: string;
  onChip: (text: string) => void;
}) {
  const hour = new Date().getHours();
  const greeting = hour < 12 ? "Good morning" : hour < 17 ? "Good afternoon" : "Good evening";
  return (
    <div className="chat-welcome-advanced">
      <div className="welcome-header">
        <div className="welcome-avatar">✦</div>
        <div>
          <div className="welcome-title">{greeting}, {userName} 👋</div>
          <div className="welcome-subtitle">I'm ZanAI, {businessName}'s admin assistant. Ask me anything about your store performance, inventory, sales, users, and more.</div>
        </div>
      </div>
      <div className="welcome-suggested-label">SUGGESTED QUESTIONS</div>
      <div className="welcome-suggested-grid">
        {WELCOME_CHIPS.map(s => (
          <button key={s.prompt} className="welcome-suggested-card" onClick={() => onChip(s.prompt)}>
            <span className="welcome-suggested-icon">{s.icon}</span>
            <span className="welcome-suggested-text">{s.text}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

// ─── Completed tool pill (collapsible, ChatGPT-style) ────────────────────────

function ToolPill({ entry }: { entry: ToolCallEntry }) {
  const [open, setOpen] = useState(false);
  const meta = toolMeta(entry.name);
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
  isStreaming = false,
}: {
  msg: DisplayMessage;
  onUndo?: () => void;
  isStreaming?: boolean;
}) {
  const [copied, setCopied] = useState(false);

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
                <span className="bubble-thinking-text">Thinking…</span>
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
                <img src={msg.imagePreviewUrl} alt="Attached image" className="bubble-img-preview" />
              )}
              {msg.text}
            </div>
          )}
          {msg.pendingAction && (
            <div className="chat-pending-badge">⏳ Awaiting confirmation…</div>
          )}
          {onUndo && (
            <button className="chat-undo-btn" onClick={onUndo}>↩ Undo this change</button>
          )}
        </div>

        {!isEmpty && (
          <div className="bubble-footer">
            <span className="bubble-timestamp">{timeStr}</span>
            {isStreaming && <span className="bubble-streaming-badge">● live</span>}
            {msg.role !== "system" && !isStreaming && (
              <button className="bubble-copy-btn" onClick={handleCopy} title="Copy message">
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
  messages, chatState, streamingMsgId, liveToolCalls, onUndo, onChip, userName, businessName,
}: {
  messages: DisplayMessage[];
  chatState: ChatState;
  streamingMsgId: string | null;
  liveToolCalls: ToolCallEntry[];
  onUndo: (undoId: string, msgId: string) => void;
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
    <div className="chat-messages">
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
