import React, { useState, useRef, useEffect, useCallback } from "react";
import type {
  SessionUser,
  ChatMessage,
  AiChatResponse,
  ToolPreview,
  ProviderConfig,
  ModelInfo,
  TodaySummary,
  StockLevel,
  SyncStatus,
} from "../types";
import { DEVICE } from "../types";
import {
  adminGetProviderConfig,
  adminSetAnthropic,
  adminValidateOpenai,
  adminSetOpenai,
  adminSetupSupabase,
  adminGetSupabaseStatus,
  aiChat,
  aiExecuteAction,
  aiCancelAction,
  aiUndoAction,
  reportToday,
  inventoryGetLevels,
  syncStatus,
} from "../tauri/commands";
import ConfirmActionModal from "../components/ConfirmActionModal";

// ─── Types ────────────────────────────────────────────────────────────────────

interface Props {
  sessionUser: SessionUser;
  onBackToPOS: () => void;
}

interface DisplayMessage {
  id: string;
  role: "user" | "assistant" | "system";
  text: string;
  timestamp: Date;
  pendingAction?: {
    action_id: string;
    tool_name: string;
    preview: ToolPreview;
    expires_at: string;
    assistant_text: string;
  };
  undoId?: string;
}

type ChatState = "idle" | "thinking" | "confirm";

// ── Setup wizard state machine ────────────────────────────────────────────────
type SetupStep =
  | "loading"
  | "sync_setup"
  | "sync_migrating"
  | "settings"
  | "pick_provider"
  | "anthropic_key"
  | "openai_url_key"
  | "openai_validating"
  | "openai_pick_model"
  | "done";

interface KpiSnapshot {
  loading: boolean;
  error: string | null;
  today: TodaySummary | null;
  lowStockCount: number;
  outOfStockCount: number;
  sync: SyncStatus | null;
}

// ─── Quick-action prompts ─────────────────────────────────────────────────────

const QUICK_ACTIONS = [
  { label: "📊 Today's sales", prompt: "Give me today's sales summary" },
  { label: "⚠️ Low stock", prompt: "Which products are low on stock?" },
  { label: "💵 Cash drawer", prompt: "Show me the current cash drawer status" },
  { label: "🏆 Top products", prompt: "What are the top selling products this month?" },
  { label: "🔄 Recent refunds", prompt: "Show me recent refunds" },
  { label: "🕐 Shift history", prompt: "Show me the last 5 shifts" },
  { label: "🔗 Audit chain", prompt: "Check the audit log chain integrity" },
  { label: "☁️ Sync status", prompt: "What is the sync status?" },
];

// ─── Markdown renderer ────────────────────────────────────────────────────────

function MarkdownContent({ text }: { text: string }) {
  const nodes = parseMarkdown(text);
  return <div className="md-body">{nodes}</div>;
}

