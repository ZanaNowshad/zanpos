import type { LucideProps } from "lucide-react";
import type { FC } from "react";

// ─── Tab identifier — shared with nav ──────────────────────────────────────────
export type OfficeTab =
  | "assistant"
  | "products" | "categories" | "users" | "reports"
  | "inventory" | "customers" | "settings" | "audit" | "devices"
  | "cashier" | "eod" | "deliveries";

// ─── Chat types ─────────────────────────────────────────────────────────────────
export interface DisplayMessage {
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

export type ChatState = "idle" | "thinking" | "confirm";

export interface ToolCallEntry {
  id: string;
  name: string;
  startTime: number;
  status: "running" | "done";
  duration?: number;
}

export interface ToolMetaEntry {
  Icon: FC<LucideProps>;
  label: string;
  color: string;
}

// ─── Setup wizard ──────────────────────────────────────────────────────────────
export type SetupStep =
  | "loading" | "settings" | "pick_provider"
  | "anthropic_key" | "openai_url_key" | "openai_validating" | "openai_pick_model"
  | "gemini_key" | "gemini_validating" | "gemini_pick_model" | "done";

// ─── KPI ────────────────────────────────────────────────────────────────────────
export interface KpiSnapshot {
  loading: boolean;
  error: string | null;
  today: import("../types").TodaySummary | null;
  lowStockCount: number;
  outOfStockCount: number;
  sync: import("../types").SyncStatus | null;
}

// ─── Quick-action prompts ───────────────────────────────────────────────────────
export const QUICK_ACTIONS = [
  { label: "📊 Today's sales", prompt: "Give me today's sales summary" },
  { label: "⚠️ Low stock", prompt: "Which products are low on stock?" },
  { label: "💵 Cash drawer", prompt: "Show me the current cash drawer status" },
  { label: "🏆 Top products", prompt: "What are the top selling products this month?" },
  { label: "🔄 Recent refunds", prompt: "Show me recent refunds" },
  { label: "🕐 Shift history", prompt: "Show me the last 5 shifts" },
  { label: "🔗 Audit chain", prompt: "Check the audit log chain integrity" },
  { label: "☁️ Sync status", prompt: "What is the sync status?" },
];

// ─── Re‑export ToolPreview from types ───────────────────────────────────────────
export type ToolPreview = import("../types").ToolPreview;
