import React, { useEffect, useRef, useState } from "react";
import type { ChatState, DisplayMessage, ToolCallEntry } from "./officeAiTypes";
import { QUICK_ACTIONS } from "./officeAiTypes";
import { MarkdownContent } from "./markdown";
import { ToolCallCard } from "./toolCards";

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

export function WelcomePanel({ userName, businessName, onChip }: {
  userName: string;
  businessName: string;
  onChip: (text: string) => void;
}) {
  const hour = new Date().getHours();
  const greeting = hour < 12 ? "Good morning" : hour < 18 ? "Good afternoon" : "Good evening";

  const SUGGESTED = [
    { icon: "📊", text: "How are today's sales compared to yesterday?", prompt: "How are today's sales compared to yesterday?" },
    { icon: "📦", text: "Show me low stock items", prompt: "Which products are low on stock?" },
    { icon: "🏆", text: "What are the top selling products this week?", prompt: "What are the top selling products this week?" },
    { icon: "💵", text: "How much cash is in the drawer?", prompt: "Show me the current cash drawer status" },
    { icon: "↩", text: "Show recent refunds", prompt: "Show me recent refunds" },
    { icon: "👥", text: "How many transactions today?", prompt: "How many transactions were made today?" },
  ];

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
        {SUGGESTED.map(s => (
          <button key={s.prompt} className="welcome-suggested-card" onClick={() => onChip(s.prompt)}>
            <span className="welcome-suggested-icon">{s.icon}</span>
            <span>{s.text}</span>
          </button>
        ))}
      </div>
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
      <div className={`chat-bubble chat-bubble-v2 ${msg.role}${isStreaming ? " chat-bubble--streaming" : ""}`}>
        <div className="bubble-body">
          {msg.role === "assistant" ? (
            isEmpty ? (
              /* Skeleton dots while waiting for first token */
              <div className="bubble-skeleton">
                <span /><span /><span />
              </div>
            ) : (
              <>
                <MarkdownContent text={msg.text} />
                {isStreaming && <span className="stream-cursor" aria-hidden="true" />}
              </>
            )
          ) : (
            <div className="bubble-text-plain">{msg.text}</div>
          )}
          {msg.pendingAction && (
            <div className="chat-pending-badge">⏳ Awaiting confirmation…</div>
          )}
          {onUndo && (
            <button className="chat-undo-btn" onClick={onUndo}>↩ Undo this change</button>
          )}
        </div>

        {/* Stored tool calls — shown after response is complete */}
        {msg.toolCalls && msg.toolCalls.length > 0 && (
          <div className="bubble-tool-calls">
            <div className="bubble-tool-calls-label">🔧 Tools used</div>
            {msg.toolCalls.map(tc => <ToolCallCard key={tc.id} entry={tc} />)}
          </div>
        )}

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
        const showSep = i === 0 || msgDate !== prevDate;
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

      {/* Live tool call cards — rendered while AI is calling tools */}
      {liveToolCalls.length > 0 && (
        <div className="live-tool-calls">
          <div className="live-tool-calls-header">
            <span className="live-tool-calls-label">🔧 AI is calling tools</span>
          </div>
          {liveToolCalls.map(tc => <ToolCallCard key={tc.id} entry={tc} />)}
        </div>
      )}

      <div ref={bottomRef} />
    </div>
  );
}
