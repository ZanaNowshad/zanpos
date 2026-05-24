import React, { useState, useRef, useEffect, useCallback } from "react";
import type {
  SessionUser,
  ChatMessage,
  StreamEvent,
  ToolPreview,
  ProviderConfig,
  ModelInfo,
  TodaySummary,
  StockLevel,
  SyncStatus,
} from "../types";
import { DEVICE } from "../types";
import { Channel } from "@tauri-apps/api/core";
import {
  adminGetProviderConfig,
  adminSetAnthropic,
  adminValidateOpenai,
  adminSetOpenai,
  adminSetupSupabase,
  adminGetSupabaseStatus,
  aiChatStream,
  aiExecuteAction,
  aiCancelAction,
  aiUndoAction,
  aiSaveMessage,
  aiLoadHistory,
  aiClearHistory,
  reportToday,
  inventoryGetLevels,
  syncStatus,
} from "../tauri/commands";
import ConfirmActionModal from "../components/ConfirmActionModal";
import { clearAdminChat } from "../adminChatClear";

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
  toolCalls?: ToolCallEntry[];
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

// ─── Tool-call tracking ────────────────────────────────────────────────────────

interface ToolCallEntry {
  id: string;
  name: string;
  startTime: number;
  status: "running" | "done";
  duration?: number; // ms
}

const TOOL_META: Record<string, { icon: string; label: string; color: string }> = {
  get_today_summary:       { icon: "📊", label: "Today's Summary",        color: "#6366f1" },
  list_products:           { icon: "🏷",  label: "Product List",           color: "#0ea5e9" },
  search_products:         { icon: "🔍", label: "Product Search",          color: "#0ea5e9" },
  get_product:             { icon: "🏷",  label: "Product Details",        color: "#0ea5e9" },
  update_product_price:    { icon: "💲", label: "Updating Price",          color: "#f59e0b" },
  set_product_active:      { icon: "🔄", label: "Toggling Product",        color: "#f59e0b" },
  update_product_name:     { icon: "✏",  label: "Renaming Product",        color: "#f59e0b" },
  get_stock_levels:        { icon: "📦", label: "Stock Levels",            color: "#22c55e" },
  get_low_stock:           { icon: "⚠",  label: "Low Stock Check",         color: "#f59e0b" },
  get_cash_summary:        { icon: "💵", label: "Cash Drawer",             color: "#10b981" },
  get_recent_refunds:      { icon: "↩",  label: "Recent Refunds",          color: "#8b5cf6" },
  get_audit_log:           { icon: "📋", label: "Audit Log",               color: "#ef4444" },
  get_sync_status:         { icon: "☁",  label: "Sync Status",             color: "#0ea5e9" },
  get_daily_report:        { icon: "📅", label: "Daily Report",            color: "#6366f1" },
  get_date_range_report:   { icon: "📈", label: "Date Range Report",       color: "#6366f1" },
  get_top_products:        { icon: "🏆", label: "Top Products",            color: "#f59e0b" },
  get_shift_history:       { icon: "🕐", label: "Shift History",           color: "#8b5cf6" },
  list_categories:         { icon: "📂", label: "Categories",              color: "#0ea5e9" },
  list_safe_drops:         { icon: "💰", label: "Safe Drops",              color: "#10b981" },
  list_no_sale_events:     { icon: "🔒", label: "No-Sale Events",          color: "#ef4444" },
  get_audit_chain_status:  { icon: "🔗", label: "Audit Chain",             color: "#ef4444" },
  get_hourly_sales:        { icon: "⏰", label: "Hourly Sales",            color: "#6366f1" },
  get_sales_by_category:   { icon: "📊", label: "Sales by Category",      color: "#6366f1" },
  get_cashier_performance: { icon: "👤", label: "Cashier Performance",     color: "#8b5cf6" },
  update_reorder_point:    { icon: "📦", label: "Update Reorder Point",    color: "#f59e0b" },
  create_product:          { icon: "➕", label: "Creating Product",        color: "#f59e0b" },
  list_customers:          { icon: "👥", label: "Customer List",           color: "#8b5cf6" },
  get_customer:            { icon: "👤", label: "Customer Details",        color: "#8b5cf6" },
  create_customer:         { icon: "👤", label: "Creating Customer",       color: "#f59e0b" },
  update_customer:         { icon: "✏",  label: "Updating Customer",       color: "#f59e0b" },
  list_deliveries:         { icon: "🚚", label: "Deliveries",              color: "#f59e0b" },
  list_users:              { icon: "👥", label: "Users",                   color: "#8b5cf6" },
  get_stock_movements:     { icon: "📦", label: "Stock Movements",         color: "#22c55e" },
  get_tax_report:          { icon: "🧾", label: "Tax Report",              color: "#6366f1" },
  search_market_prices:    { icon: "🌐", label: "Market Prices",           color: "#0ea5e9" },
  get_exchange_rates:      { icon: "💱", label: "Exchange Rates",          color: "#0ea5e9" },
  get_prayer_times:        { icon: "🕌", label: "Prayer Times",            color: "#0ea5e9" },
  get_bahrain_holidays:    { icon: "🇧🇭", label: "Bahrain Holidays",       color: "#0ea5e9" },
};

