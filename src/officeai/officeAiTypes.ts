import type { LucideProps } from "lucide-react";
import type { FC } from "react";

// ─── Tab identifier — shared with nav ──────────────────────────────────────────
export type OfficeTab =
  | "overview" | "assistant" | "actions" | "workflows" | "health"
  | "conflicts"
  | "insights" | "loyalty"
  | "operations" | "purchasing"
  | "products" | "categories" | "users" | "reports"
  | "inventory" | "customers" | "settings" | "audit" | "devices"
  | "cashier" | "eod" | "deliveries" | "riders"
  | "zanshop";

/**
 * A status signal rendered in the shell header.
 * Extracted from OfficeAIShell so that dead shell could be deleted — a type
 * alias was the only thing keeping the file in the build graph.
 */
export interface OfficePulseItem {
  id: string;
  label: string;
  level: "ok" | "warning" | "critical";
  icon: import("react").ReactNode;
}

export interface OfficeAttentionItem {
  id: string;
  title: string;
  detail: string;
  severity: "critical" | "warning" | "info";
  destination: OfficeTab;
  actionLabel: string;
}

export interface OfficeHomeSignal {
  id: "sales" | "transactions" | "stock" | "approvals";
  label: string;
  value: string;
  detail: string;
}

export interface OfficePulseModel {
  summary: string;
  signals: OfficeHomeSignal[];
  attention: OfficeAttentionItem[];
  allSystemsNormal: boolean;
}

// ─── Chat types ─────────────────────────────────────────────────────────────────
export interface DisplayMessage {
  id: string;
  role: "user" | "assistant" | "system";
  text: string;
  timestamp: Date;
  imagePreviewUrl?: string;
  toolCalls?: ToolCallEntry[];
  pendingAction?: {
    action_id: string;
    tool_name: string;
    preview: ToolPreview;
    expires_at: string;
    assistant_text: string;
  };
  pendingBatchActions?: import("../types").BatchPendingAction[];
  undoId?: string;
  feedbackReady?: boolean;
  aiSessionId?: string;
  suggestedLabel?: string;
  suggestedPrompt?: string;
}

export type ChatState = "idle" | "thinking" | "confirm" | "run_confirm" | "run_executing";

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
  | "pick_provider"
  | "anthropic_key" | "openai_url_key" | "openai_pick_model"
  | "gemini_key" | "gemini_pick_model";

// ─── Bulk Run state ─────────────────────────────────────────────────────────────
export interface RunState {
  runId: string;
  opId: string;
  description: string;
  count: number;
  done: number;
  phase: "preview" | "executing" | "done" | "failed";
  error?: string;
}

// ─── KPI ────────────────────────────────────────────────────────────────────────
export interface KpiSnapshot {
  loading: boolean;
  error: string | null;
  today: import("../types").TodaySummary | null;
  lowStockCount: number;
  outOfStockCount: number;
  sync: import("../types").SyncStatus | null;
  alerts: import("../types").ProactiveAlert[];
}

export interface OfficeAiHealthCard {
  label: string;
  value: string;
  severity: "ok" | "info" | "warning" | "critical";
}

export interface OfficeAiOverviewSnapshot {
  loading: boolean;
  refreshedAt: string | null;
  errors: string[];
  today: import("../types").TodaySummary | null;
  lowStockCount: number;
  outOfStockCount: number;
  sync: import("../types").SyncStatus | null;
  whatsapp: import("../types").WhatsAppStatus | null;
  whatsappUnread: number;
  paymentConfirmations: import("../types").PaymentConfirmation[];
  provider: import("../types").ProviderConfig | null;
  aiEnabled: boolean | null;
  aiConfig: import("../types").AiConfigPayload | null;
  featureToggles: import("../types").FeatureToggles | null;
  health: import("../types").SystemHealthReport | null;
  benefitNumber: string | null;
}

export interface OfficeAiActionQueueItem {
  id: string;
  messageId?: string;
  title: string;
  detail: string;
  status: "pending" | "running" | "done" | "failed";
  kind: "single" | "batch" | "run" | "undo";
  count?: number;
  canConfirm?: boolean;
  canCancel?: boolean;
  canUndo?: boolean;
  severity?: "info" | "warning" | "critical";
}

export interface OfficeAiWorkflowInboxItem {
  id: string;
  kind: "whatsapp" | "catalog" | "payment";
  title: string;
  detail: string;
  status: string;
  timestamp: string | null;
  unread?: boolean;
  mediaType?: string | null;
  source?: import("../types").WaMessage | import("../types").PaymentConfirmation;
}

export interface OfficeAiAuditTimelineItem {
  id: string;
  label: string;
  detail: string;
  status: "proposed" | "approved" | "applied" | "failed" | "undone";
  timestamp: string;
}

export interface OfficeAiCommandShortcut {
  id: string;
  label: string;
  description: string;
  tab?: OfficeTab;
  prompt?: string;
}

// ─── Quick-action prompts ───────────────────────────────────────────────────────
export const QUICK_ACTIONS = [
  { label: "Today's sales", prompt: "Give me today's sales summary" },
  { label: "Low stock", prompt: "Which products are low on stock?" },
  { label: "Cash drawer", prompt: "Show me the current cash drawer status" },
  { label: "Top products", prompt: "What are the top selling products this month?" },
  { label: "Recent refunds", prompt: "Show me recent refunds" },
  { label: "Shift history", prompt: "Show me the last 5 shifts" },
  { label: "Audit chain", prompt: "Check the audit log chain integrity" },
  { label: "Sync status", prompt: "What is the sync status?" },
];

// ─── Re‑export ToolPreview from types ───────────────────────────────────────────
export type ToolPreview = import("../types").ToolPreview;