function parseMarkdown(text: string): React.ReactNode[] {
  const lines = text.split("\n");
  const result: React.ReactNode[] = [];
  let i = 0;
  let key = 0;

  while (i < lines.length) {
    const line = lines[i];

    // ── Code block ──────────────────────────────────────────────────────────
    if (line.trimStart().startsWith("```")) {
      const lang = line.trimStart().slice(3).trim();
      const codeLines: string[] = [];
      i++;
      while (i < lines.length && !lines[i].trimStart().startsWith("```")) {
        codeLines.push(lines[i]);
        i++;
      }
      result.push(
        <div key={key++} className="md-code-block">
          {lang && <span className="md-code-lang">{lang}</span>}
          <pre><code>{codeLines.join("\n")}</code></pre>
        </div>
      );
      i++; // skip closing ```
      continue;
    }

    // ── Table ────────────────────────────────────────────────────────────────
    if (line.includes("|") && line.trim().startsWith("|")) {
      const tableRows: string[][] = [];
      while (i < lines.length && lines[i].includes("|") && lines[i].trim().startsWith("|")) {
        const cols = lines[i].split("|").map(c => c.trim()).filter((_, idx, arr) => idx > 0 && idx < arr.length - 1);
        // Skip separator rows (e.g. |---|---|)
        if (!cols.every(c => /^[-: ]+$/.test(c))) {
          tableRows.push(cols);
        }
        i++;
      }
      if (tableRows.length > 0) {
        result.push(
          <div key={key++} className="md-table-wrap">
            <table className="md-table">
              <thead>
                <tr>{tableRows[0].map((h, j) => <th key={j}>{inlineMarkdown(h)}</th>)}</tr>
              </thead>
              <tbody>
                {tableRows.slice(1).map((row, ri) => (
                  <tr key={ri}>{row.map((cell, ci) => <td key={ci}>{inlineMarkdown(cell)}</td>)}</tr>
                ))}
              </tbody>
            </table>
          </div>
        );
      }
      continue;
    }

    // ── Heading ──────────────────────────────────────────────────────────────
    const headMatch = line.match(/^(#{1,4})\s+(.+)/);
    if (headMatch) {
      const level = headMatch[1].length;
      const Tag = (level <= 2 ? "h3" : "h4") as keyof React.JSX.IntrinsicElements;
      result.push(<Tag key={key++} className={`md-h${level}`}>{inlineMarkdown(headMatch[2])}</Tag>);
      i++;
      continue;
    }

    // ── Unordered list ────────────────────────────────────────────────────────
    if (/^[-*+]\s/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^[-*+]\s/.test(lines[i])) {
        items.push(lines[i].replace(/^[-*+]\s/, ""));
        i++;
      }
      result.push(
        <ul key={key++} className="md-ul">
          {items.map((it, j) => <li key={j}>{inlineMarkdown(it)}</li>)}
        </ul>
      );
      continue;
    }

    // ── Ordered list ──────────────────────────────────────────────────────────
    if (/^\d+\.\s/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^\d+\.\s/.test(lines[i])) {
        items.push(lines[i].replace(/^\d+\.\s/, ""));
        i++;
      }
      result.push(
        <ol key={key++} className="md-ol">
          {items.map((it, j) => <li key={j}>{inlineMarkdown(it)}</li>)}
        </ol>
      );
      continue;
    }

    // ── Horizontal rule ──────────────────────────────────────────────────────
    if (/^---+$/.test(line.trim())) {
      result.push(<hr key={key++} className="md-hr" />);
      i++;
      continue;
    }

    // ── Blank line ────────────────────────────────────────────────────────────
    if (line.trim() === "") {
      i++;
      continue;
    }

    // ── Paragraph ────────────────────────────────────────────────────────────
    const paraLines: string[] = [];
    while (
      i < lines.length &&
      lines[i].trim() !== "" &&
      !/^(#{1,4}|[-*+]|\d+\.)\s/.test(lines[i]) &&
      !lines[i].includes("|") &&
      !lines[i].trimStart().startsWith("```") &&
      !/^---+$/.test(lines[i].trim())
    ) {
      paraLines.push(lines[i]);
      i++;
    }
    result.push(
      <p key={key++} className="md-p">{inlineMarkdown(paraLines.join(" "))}</p>
    );
  }

  return result;
}

function inlineMarkdown(text: string): React.ReactNode {
  // Parse bold, italic, inline code
  const parts: React.ReactNode[] = [];
  const regex = /(\*\*(.+?)\*\*|\*(.+?)\*|`(.+?)`)/g;
  let last = 0;
  let match: RegExpExecArray | null;
  let key = 0;

  while ((match = regex.exec(text)) !== null) {
    if (match.index > last) {
      parts.push(text.slice(last, match.index));
    }
    if (match[2] !== undefined) {
      parts.push(<strong key={key++}>{match[2]}</strong>);
    } else if (match[3] !== undefined) {
      parts.push(<em key={key++}>{match[3]}</em>);
    } else if (match[4] !== undefined) {
      parts.push(<code key={key++} className="md-inline-code">{match[4]}</code>);
    }
    last = match.index + match[0].length;
  }
  if (last < text.length) {
    parts.push(text.slice(last));
  }
  return parts.length === 0 ? text : parts;
}

// ─── Thinking context chip ─────────────────────────────────────────────────────

function getThinkingLabel(lastMessage: string): string {
  const m = lastMessage.toLowerCase();
  if (/\b(sale|revenue|report|today|total|transaction)\b/.test(m)) return "📊 Fetching sales data…";
  if (/\b(stock|inventory|level|reorder|low)\b/.test(m))            return "📦 Checking inventory…";
  if (/\b(cash|drawer|float|drop|paid)\b/.test(m))                  return "💵 Reading cash drawer…";
  if (/\b(refund|return)\b/.test(m))                                 return "🔄 Looking up refunds…";
  if (/\b(product|item|barcode|sku|price|catalog)\b/.test(m))       return "🏷 Searching products…";
  if (/\b(audit|chain|tamper|hash|integrity)\b/.test(m))            return "🔍 Verifying audit chain…";
  if (/\b(sync|cloud|supabase|upload|queue)\b/.test(m))             return "☁ Checking sync status…";
  if (/\b(shift|cashier|open|close|session)\b/.test(m))             return "🕐 Loading shift data…";
  if (/\b(top|best|popular|sell)\b/.test(m))                        return "🏆 Ranking products…";
  if (/\b(categor|group|type)\b/.test(m))                           return "📂 Fetching categories…";
  return "🤔 Analyzing your request…";
}

// ─── Subcomponents ────────────────────────────────────────────────────────────

function TopBar({
  label, user, providerLabel, showKpi, onToggleKpi,
  onClearChat, onSettings, onBack,
}: {
  label: string; user: string; providerLabel: string;
  showKpi: boolean; onToggleKpi: () => void;
  onClearChat: () => void; onSettings: () => void; onBack: () => void;
}) {
  return (
    <div className="admin-chat-topbar">
      <button className="topbar-btn" onClick={onBack}>← POS</button>
      <div className="topbar-title-wrap">
        <span className="admin-chat-title">{label}</span>
        {providerLabel && (
          <span className="admin-chat-provider-badge">{providerLabel}</span>
        )}
      </div>
      <div className="topbar-actions">
        <button
          className={`topbar-icon-btn ${showKpi ? "topbar-icon-btn-active" : ""}`}
          onClick={onToggleKpi}
          title="Toggle KPI panel"
        >📊</button>
        <button
          className="topbar-icon-btn"
          onClick={onClearChat}
          title="Clear conversation"
        >🗑</button>
        <button
          className="topbar-icon-btn"
          onClick={onSettings}
          title="Sync & AI settings"
        >⚙</button>
        <span className="admin-chat-user">{user}</span>
      </div>
    </div>
  );
}

function SetupTopBar({ label, user, onBack }: { label: string; user: string; onBack: () => void }) {
  return (
    <div className="admin-chat-topbar">
      <button className="topbar-btn" onClick={onBack}>← Back</button>
      <span className="admin-chat-title">{label}</span>
      <span className="admin-chat-user">{user}</span>
    </div>
  );
}

// ─── KPI Sidebar ──────────────────────────────────────────────────────────────

function KpiSidebar({ kpi, currencyExp, onRefresh }: {
  kpi: KpiSnapshot;
  currencyExp: number;
  onRefresh: () => void;
}) {
  const fmt = (n: number) => {
    const exp = currencyExp;
    const divisor = Math.pow(10, exp);
    return (n / divisor).toFixed(exp);
  };

  return (
    <aside className="kpi-sidebar">
      <div className="kpi-sidebar-header">
        <span className="kpi-sidebar-title">Live Snapshot</span>
        <button className="kpi-refresh-btn" onClick={onRefresh} disabled={kpi.loading} title="Refresh">
          {kpi.loading ? "⌛" : "↻"}
        </button>
      </div>

      {kpi.error && <div className="kpi-error">{kpi.error}</div>}

      {kpi.today && (
        <>
          <div className="kpi-section-label">Today · {kpi.today.business_date}</div>
          <div className="kpi-card">
            <div className="kpi-card-label">Net Sales</div>
            <div className="kpi-card-value">BHD {fmt(kpi.today.net_total_minor)}</div>
          </div>
          <div className="kpi-card">
            <div className="kpi-card-label">Transactions</div>
            <div className="kpi-card-value kpi-value-neutral">{kpi.today.transaction_count}</div>
          </div>
          <div className="kpi-row-pair">
            <div className="kpi-mini-card">
              <div className="kpi-mini-label">💵 Cash</div>
              <div className="kpi-mini-value">{fmt(kpi.today.cash_total_minor)}</div>
            </div>
            <div className="kpi-mini-card">
              <div className="kpi-mini-label">💳 Card</div>
              <div className="kpi-mini-value">{fmt(kpi.today.card_total_minor)}</div>
            </div>
          </div>
          {kpi.today.refund_count > 0 && (
            <div className="kpi-card kpi-card-warn">
              <div className="kpi-card-label">Refunds</div>
              <div className="kpi-card-value kpi-value-warn">
                {kpi.today.refund_count} · BHD {fmt(kpi.today.refund_total_minor)}
              </div>
            </div>
          )}
        </>
      )}

      {!kpi.loading && kpi.today === null && !kpi.error && (
        <div className="kpi-empty">No data yet</div>
      )}

      {(kpi.lowStockCount > 0 || kpi.outOfStockCount > 0) && (
        <div className="kpi-card kpi-card-alert">
          <div className="kpi-card-label">Stock Alerts</div>
          <div className="kpi-card-value kpi-value-alert">
            {kpi.outOfStockCount > 0 && <span>❌ {kpi.outOfStockCount} out</span>}
            {kpi.lowStockCount > 0 && <span>⚠️ {kpi.lowStockCount} low</span>}
          </div>
        </div>
      )}

      {kpi.sync && (
        <div className={`kpi-card ${kpi.sync.supabase_configured ? "kpi-card-ok" : "kpi-card-neutral"}`}>
          <div className="kpi-card-label">Sync</div>
          <div className="kpi-card-value">
            {kpi.sync.supabase_configured
              ? <>☁ <span className="kpi-value-ok">Connected</span></>
              : <span className="kpi-value-dim">Not configured</span>}
          </div>
          {kpi.sync.pending_events > 0 && (
            <div className="kpi-card-sub">{kpi.sync.pending_events} pending</div>
          )}
          {kpi.sync.last_successful_sync_at && (
            <div className="kpi-card-sub">
              Last: {kpi.sync.last_successful_sync_at.slice(0, 16).replace("T", " ")}
            </div>
          )}
        </div>
      )}
    </aside>
  );
}

// ─── Welcome panel ────────────────────────────────────────────────────────────

function WelcomePanel({ userName, kpi, currencyExp, onChip }: {
  userName: string;
  kpi: KpiSnapshot;
  currencyExp: number;
  onChip: (text: string) => void;
}) {
  const fmt = (n: number) => {
    const divisor = Math.pow(10, currencyExp);
    return `BHD ${(n / divisor).toFixed(currencyExp)}`;
  };

  const hour = new Date().getHours();
  const greeting = hour < 12 ? "Good morning" : hour < 18 ? "Good afternoon" : "Good evening";

  return (
    <div className="chat-welcome-advanced">
      <div className="welcome-header">
        <span className="welcome-icon">◆</span>
        <div>
          <div className="welcome-title">{greeting}, {userName}</div>
          <div className="welcome-subtitle">AI Admin Intelligence · ZanPOS</div>
        </div>
      </div>

      {kpi.today && (
        <div className="welcome-insight-card">
          <div className="welcome-insight-label">📈 Today so far</div>
          <div className="welcome-insight-row">
            <div className="welcome-insight-stat">
              <div className="wi-value">{fmt(kpi.today.net_total_minor)}</div>
              <div className="wi-label">Net Sales</div>
            </div>
            <div className="welcome-insight-divider" />
            <div className="welcome-insight-stat">
              <div className="wi-value">{kpi.today.transaction_count}</div>
              <div className="wi-label">Transactions</div>
            </div>
            <div className="welcome-insight-divider" />
            <div className="welcome-insight-stat">
              <div className="wi-value">{fmt(kpi.today.cash_total_minor)}</div>
              <div className="wi-label">Cash</div>
            </div>
          </div>
          {(kpi.lowStockCount > 0 || kpi.outOfStockCount > 0) && (
            <div className="welcome-alert-row">
              {kpi.outOfStockCount > 0 && (
                <button className="welcome-alert-chip welcome-alert-danger" onClick={() => onChip("Which products are out of stock?")}>
                  ❌ {kpi.outOfStockCount} out of stock
                </button>
              )}
              {kpi.lowStockCount > 0 && (
                <button className="welcome-alert-chip welcome-alert-warn" onClick={() => onChip("Which products are low on stock?")}>
                  ⚠️ {kpi.lowStockCount} low stock
                </button>
              )}
            </div>
          )}
        </div>
      )}

      <div className="welcome-capabilities">
        <div className="welcome-cap-title">I can help you with:</div>
        <div className="welcome-cap-grid">
          <div className="welcome-cap-item">📊 Sales reports &amp; analytics</div>
          <div className="welcome-cap-item">🏷 Product &amp; price management</div>
          <div className="welcome-cap-item">📦 Inventory &amp; stock levels</div>
          <div className="welcome-cap-item">💵 Cash drawer reconciliation</div>
          <div className="welcome-cap-item">🔍 Audit trail &amp; chain verify</div>
          <div className="welcome-cap-item">☁ Sync status &amp; conflicts</div>
        </div>
      </div>

      <div className="welcome-hint">All changes require your confirmation before executing.</div>
    </div>
  );
}

// ─── Quick chips bar ──────────────────────────────────────────────────────────

function QuickChipsBar({ onSelect }: { onSelect: (text: string) => void }) {
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

// ─── Chat bubble ──────────────────────────────────────────────────────────────

function ChatBubble({
  msg,
  onUndo,
}: {
  msg: DisplayMessage;
  onUndo?: () => void;
}) {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(msg.text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  };

  const timeStr = msg.timestamp.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

  return (
    <div className={`chat-bubble-wrap chat-bubble-wrap-${msg.role}`}>
      <div className={`chat-bubble chat-bubble-v2 ${msg.role}`}>
        <div className="bubble-body">
          {msg.role === "assistant" ? (
            <MarkdownContent text={msg.text} />
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
        <div className="bubble-footer">
          <span className="bubble-timestamp">{timeStr}</span>
          {msg.role !== "system" && (
            <button className="bubble-copy-btn" onClick={handleCopy} title="Copy message">
              {copied ? "✓" : "⎘"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

// ─── Thinking bubble ──────────────────────────────────────────────────────────

function ThinkingBubble({ lastMessage }: { lastMessage: string }) {
  const label = getThinkingLabel(lastMessage);
  return (
    <div className="chat-bubble-wrap chat-bubble-wrap-assistant">
      <div className="chat-bubble chat-bubble-v2 assistant thinking-v2">
        <div className="thinking-context-chip">{label}</div>
        <div className="thinking-dots-v2">
          <span /><span /><span />
        </div>
      </div>
    </div>
  );
}

// ─── Main component ───────────────────────────────────────────────────────────

export default function AdminChatPage({ sessionUser, onBackToPOS }: Props) {
  const [setupStep, setSetupStep] = useState<SetupStep>("loading");
  const [config, setConfig] = useState<ProviderConfig | null>(null);

  // Supabase sync setup
  const [supabaseUrl, setSupabaseUrl]     = useState("");
  const [supabaseKey, setSupabaseKey]     = useState("");
  const [supabasePat, setSupabasePat]     = useState("");
  const [supabaseError, setSupabaseError] = useState("");
  const [supabaseMigrating, setSupabaseMigrating] = useState(false);
  const [settingsTab, setSettingsTab]     = useState<"sync" | "ai">("sync");

  // Anthropic setup
  const [anthropicKey, setAnthropicKey]       = useState("");
  const [savingAnthropic, setSavingAnthropic] = useState(false);
  const [anthropicError, setAnthropicError]   = useState("");

  // OpenAI setup
  const [openaiBaseUrl, setOpenaiBaseUrl] = useState("https://api.openai.com/v1");
  const [openaiKey, setOpenaiKey]         = useState("");
  const [validating, setValidating]       = useState(false);
  const [validateError, setValidateError] = useState("");
  const [modelList, setModelList]         = useState<ModelInfo[]>([]);
  const [selectedModel, setSelectedModel] = useState("");
  const [savingOpenai, setSavingOpenai]   = useState(false);

  // Chat state
  const [messages, setMessages]           = useState<DisplayMessage[]>([]);
  const [input, setInput]                 = useState("");
  const [chatState, setChatState]         = useState<ChatState>("idle");
  const [history, setHistory]             = useState<ChatMessage[]>([]);
  const [pendingAction, setPendingAction] = useState<DisplayMessage["pendingAction"] | null>(null);
  const [lastUserMsg, setLastUserMsg]     = useState("");

  // UI state
  const [showKpi, setShowKpi]             = useState(true);
  const [kpi, setKpi]                     = useState<KpiSnapshot>({
    loading: false, error: null, today: null, lowStockCount: 0, outOfStockCount: 0, sync: null,
  });

  const bottomRef  = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // ── Initial load ────────────────────────────────────────────────────────────
  useEffect(() => {
    Promise.all([
      adminGetSupabaseStatus().catch(() => ({ configured: false })),
      adminGetProviderConfig().catch(() => null),
    ]).then(([supaStatus, cfg]) => {
      if (cfg) {
        setConfig(cfg);
        if (cfg.openai_base_url) setOpenaiBaseUrl(cfg.openai_base_url);
      }
      if (!supaStatus.configured) {
        setSetupStep("sync_setup");
      } else if (!cfg?.provider) {
        setSetupStep("pick_provider");
      } else {
        setSetupStep("done");
      }
    });
  }, []);

  // ── Fetch KPI when chat is ready ────────────────────────────────────────────
  const fetchKpi = useCallback(async () => {
    setKpi(prev => ({ ...prev, loading: true, error: null }));
    try {
      const today = new Date().toISOString().slice(0, 10);
      const [todaySummary, levels, syncStat] = await Promise.all([
        reportToday(DEVICE.branch_id, today).catch(() => null),
        inventoryGetLevels().catch(() => [] as StockLevel[]),
        syncStatus().catch(() => null),
      ]);
      const lowStockCount  = levels.filter(l => l.is_low_stock && !l.is_out_of_stock).length;
      const outOfStockCount = levels.filter(l => l.is_out_of_stock).length;
      setKpi({ loading: false, error: null, today: todaySummary, lowStockCount, outOfStockCount, sync: syncStat });
    } catch (e) {
      setKpi(prev => ({ ...prev, loading: false, error: String(e) }));
    }
  }, []);

  useEffect(() => {
    if (setupStep === "done") fetchKpi();
  }, [setupStep, fetchKpi]);

  // ── Auto-scroll ─────────────────────────────────────────────────────────────
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, chatState]);

  // ── Auto-expand textarea ────────────────────────────────────────────────────
  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
      textareaRef.current.style.height = Math.min(textareaRef.current.scrollHeight, 160) + "px";
    }
  }, [input]);

  const addMessage = (msg: Omit<DisplayMessage, "id" | "timestamp">): DisplayMessage => {
    const full = { ...msg, id: crypto.randomUUID(), timestamp: new Date() };
    setMessages(prev => [...prev, full]);
    return full;
  };

  // ── Anthropic save ──────────────────────────────────────────────────────────
  const handleSaveAnthropic = async () => {
    if (!anthropicKey.trim()) return;
    setSavingAnthropic(true);
    setAnthropicError("");
    try {
      await adminSetAnthropic(anthropicKey.trim());
      const cfg = await adminGetProviderConfig();
      setConfig(cfg);
      setSetupStep("done");
    } catch (e) {
      setAnthropicError(String(e));
    } finally {
      setSavingAnthropic(false);
    }
  };

  // ── OpenAI validate ─────────────────────────────────────────────────────────
  const handleValidateOpenai = async () => {
    if (!openaiBaseUrl.trim() || !openaiKey.trim()) return;
    setValidating(true);
    setValidateError("");
    setModelList([]);
    setSetupStep("openai_validating");
    try {
      const result = await adminValidateOpenai(openaiBaseUrl.trim(), openaiKey.trim());
      if (result.success && result.models.length > 0) {
        setModelList(result.models);
        setSelectedModel(result.models[0].id);
        setSetupStep("openai_pick_model");
      } else if (result.success) {
        setValidateError("Connection successful but no models returned. Enter a model name manually.");
        setSetupStep("openai_pick_model");
      } else {
        setValidateError(result.error ?? "Validation failed");
        setSetupStep("openai_url_key");
      }
    } catch (e) {
      setValidateError(String(e));
      setSetupStep("openai_url_key");
    } finally {
      setValidating(false);
    }
  };

  // ── OpenAI save ─────────────────────────────────────────────────────────────
  const handleSaveOpenai = async () => {
    if (!selectedModel.trim()) return;
    setSavingOpenai(true);
    try {
      await adminSetOpenai(openaiBaseUrl.trim(), openaiKey.trim(), selectedModel.trim());
      const cfg = await adminGetProviderConfig();
      setConfig(cfg);
      setSetupStep("done");
    } catch (e) {
      setValidateError(String(e));
    } finally {
      setSavingOpenai(false);
    }
  };

  // ── Supabase setup ──────────────────────────────────────────────────────────
  const handleSetupSupabase = async () => {
    if (!supabaseUrl.trim() || !supabaseKey.trim() || !supabasePat.trim()) return;
    setSupabaseError("");
    setSupabaseMigrating(true);
    setSetupStep("sync_migrating");
    try {
      await adminSetupSupabase(supabaseUrl.trim(), supabaseKey.trim(), supabasePat.trim());
      setSupabasePat("");
      const cfg = await adminGetProviderConfig().catch(() => null);
      if (cfg) setConfig(cfg);
      setSetupStep(cfg?.provider ? "done" : "pick_provider");
    } catch (e) {
      setSupabaseError(String(e));
      setSetupStep("sync_setup");
    } finally {
      setSupabaseMigrating(false);
    }
  };

  // ── Chat send ───────────────────────────────────────────────────────────────
  const handleSend = async (overrideText?: string) => {
    const text = (overrideText ?? input).trim();
    if (!text || chatState !== "idle") return;
    setInput("");
    setLastUserMsg(text);
    addMessage({ role: "user", text });
    const newHistory: ChatMessage[] = [...history, { role: "user", content: text }];
    setHistory(newHistory);
    setChatState("thinking");

    try {
      const resp: AiChatResponse = await aiChat({
        history,
        message: text,
        user_id: sessionUser.user_id,
        branch_id: DEVICE.branch_id,
        currency_exponent: DEVICE.currency_exponent,
      });

      if (resp.type === "no_api_key") {
        addMessage({ role: "system", text: "No AI provider configured. Please set one up." });
        const cfg = await adminGetProviderConfig().catch(() => null);
        if (cfg) setConfig(cfg);
        setSetupStep("pick_provider");
        setChatState("idle");
        return;
      }

      if (resp.type === "message") {
        addMessage({ role: "assistant", text: resp.content });
        setHistory(prev => [...prev, { role: "assistant", content: resp.content }]);
        setChatState("idle");
        return;
      }

      if (resp.type === "pending_action") {
        const actionData = {
          action_id: resp.action_id,
          tool_name: resp.tool_name,
          preview: resp.preview,
          expires_at: resp.expires_at,
          assistant_text: resp.assistant_text,
        };
        addMessage({
          role: "assistant",
          text: resp.assistant_text || "I'd like to make the following change:",
          pendingAction: actionData,
        });
        setPendingAction(actionData);
        setChatState("confirm");
        return;
      }
    } catch (e) {
      addMessage({ role: "system", text: `Error: ${String(e)}` });
      setChatState("idle");
    }
  };

  const handleConfirm = async () => {
    if (!pendingAction) return;
    setChatState("thinking");
    setPendingAction(null);
    try {
      const result = await aiExecuteAction({
        action_id: pendingAction.action_id,
        user_id: sessionUser.user_id,
        history,
        assistant_text: pendingAction.assistant_text,
        currency_exponent: DEVICE.currency_exponent,
      });
      addMessage({
        role: "assistant",
        text: result.followup,
        undoId: result.undo_id ?? undefined,
      });
      setHistory(prev => [
        ...prev,
        { role: "assistant", content: pendingAction.assistant_text },
        { role: "assistant", content: result.followup },
      ]);
      setChatState("idle");
      // Refresh KPI after a mutation
      setTimeout(fetchKpi, 500);
    } catch (e) {
      addMessage({ role: "system", text: `Execution failed: ${String(e)}` });
      setChatState("idle");
    }
  };

  const handleCancel = async () => {
    if (!pendingAction) return;
    await aiCancelAction(pendingAction.action_id).catch(() => {});
    addMessage({ role: "system", text: "Action cancelled." });
    setPendingAction(null);
    setChatState("idle");
  };

  const handleUndo = async (undoId: string, msgId: string) => {
    try {
      const result = await aiUndoAction(undoId, sessionUser.user_id, DEVICE.currency_exponent);
      setMessages(prev => prev.map(m => m.id === msgId ? { ...m, undoId: undefined } : m));
      addMessage({ role: "system", text: result.followup });
      setTimeout(fetchKpi, 500);
    } catch (e) {
      addMessage({ role: "system", text: `Undo failed: ${String(e)}` });
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); handleSend(); }
  };

  const providerLabel = config?.provider === "anthropic"
    ? "◆ Claude"
    : config?.provider === "openai"
    ? `⬡ ${config.openai_model}`
    : "";

  // ── Render: sync setup ──────────────────────────────────────────────────────
  if (setupStep === "sync_setup" || setupStep === "sync_migrating") {
    return (
      <div className="admin-chat-page">
        <SetupTopBar label="Store Setup — Sync" user={sessionUser.display_name} onBack={onBackToPOS} />
        <div className="setup-center">
          <div className="setup-card setup-card-wide">
            <h2>Connect to Supabase</h2>
            <p className="setup-subtitle">
              ZanPOS uses <strong>Supabase</strong> to sync data across devices.
              Credentials are stored locally. The PAT is used once and discarded.
            </p>
            <label className="setup-label">Supabase Project URL
              <input type="text" className="setup-input"
                placeholder="https://xyz.supabase.co"
                value={supabaseUrl}
                onChange={e => setSupabaseUrl(e.target.value)}
                disabled={supabaseMigrating}
              />
            </label>
            <p className="setup-hint">Settings → API → Project URL</p>
            <label className="setup-label">Service Role Key (Secret)
              <input type="password" className="setup-input"
                placeholder="eyJhbGciOi…"
                value={supabaseKey}
                onChange={e => setSupabaseKey(e.target.value)}
                disabled={supabaseMigrating}
              />
            </label>
            <label className="setup-label">
              Personal Access Token <span className="setup-hint-inline">(one-time use)</span>
              <input type="password" className="setup-input"
                placeholder="sbp_…"
                value={supabasePat}
                onChange={e => setSupabasePat(e.target.value)}
                disabled={supabaseMigrating}
                onKeyDown={e => e.key === "Enter" && handleSetupSupabase()}
              />
            </label>
            {supabaseError && <p className="setup-error">{supabaseError}</p>}
            {supabaseMigrating && <p className="setup-migrating">Creating central tables…</p>}
            <div className="setup-actions">
              <button className="btn-secondary" onClick={() => setSetupStep("pick_provider")} disabled={supabaseMigrating}>Skip for now</button>
              <button className="btn-primary"
                onClick={handleSetupSupabase}
                disabled={supabaseMigrating || !supabaseUrl.trim() || !supabaseKey.trim() || !supabasePat.trim()}
              >{supabaseMigrating ? "Migrating…" : "Connect & Set Up"}</button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // ── Render: settings hub ────────────────────────────────────────────────────
  if (setupStep === "settings") {
    return (
      <div className="admin-chat-page">
        <SetupTopBar label="Settings" user={sessionUser.display_name} onBack={() => setSetupStep("done")} />
        <div className="setup-center">
          <div className="setup-card setup-card-wide">
            <div className="settings-tabs">
              <button className={`settings-tab ${settingsTab === "sync" ? "settings-tab-active" : ""}`} onClick={() => setSettingsTab("sync")}>☁ Sync</button>
              <button className={`settings-tab ${settingsTab === "ai" ? "settings-tab-active" : ""}`} onClick={() => setSettingsTab("ai")}>◆ AI Provider</button>
            </div>
            {settingsTab === "sync" && (
              <div>
                <h2>Supabase Sync</h2>
                <p className="setup-subtitle">Update credentials. Re-running migration is safe (idempotent).</p>
                <label className="setup-label">Project URL
                  <input type="text" className="setup-input" value={supabaseUrl} onChange={e => setSupabaseUrl(e.target.value)} disabled={supabaseMigrating} />
                </label>
                <label className="setup-label">Service Role Key
                  <input type="password" className="setup-input" value={supabaseKey} onChange={e => setSupabaseKey(e.target.value)} disabled={supabaseMigrating} />
                </label>
                <label className="setup-label">PAT <span className="setup-hint-inline">(leave blank to update keys only)</span>
                  <input type="password" className="setup-input" placeholder="sbp_…" value={supabasePat} onChange={e => setSupabasePat(e.target.value)} disabled={supabaseMigrating} />
                </label>
                {supabaseError && <p className="setup-error">{supabaseError}</p>}
                <div className="setup-actions">
                  <button className="btn-secondary" onClick={() => setSetupStep("done")}>Cancel</button>
                  <button className="btn-primary" onClick={handleSetupSupabase} disabled={supabaseMigrating || !supabaseUrl.trim() || !supabaseKey.trim() || !supabasePat.trim()}>
                    {supabaseMigrating ? "Saving…" : "Save & Re-Migrate"}
                  </button>
                </div>
              </div>
            )}
            {settingsTab === "ai" && (
              <div>
                <h2>AI Provider</h2>
                <p className="setup-subtitle">Current: <strong>{providerLabel || "Not configured"}</strong></p>
                <div className="setup-actions" style={{ marginTop: "1rem" }}>
                  <button className="btn-secondary" onClick={() => setSetupStep("done")}>Cancel</button>
                  <button className="btn-primary" onClick={() => setSetupStep("pick_provider")}>Change Provider</button>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    );
  }

  // ── Render: loading ─────────────────────────────────────────────────────────
  if (setupStep === "loading") {
    return (
      <div className="admin-chat-page">
        <SetupTopBar label="Admin AI" user={sessionUser.display_name} onBack={onBackToPOS} />
        <div className="setup-center"><span className="setup-loading">Loading…</span></div>
      </div>
    );
  }

  // ── Render: pick provider ───────────────────────────────────────────────────
  if (setupStep === "pick_provider") {
    return (
      <div className="admin-chat-page">
        <SetupTopBar label="Admin AI — Setup" user={sessionUser.display_name} onBack={onBackToPOS} />
        <div className="setup-center">
          <div className="setup-card">
            <h2>Choose AI Provider</h2>
            <p className="setup-subtitle">Select the AI service to power the admin assistant.</p>
            <div className="provider-choice-grid">
              <button className="provider-choice-btn" onClick={() => setSetupStep("anthropic_key")}>
                <span className="provider-choice-icon">◆</span>
                <span className="provider-choice-name">Anthropic</span>
                <span className="provider-choice-desc">Claude — claude-sonnet-4-6</span>
              </button>
              <button className="provider-choice-btn" onClick={() => setSetupStep("openai_url_key")}>
                <span className="provider-choice-icon">⬡</span>
                <span className="provider-choice-name">OpenAI Compatible</span>
                <span className="provider-choice-desc">OpenAI, Groq, Ollama, LM Studio, vLLM</span>
              </button>
            </div>
            {config?.provider && (
              <button className="setup-skip-btn" onClick={() => setSetupStep("done")}>
                Keep current: {providerLabel}
              </button>
            )}
          </div>
        </div>
      </div>
    );
  }

  // ── Render: Anthropic key ───────────────────────────────────────────────────
  if (setupStep === "anthropic_key") {
    return (
      <div className="admin-chat-page">
        <SetupTopBar label="Anthropic Setup" user={sessionUser.display_name} onBack={() => setSetupStep("pick_provider")} />
        <div className="setup-center">
          <div className="setup-card">
            <h2>Anthropic API Key</h2>
            <p className="setup-subtitle">From <strong>console.anthropic.com</strong>. Model: <code>claude-sonnet-4-6</code></p>
            <label className="setup-label">API Key
              <input type="password" className="setup-input" placeholder="sk-ant-api03-…"
                value={anthropicKey} onChange={e => setAnthropicKey(e.target.value)}
                onKeyDown={e => e.key === "Enter" && handleSaveAnthropic()} autoFocus />
            </label>
            {anthropicError && <p className="setup-error">{anthropicError}</p>}
            <div className="setup-actions">
              <button className="btn-secondary" onClick={() => setSetupStep("pick_provider")}>Back</button>
              <button className="btn-primary" onClick={handleSaveAnthropic} disabled={savingAnthropic || !anthropicKey.trim()}>
                {savingAnthropic ? "Saving…" : "Save & Continue"}
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // ── Render: OpenAI URL + key ────────────────────────────────────────────────
  if (setupStep === "openai_url_key" || setupStep === "openai_validating") {
    return (
      <div className="admin-chat-page">
        <SetupTopBar label="OpenAI Setup" user={sessionUser.display_name} onBack={() => setSetupStep("pick_provider")} />
        <div className="setup-center">
          <div className="setup-card">
            <h2>OpenAI-Compatible Provider</h2>
            <label className="setup-label">Base URL
              <input type="text" className="setup-input"
                placeholder="https://api.openai.com/v1"
                value={openaiBaseUrl} onChange={e => setOpenaiBaseUrl(e.target.value)} disabled={validating} />
            </label>
            <div className="setup-url-examples">
              {["https://api.openai.com/v1", "https://api.groq.com/openai/v1", "http://localhost:11434/v1", "http://localhost:1234/v1"].map(u => (
                <button key={u} className="setup-url-chip" onClick={() => setOpenaiBaseUrl(u)} disabled={validating}>
                  {u.replace(/https?:\/\//, "").split("/")[0]}
                </button>
              ))}
            </div>
            <label className="setup-label">API Key
              <input type="password" className="setup-input" placeholder="sk-…"
                value={openaiKey} onChange={e => setOpenaiKey(e.target.value)} disabled={validating}
                onKeyDown={e => e.key === "Enter" && handleValidateOpenai()} />
            </label>
            {validateError && <p className="setup-error">{validateError}</p>}
            <div className="setup-actions">
              <button className="btn-secondary" onClick={() => setSetupStep("pick_provider")} disabled={validating}>Back</button>
              <button className="btn-primary" onClick={handleValidateOpenai} disabled={validating || !openaiBaseUrl.trim() || !openaiKey.trim()}>
                {validating ? "Connecting…" : "Connect & Fetch Models"}
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // ── Render: OpenAI model picker ─────────────────────────────────────────────
  if (setupStep === "openai_pick_model") {
    return (
      <div className="admin-chat-page">
        <SetupTopBar label="Select Model" user={sessionUser.display_name} onBack={() => setSetupStep("openai_url_key")} />
        <div className="setup-center">
          <div className="setup-card setup-card-wide">
            <h2>Select a Model</h2>
            <p className="setup-subtitle">
              {modelList.length > 0
                ? `${modelList.length} model(s) available from ${openaiBaseUrl.replace(/https?:\/\//, "").split("/")[0]}`
                : "No models listed — enter a name manually."}
            </p>
            {modelList.length > 0 ? (
              <div className="model-list">
                {modelList.map(m => (
                  <button key={m.id} className={`model-list-item ${selectedModel === m.id ? "model-list-item-active" : ""}`} onClick={() => setSelectedModel(m.id)}>
                    <span className="model-list-id">{m.id}</span>
                    {selectedModel === m.id && <span className="model-list-check">✓</span>}
                  </button>
                ))}
              </div>
            ) : (
              <label className="setup-label">Model name
                <input type="text" className="setup-input" placeholder="gpt-4o, llama3, mistral…"
                  value={selectedModel} onChange={e => setSelectedModel(e.target.value)} autoFocus />
              </label>
            )}
            {validateError && <p className="setup-error">{validateError}</p>}
            <div className="setup-actions">
              <button className="btn-secondary" onClick={() => setSetupStep("openai_url_key")}>Back</button>
              <button className="btn-primary" onClick={handleSaveOpenai} disabled={savingOpenai || !selectedModel.trim()}>
                {savingOpenai ? "Saving…" : "Use This Model"}
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // ── Render: main chat (done state) ──────────────────────────────────────────
  return (
    <div className="admin-chat-page">
      <TopBar
        label="Admin Intelligence"
        user={sessionUser.display_name}
        providerLabel={providerLabel}
        showKpi={showKpi}
        onToggleKpi={() => setShowKpi(v => !v)}
        onClearChat={() => { setMessages([]); setHistory([]); setLastUserMsg(""); }}
        onSettings={() => { setSettingsTab("sync"); setSetupStep("settings"); }}
        onBack={onBackToPOS}
      />

      <div className="admin-chat-body">
        {/* ── Chat area ─────────────────────────────────────────────────────── */}
        <div className="admin-chat-main">
          <div className="chat-messages">
            {messages.length === 0 && (
              <WelcomePanel
                userName={sessionUser.display_name}
                kpi={kpi}
                currencyExp={DEVICE.currency_exponent}
                onChip={(text) => handleSend(text)}
              />
            )}

            {messages.map(msg => (
              <ChatBubble
                key={msg.id}
                msg={msg}
                onUndo={msg.undoId ? () => handleUndo(msg.undoId!, msg.id) : undefined}
              />
            ))}

            {chatState === "thinking" && (
              <ThinkingBubble lastMessage={lastUserMsg} />
            )}
            <div ref={bottomRef} />
          </div>

          {/* ── Footer: chips + input ──────────────────────────────────────── */}
          <div className="chat-footer-area">
            {messages.length === 0 && chatState === "idle" && (
              <QuickChipsBar onSelect={(text) => handleSend(text)} />
            )}

            <div className="chat-input-row">
              <textarea
                ref={textareaRef}
                className="chat-input-v2"
                placeholder={chatState === "confirm" ? "Confirm or cancel the action above…" : "Ask anything about your business…"}
                value={input}
                onChange={e => setInput(e.target.value)}
                onKeyDown={handleKeyDown}
                disabled={chatState !== "idle"}
                rows={1}
              />
              <button
                className="chat-send-btn-v2"
                onClick={() => handleSend()}
                disabled={!input.trim() || chatState !== "idle"}
              >
                ↑
              </button>
            </div>
            <div className="chat-input-hint">
              Enter to send · Shift+Enter for new line
              {messages.length > 0 && (
                <button className="chat-clear-link" onClick={() => { setMessages([]); setHistory([]); setLastUserMsg(""); }}>
                  · Clear chat
                </button>
              )}
            </div>
          </div>
        </div>

        {/* ── KPI Sidebar ───────────────────────────────────────────────────── */}
        {showKpi && (
          <KpiSidebar
            kpi={kpi}
            currencyExp={DEVICE.currency_exponent}
            onRefresh={fetchKpi}
          />
        )}
      </div>

      {/* ── Confirm action modal ──────────────────────────────────────────────── */}
      {chatState === "confirm" && pendingAction && (
        <ConfirmActionModal
          preview={pendingAction.preview}
          onConfirm={handleConfirm}
          onCancel={handleCancel}
        />
      )}
    </div>
  );
}