function toolMeta(name: string) {
  return TOOL_META[name] ?? {
    icon: "⚙",
    label: name.replace(/_/g, " ").replace(/\b\w/g, c => c.toUpperCase()),
    color: "#6b7280",
  };
}

// ─── Tool Call Card ────────────────────────────────────────────────────────────

function ToolCallCard({ entry }: { entry: ToolCallEntry }) {
  const [elapsed, setElapsed] = useState(0);
  const meta  = toolMeta(entry.name);
  const running = entry.status === "running";

  useEffect(() => {
    if (!running) return;
    setElapsed(0);
    const id = setInterval(() => setElapsed(Date.now() - entry.startTime), 80);
    return () => clearInterval(id);
  }, [running, entry.startTime]);

  return (
    <div className={`ai-tool-card${running ? " ai-tool-card--running" : " ai-tool-card--done"}`}>
      {/* Shimmer sweep while running */}
      {running && <div className="ai-tool-shimmer" />}

      {/* Colored left accent strip */}
      <div className="ai-tool-strip" style={{ background: meta.color }} />

      {/* Icon */}
      <div className="ai-tool-icon">{meta.icon}</div>

      {/* Text body */}
      <div className="ai-tool-body">
        <div className="ai-tool-label" style={{ color: running ? meta.color : undefined }}>
          {running ? `Calling ${meta.label}…` : meta.label}
        </div>
        <div className="ai-tool-raw">{entry.name}</div>
      </div>

      {/* Status */}
      <div className="ai-tool-status">
        {running ? (
          <div className="ai-tool-running-status">
            <span className="ai-tool-spinner" />
            {elapsed > 100 && (
              <span className="ai-tool-elapsed-live">{elapsed}ms</span>
            )}
          </div>
        ) : (
          <span className="ai-tool-done-badge">
            ✓ {entry.duration !== undefined ? `${entry.duration}ms` : "done"}
          </span>
        )}
      </div>
    </div>
  );
}

// ─── Live Activity Bar ────────────────────────────────────────────────────────

type ActivityPhase = "thinking" | "tool" | "streaming";

function LiveActivityBar({
  chatState,
  liveToolCalls,
  tokenCount,
  streamStartTime,
}: {
  chatState: ChatState;
  liveToolCalls: ToolCallEntry[];
  tokenCount: number;
  streamStartTime: number | null;
}) {
  const [, tick] = useState(0);

  useEffect(() => {
    if (chatState === "idle") return;
    const id = setInterval(() => tick(n => n + 1), 100);
    return () => clearInterval(id);
  }, [chatState]);

  if (chatState === "idle") return null;

  const elapsed     = streamStartTime ? Date.now() - streamStartTime : 0;
  const runningTool = liveToolCalls.find(t => t.status === "running");

  const phase: ActivityPhase =
    runningTool   ? "tool"      :
    tokenCount > 0 ? "streaming" :
    "thinking";

  const meta = runningTool ? toolMeta(runningTool.name) : null;

  return (
    <div className={`lab lab--${phase}`}>
      <div className="lab-inner">

        {/* Left: animated indicator */}
        <div className="lab-indicator">
          {phase === "thinking" && (
            <div className="lab-dots">
              <span /><span /><span />
            </div>
          )}
          {phase === "tool" && (
            <div className="lab-ring" style={{ borderTopColor: meta?.color ?? "var(--accent)" }} />
          )}
          {phase === "streaming" && (
            <div className="lab-wave">
              <span /><span /><span /><span /><span />
            </div>
          )}
        </div>

        {/* Center: text */}
        <div className="lab-text">
          {phase === "thinking" && <span className="lab-status">Thinking…</span>}
          {phase === "tool" && meta && (
            <span className="lab-status">
              <span className="lab-tool-icon">{meta.icon}</span>
              {" Calling "}<strong>{meta.label}</strong>
            </span>
          )}
          {phase === "streaming" && (
            <span className="lab-status">
              Streaming response
              <span className="lab-token-count">{tokenCount} tokens</span>
            </span>
          )}
        </div>

        {/* Right: elapsed + cursor pip */}
        <div className="lab-right">
          {elapsed > 300 && (
            <span className="lab-elapsed">{(elapsed / 1000).toFixed(1)}s</span>
          )}
          {phase === "streaming" && <span className="lab-blink-dot" />}
        </div>

      </div>
    </div>
  );
}

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

function stripXmlArtifacts(text: string): string {
  // Remove <tool_call>...</tool_call> blocks that leaked through from models
  // that don't support native function calling
  return text
    .replace(/<tool_call>[\s\S]*?<\/tool_call>/g, "")
    .replace(/<function=\S+>[\s\S]*?<\/function>/g, "")
    .trim();
}

const MarkdownContent = React.memo(function MarkdownContent({ text }: { text: string }) {
  const nodes = React.useMemo(() => parseMarkdown(stripXmlArtifacts(text)), [text]);
  return <div className="md-body">{nodes}</div>;
});

function parseMarkdown(text: string): React.ReactNode[] {
  const lines = text.split("\n");
  const result: React.ReactNode[] = [];
  let i = 0;
  let key = 0;

  while (i < lines.length) {
    const startI = i; // guard: detect if nothing consumed this iteration
    const line = lines[i];

    // ── Blank line ────────────────────────────────────────────────────────────
    // Check blank FIRST — prevents fall-through to paragraph with empty line
    if (line.trim() === "") {
      i++;
      continue;
    }

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
    if (paraLines.length > 0) {
      result.push(
        <p key={key++} className="md-p">{inlineMarkdown(paraLines.join(" "))}</p>
      );
    }

    // Safety guard: if nothing consumed this line, force-advance to avoid
    // an infinite loop on any input pattern not matched above.
    if (i === startI) {
      i++;
    }
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

// ─── Subcomponents ────────────────────────────────────────────────────────────

function TopBar({
  label, user, providerLabel, showKpi, onToggleKpi,
  onSettings,
}: {
  label: string; user: string; providerLabel: string;
  showKpi: boolean; onToggleKpi: () => void;
  onClearChat: () => void; onSettings: () => void; onBack: () => void;
}) {
  return (
    <div className="admin-chat-topbar">
      {/* Logo */}
      <div className="admin-topbar-logo">
        <span className="admin-topbar-logo-text">ZAN<span>POS</span></span>
      </div>
      <span className="admin-topbar-page">{label}</span>

      {/* Center search hint */}
      <div className="admin-topbar-search">
        <span className="admin-topbar-search-ph">Ask AI anything about your business…</span>
        <span className="admin-topbar-search-kbd">Ctrl /</span>
      </div>

      {/* Right actions */}
      <div className="topbar-actions">
        <button
          className={`topbar-icon-btn ${showKpi ? "topbar-icon-btn-active" : ""}`}
          onClick={onToggleKpi}
          title="Toggle Live Snapshot"
        >📊</button>
        <button className="topbar-icon-btn" onClick={onSettings} title="Settings">⚙</button>
        <div className="admin-topbar-user">
          <div className="admin-topbar-avatar">{user.charAt(0).toUpperCase()}</div>
          <div className="admin-topbar-user-info">
            <span className="admin-topbar-user-name">{user}</span>
            {providerLabel && <span className="admin-chat-provider-badge">{providerLabel}</span>}
          </div>
        </div>
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

function WelcomePanel({ userName, onChip }: {
  userName: string;
  kpi: KpiSnapshot;
  currencyExp: number;
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
          <div className="welcome-subtitle">I'm your AI assistant. Ask me anything about your store performance, inventory, sales, users, and more.</div>
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

  // Session tracking for history persistence
  const [sessionId, setSessionId] = useState<string>(() => crypto.randomUUID());

  // Chat state
  const [messages, setMessages]           = useState<DisplayMessage[]>([]);
  const [input, setInput]                 = useState("");
  const [chatState, setChatState]         = useState<ChatState>("idle");
  const [history, setHistory]             = useState<ChatMessage[]>([]);
  const [pendingAction, setPendingAction]   = useState<DisplayMessage["pendingAction"] | null>(null);
  const [liveToolCalls, setLiveToolCalls]   = useState<ToolCallEntry[]>([]);
  const liveToolCallsRef                    = useRef<ToolCallEntry[]>([]);
  const [streamingMsgId, setStreamingMsgId] = useState<string | null>(null);
  const [tokenCount, setTokenCount]         = useState(0);
  const tokenCountRef                       = useRef(0);
  const [streamStartTime, setStreamStartTime] = useState<number | null>(null);

  // UI state
  const [showKpi, setShowKpi]             = useState(true);
  const [kpi, setKpi]                     = useState<KpiSnapshot>({
    loading: false, error: null, today: null, lowStockCount: 0, outOfStockCount: 0, sync: null,
  });

  const bottomRef         = useRef<HTMLDivElement>(null);
  const textareaRef       = useRef<HTMLTextAreaElement>(null);
  const assistantMsgIdRef = useRef<string>("");

  const handleClearChat = useCallback(() => {
    clearAdminChat({
      branchId: DEVICE.branch_id,
      userId: sessionUser.user_id,
      newSessionId: () => crypto.randomUUID(),
      clearHistory: aiClearHistory,
      setMessages: () => setMessages([]),
      setHistory: () => setHistory([]),
      setSessionId,
    });
  }, [sessionUser.user_id]);

  // ── Initial load ────────────────────────────────────────────────────────────
  useEffect(() => {
    Promise.all([
      adminGetSupabaseStatus().catch(() => ({ configured: false } as { configured: boolean })),
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
    }).catch((e) => {
      console.error("[AdminChat] init failed:", e);
      // Fallback: show provider picker so user can at least configure
      setSetupStep("pick_provider");
    });
  }, []);

  // ── Load history when setup is done ────────────────────────────────────────
  useEffect(() => {
    if (setupStep !== "done") return;
    aiLoadHistory(DEVICE.branch_id, sessionUser.user_id)
      .then(loaded => {
        if (loaded.length === 0) return;
        setMessages(loaded
          .filter(m => m.role === "user" || m.role === "assistant")
          .map(m => ({
            id: crypto.randomUUID(),
            role: m.role as "user" | "assistant",
            text: m.content,
            timestamp: new Date(m.created_at.replace(" ", "T")),
          }))
        );
        setHistory(loaded
          .filter(m => m.role === "user" || m.role === "assistant")
          .map(m => ({ role: m.role as "user" | "assistant", content: m.content }))
        );
        const lastSessionId = loaded[loaded.length - 1]?.session_id;
        if (lastSessionId) setSessionId(lastSessionId);
      })
      .catch(() => {});
  }, [setupStep, sessionUser.user_id]);

  // ── Fetch KPI when chat is ready ────────────────────────────────────────────
  // Sequential (not parallel) to avoid spiking Rust thread pool + SQLite
  // connections all at once on page open, which stresses WebView2 memory.
  const fetchKpi = useCallback(async () => {
    setKpi(prev => ({ ...prev, loading: true, error: null }));
    try {
      const today = new Date().toISOString().slice(0, 10);
      const todaySummary = await reportToday(DEVICE.branch_id, today).catch(() => null);
      const levels = await inventoryGetLevels().catch(() => [] as StockLevel[]);
      const syncStat = await syncStatus().catch(() => null);
      const lowStockCount  = levels.filter(l => l.is_low_stock && !l.is_out_of_stock).length;
      const outOfStockCount = levels.filter(l => l.is_out_of_stock).length;
      setKpi({ loading: false, error: null, today: todaySummary, lowStockCount, outOfStockCount, sync: syncStat });
    } catch (e) {
      setKpi(prev => ({ ...prev, loading: false, error: String(e) }));
    }
  }, []);

  useEffect(() => {
    if (setupStep !== "done") return;
    // Session-based only: no history loaded from DB.
    // Fetch KPI after a short delay to avoid spiking memory on page open.
    setTimeout(fetchKpi, 300);
  }, [setupStep, fetchKpi]);

  // ── Auto-scroll ─────────────────────────────────────────────────────────────
  // Use "auto" (instant) during streaming to avoid queuing hundreds of smooth-
  // scroll animations per token which can hold DOM references and leak memory.
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: chatState === "idle" ? "smooth" : "auto" });
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

  // ── Chat send (streaming) ────────────────────────────────────────────────────
  const handleSend = async (overrideText?: string) => {
    const text = (overrideText ?? input).trim();
    if (!text || chatState !== "idle") return;
    setInput("");
    addMessage({ role: "user", text });
    aiSaveMessage(sessionId, DEVICE.branch_id, sessionUser.user_id, "user", text, "text").catch(() => {});
    const rawHistory: ChatMessage[] = [...history, { role: "user", content: text }];
    // Keep bounded — 60 entries max
    const newHistory: ChatMessage[] = rawHistory.length > 60 ? rawHistory.slice(rawHistory.length - 60) : rawHistory;
    setHistory(newHistory);
    setChatState("thinking");
    // Reset live tool calls + streaming metrics for this new response
    liveToolCallsRef.current = [];
    setLiveToolCalls([]);
    tokenCountRef.current = 0;
    setTokenCount(0);
    setStreamStartTime(Date.now());

    // Push empty assistant bubble (will be filled by tokens)
    const assistantMsg = addMessage({ role: "assistant", text: "" });
    assistantMsgIdRef.current = assistantMsg.id;
    setStreamingMsgId(assistantMsg.id);

    try {
      const onEvent = new Channel<StreamEvent>();
      let finalText = "";

      onEvent.onmessage = (event: StreamEvent) => {
        if (event.type === "token") {
          finalText += event.text;
          tokenCountRef.current += 1;
          setTokenCount(tokenCountRef.current);
          const currentId = assistantMsgIdRef.current;
          setMessages(prev =>
            prev.map(m => m.id === currentId ? { ...m, text: finalText } : m)
          );
        } else if (event.type === "tool_start") {
          // Add live tool call card
          const entry: ToolCallEntry = {
            id: `${event.name}-${Date.now()}`,
            name: event.name,
            startTime: Date.now(),
            status: "running",
          };
          liveToolCallsRef.current = [...liveToolCallsRef.current, entry];
          setLiveToolCalls([...liveToolCallsRef.current]);
        } else if (event.type === "tool_done") {
          // Mark tool call as done with duration
          const now = Date.now();
          liveToolCallsRef.current = liveToolCallsRef.current.map(e =>
            e.name === event.name && e.status === "running"
              ? { ...e, status: "done" as const, duration: now - e.startTime }
              : e
          );
          setLiveToolCalls([...liveToolCallsRef.current]);
        } else if (event.type === "mutation_pending") {
          const actionData = {
            action_id: event.action_id,
            tool_name: event.tool_name,
            preview: event.preview,
            expires_at: event.expires_at,
            assistant_text: event.assistant_text,
          };
          const currentId = assistantMsgIdRef.current;
          setMessages(prev =>
            prev.map(m =>
              m.id === currentId
                ? { ...m, text: event.assistant_text || "I'd like to make the following change:", pendingAction: actionData }
                : m
            )
          );
          setPendingAction(actionData);
          setChatState("confirm");
        } else if (event.type === "done") {
          // Stamp stored tool calls into the assistant message, reset live state
          const storedCalls = [...liveToolCallsRef.current];
          if (storedCalls.length > 0) {
            const currentId = assistantMsgIdRef.current;
            setMessages(prev =>
              prev.map(m => m.id === currentId ? { ...m, toolCalls: storedCalls } : m)
            );
          }
          liveToolCallsRef.current = [];
          setLiveToolCalls([]);
          setStreamingMsgId(null);
          setStreamStartTime(null);
          setChatState("idle");
          if (finalText) {
            // Keep in-memory context bounded at 60 entries (30 exchanges)
            setHistory(prev => {
              const next = [...prev, { role: "assistant" as const, content: finalText }];
              return next.length > 60 ? next.slice(next.length - 60) : next;
            });
            aiSaveMessage(sessionId, DEVICE.branch_id, sessionUser.user_id, "assistant", finalText, "text").catch(() => {});
          }
        } else if (event.type === "error") {
          const currentId = assistantMsgIdRef.current;
          setMessages(prev =>
            prev.map(m =>
              m.id === currentId
                ? { ...m, role: "system" as const, text: `Error: ${event.message}` }
                : m
            )
          );
          liveToolCallsRef.current = [];
          setLiveToolCalls([]);
          setStreamingMsgId(null);
          setStreamStartTime(null);
          setChatState("idle");
        }
      };

      // Cap history to last 40 messages to avoid unbounded IPC payload growth
      const cappedHistory = history.length > 40 ? history.slice(history.length - 40) : history;
      await aiChatStream(
        {
          history: cappedHistory,
          message: text,
          user_id: sessionUser.user_id,
          branch_id: DEVICE.branch_id,
          currency_exponent: DEVICE.currency_exponent,
        },
        onEvent
      );
    } catch (e) {
      const currentId = assistantMsgIdRef.current;
      setMessages(prev =>
        prev.map(m =>
          m.id === currentId
            ? { ...m, role: "system" as const, text: `Error: ${String(e)}` }
            : m
        )
      );
      liveToolCallsRef.current = [];
      setLiveToolCalls([]);
      setStreamingMsgId(null);
      setStreamStartTime(null);
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

  // ── Render: initial loading ─────────────────────────────────────────────────
  if (setupStep === "loading") {
    return (
      <div className="admin-chat-page">
        <div className="setup-center">
          <div className="setup-loading-spinner">
            <div className="spinner-ring" />
            <p>Loading…</p>
          </div>
        </div>
      </div>
    );
  }

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
        onClearChat={handleClearChat}
        onSettings={() => { setSettingsTab("sync"); setSetupStep("settings"); }}
        onBack={onBackToPOS}
      />

      <div className="admin-chat-body">
        {/* ── Left sidebar nav ──────────────────────────────────────────────── */}
        <aside className="admin-chat-sidebar">
          <button className="admin-sidebar-back" onClick={onBackToPOS}>← Back to POS</button>
          <nav className="admin-sidebar-nav">
            {[
              { icon: "✦", label: "AI Assistant", active: true },
              { icon: "📊", label: "Today's Sales", prompt: "Give me today's sales summary" },
              { icon: "⚠", label: "Low Stock Alerts", prompt: "Which products are low on stock?" },
              { icon: "🏆", label: "Top Products", prompt: "What are the top selling products this month?" },
              { icon: "↩", label: "Recent Refunds", prompt: "Show me recent refunds" },
              { icon: "🕐", label: "Shift History", prompt: "Show me the last 5 shifts" },
              { icon: "🔗", label: "Audit Chain", prompt: "Check the audit log chain integrity" },
              { icon: "☁", label: "Sync Status", prompt: "What is the sync status?" },
            ].map(item => (
              <button
                key={item.label}
                className={`admin-sidebar-item${item.active ? " active" : ""}`}
                onClick={item.prompt ? () => handleSend(item.prompt) : undefined}
              >
                <span className="admin-sidebar-icon">{item.icon}</span>
                <span>{item.label}</span>
              </button>
            ))}
          </nav>
        </aside>

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

            {messages.map((msg, i) => {
              const msgDate = msg.timestamp.toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" });
              const prevDate = i > 0 ? messages[i-1].timestamp.toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" }) : null;
              const showSep = i === 0 || msgDate !== prevDate;
              const isStreaming = msg.id === streamingMsgId;
              return (
                <React.Fragment key={msg.id}>
                  {showSep && <div className="ai-date-sep">{msgDate}</div>}
                  <ChatBubble
                    msg={msg}
                    isStreaming={isStreaming}
                    onUndo={msg.undoId ? () => handleUndo(msg.undoId!, msg.id) : undefined}
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

          {/* ── Live activity bar ─────────────────────────────────────────── */}
          <LiveActivityBar
            chatState={chatState}
            liveToolCalls={liveToolCalls}
            tokenCount={tokenCount}
            streamStartTime={streamStartTime}
          />

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
                <button className="chat-clear-link" onClick={handleClearChat}>
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
