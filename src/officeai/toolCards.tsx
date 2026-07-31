import { useEffect, useState } from "react";
import {
  AlarmClock, AlertTriangle, ArrowLeftRight, Banknote, BarChart2, BookOpen,
  Building2, CalendarDays, ClipboardList, Clock, Cloud, Database,
  Flag, FolderOpen, Globe, HeartPulse, Link, Lock, Monitor, Package, PanelsTopLeft,
  Pencil, PlusCircle, QrCode, Receipt, RefreshCw, Search, Settings,
  ShoppingCart, Store, Tag, TrendingUp, Truck, Undo2, User,
  Users, Wallet, Trophy,
} from "lucide-react";
import type { ToolCallEntry, ToolMetaEntry, ChatState } from "./officeAiTypes";
import { useLanguage } from "../hooks/useLanguage";
import { isKnownOfficeAiTool, officeAiToolLabel } from "../i18n/officeAiToolStrings";
import { officeAiTranslator } from "../i18n/officeAiStrings";

export { QUICK_ACTIONS } from "./officeAiTypes";

// T16: colors reference CSS design-system tokens (no hardcoded hex).
// T17: icon field uses Lucide React component references.
export const TOOL_META: Record<string, ToolMetaEntry> = {
  get_today_summary:          { Icon: BarChart2,       label: "Today's Summary",          color: "var(--ai-indigo)" },
  list_products:              { Icon: Tag,             label: "Product List",             color: "var(--info)"      },
  search_products:            { Icon: Search,          label: "Product Search",           color: "var(--info)"      },
  get_product:                { Icon: Tag,             label: "Product Details",          color: "var(--info)"      },
  update_product_price:       { Icon: Pencil,          label: "Updating Price",           color: "var(--warning)"   },
  set_product_active:         { Icon: RefreshCw,       label: "Toggling Product",         color: "var(--warning)"   },
  update_product_name:        { Icon: Pencil,          label: "Renaming Product",         color: "var(--warning)"   },
  get_stock_levels:           { Icon: Package,         label: "Stock Levels",             color: "var(--success)"   },
  get_low_stock:              { Icon: AlertTriangle,   label: "Low Stock Check",          color: "var(--warning)"   },
  get_cash_summary:           { Icon: Banknote,        label: "Cash Drawer",              color: "var(--success)"   },
  get_recent_refunds:         { Icon: Undo2,           label: "Recent Refunds",           color: "var(--ai-purple)" },
  get_audit_log:              { Icon: ClipboardList,   label: "Audit Log",                color: "var(--error)"     },
  get_sync_status:            { Icon: Cloud,           label: "Sync Status",              color: "var(--info)"      },
  get_daily_report:           { Icon: CalendarDays,    label: "Daily Report",             color: "var(--ai-indigo)" },
  get_date_range_report:      { Icon: TrendingUp,      label: "Date Range Report",        color: "var(--ai-indigo)" },
  get_top_products:           { Icon: Trophy,          label: "Top Products",             color: "var(--warning)"   },
  get_shift_history:          { Icon: Clock,           label: "Shift History",            color: "var(--ai-purple)" },
  list_categories:            { Icon: FolderOpen,      label: "Categories",               color: "var(--info)"      },
  list_safe_drops:            { Icon: Wallet,          label: "Safe Drops",               color: "var(--success)"   },
  list_no_sale_events:        { Icon: Lock,            label: "No-Sale Events",           color: "var(--error)"     },
  get_audit_chain_status:     { Icon: Link,            label: "Audit Chain",              color: "var(--error)"     },
  get_hourly_sales:           { Icon: AlarmClock,      label: "Hourly Sales",             color: "var(--ai-indigo)" },
  get_sales_by_category:      { Icon: BarChart2,       label: "Sales by Category",        color: "var(--ai-indigo)" },
  get_cashier_performance:    { Icon: User,            label: "Cashier Performance",      color: "var(--ai-purple)" },
  update_reorder_point:       { Icon: Package,         label: "Update Reorder Point",     color: "var(--warning)"   },
  create_product:             { Icon: PlusCircle,      label: "Creating Product",         color: "var(--warning)"   },
  list_customers:             { Icon: Users,           label: "Customer List",            color: "var(--ai-purple)" },
  get_customer:               { Icon: User,            label: "Customer Details",         color: "var(--ai-purple)" },
  create_customer:            { Icon: User,            label: "Creating Customer",        color: "var(--warning)"   },
  update_customer:            { Icon: Pencil,          label: "Updating Customer",        color: "var(--warning)"   },
  list_deliveries:            { Icon: Truck,           label: "Deliveries",               color: "var(--warning)"   },
  list_users:                 { Icon: Users,           label: "Users",                    color: "var(--ai-purple)" },
  get_stock_movements:        { Icon: Package,         label: "Stock Movements",          color: "var(--success)"   },
  get_tax_report:             { Icon: Receipt,         label: "Tax Report",               color: "var(--ai-indigo)" },
  search_market_prices:       { Icon: Globe,           label: "Market Prices",            color: "var(--info)"      },
  get_exchange_rates:         { Icon: ArrowLeftRight,  label: "Exchange Rates",           color: "var(--info)"      },
  get_prayer_times:           { Icon: Building2,       label: "Prayer Times",             color: "var(--info)"      },
  get_bahrain_holidays:       { Icon: Flag,            label: "Bahrain Holidays",         color: "var(--info)"      },
  list_roles:                 { Icon: Users,           label: "Roles",                    color: "var(--ai-purple)" },
  list_tax_rules:             { Icon: Receipt,         label: "Tax Rules",                color: "var(--ai-indigo)" },
  get_store_settings:         { Icon: Store,           label: "Store Settings",           color: "var(--ai-indigo)" },
  get_business_rules:         { Icon: Settings,        label: "Business Rules",           color: "var(--ai-indigo)" },
  list_devices:               { Icon: Monitor,         label: "Devices",                  color: "var(--info)"      },
  get_session_timeout:        { Icon: Clock,           label: "Session Timeout",          color: "var(--ai-purple)" },
  create_category:            { Icon: FolderOpen,      label: "Creating Category",        color: "var(--warning)"   },
  update_category:            { Icon: FolderOpen,      label: "Updating Category",        color: "var(--warning)"   },
  create_user:                { Icon: User,            label: "Creating User",            color: "var(--warning)"   },
  update_user:                { Icon: User,            label: "Updating User",            color: "var(--warning)"   },
  create_tax_rule:            { Icon: Receipt,         label: "Creating Tax Rule",        color: "var(--warning)"   },
  update_tax_rule:            { Icon: Receipt,         label: "Updating Tax Rule",        color: "var(--warning)"   },
  update_product_full:        { Icon: Tag,             label: "Updating Product",         color: "var(--warning)"   },
  update_store_settings:      { Icon: Store,           label: "Updating Store",           color: "var(--warning)"   },
  update_business_rules:      { Icon: Settings,        label: "Updating Business Rules",  color: "var(--warning)"   },
  confirm_delivery_payment:   { Icon: Truck,           label: "Confirming Payment",       color: "var(--warning)"   },
  cancel_delivery:            { Icon: Truck,           label: "Cancelling Delivery",      color: "var(--error)"     },
  backup_database:            { Icon: Database,        label: "Database Backup",          color: "var(--warning)"   },
  smart_barcode_lookup:       { Icon: QrCode,          label: "Scanning Barcode",         color: "var(--info)"      },
  compare_store_prices:       { Icon: ShoppingCart,    label: "Price Comparison",         color: "var(--info)"      },
  bahrain_market_price_check: { Icon: ShoppingCart,    label: "Market Check",             color: "var(--info)"      },
  open_tab:                   { Icon: PanelsTopLeft,   label: "Opening Tab",              color: "var(--info)"      },
  // ZanAI Insights — Phase 1
  get_frequently_bought_together: { Icon: ShoppingCart, label: "Bought Together",        color: "var(--ai-indigo)" },
  get_bundle_suggestions:         { Icon: Tag,           label: "Bundle Ideas",           color: "var(--success)"   },
  get_weekly_forecast:            { Icon: TrendingUp,    label: "Weekly Forecast",        color: "var(--ai-indigo)" },
  get_rfm_segmentation:           { Icon: Users,         label: "RFM Segments",           color: "var(--ai-purple)" },
  get_margin_trend:               { Icon: BarChart2,     label: "Margin Trend",           color: "var(--success)"   },
  get_restock_priority:           { Icon: AlertTriangle, label: "Restock Priority",       color: "var(--warning)"   },
  get_dead_stock_value:           { Icon: Package,       label: "Dead Stock Value",       color: "var(--error)"     },
  get_category_forecast:          { Icon: CalendarDays,  label: "Category Forecast",      color: "var(--ai-indigo)" },
  // Product Quality
  find_duplicate_products:        { Icon: Search,        label: "Find Duplicates",        color: "var(--warning)"   },
  merge_products:                 { Icon: RefreshCw,     label: "Merging Products",       color: "var(--warning)"   },
  // Workflow loader
  load_workflow:                  { Icon: BookOpen,       label: "Loading Workflow",       color: "var(--info)"      },
  // Database maintenance
  reindex_database:               { Icon: Database,      label: "Rebuilding Indexes",     color: "var(--warning)"   },
  force_wal_checkpoint:           { Icon: Database,      label: "WAL Checkpoint",         color: "var(--warning)"   },
  check_foreign_key_integrity:    { Icon: Link,           label: "FK Integrity Check",     color: "var(--info)"      },
  // Ghost barcodes
  list_ghost_barcodes:            { Icon: QrCode,        label: "Ghost Barcodes",         color: "var(--warning)"   },
  resolve_ghost_barcode:          { Icon: QrCode,        label: "Resolving Barcode",      color: "var(--warning)"   },
  // Sync conflicts
  list_sync_conflicts:            { Icon: AlertTriangle, label: "Sync Conflicts",         color: "var(--error)"     },
  resolve_sync_conflict:          { Icon: RefreshCw,     label: "Resolving Conflict",     color: "var(--warning)"   },
  // Database maintenance (advanced)
  run_quick_integrity_check:       { Icon: ClipboardList, label: "Quick Integrity Check",  color: "var(--info)"      },
  get_database_fragmentation:      { Icon: Database,      label: "DB Fragmentation",       color: "var(--info)"      },
  clear_ghost_sync_records:       { Icon: Cloud,          label: "Clean Sync Orphans",     color: "var(--warning)"   },
  run_diagnostics_and_fix:        { Icon: HeartPulse,     label: "Diagnostics & Repair",   color: "var(--warning)"   },
  // Bulk imports
  bulk_import_products:           { Icon: Tag,            label: "Importing Products",     color: "var(--warning)"   },
  bulk_import_categories:         { Icon: FolderOpen,     label: "Importing Categories",   color: "var(--warning)"   },
  // WhatsApp
  send_receipt_via_whatsapp:      { Icon: Receipt,        label: "Sending Receipt PDF",    color: "var(--warning)"   },
};

export function toolMeta(name: string, language: "en" | "ar"): ToolMetaEntry {
  const meta = TOOL_META[name];
  if (meta) {
    return {
      ...meta,
      label: isKnownOfficeAiTool(name) ? officeAiToolLabel(language, name) : meta.label,
    };
  }
  return {
    Icon: Settings,
    label: name.replace(/_/g, " ").replace(/\b\w/g, c => c.toUpperCase()),
    color: "var(--text-dim)",
  };
}

// ─── Tool Call Card ────────────────────────────────────────────────────────────

export function ToolCallCard({ entry }: { entry: ToolCallEntry }) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  const [elapsed, setElapsed] = useState(0);
  const meta  = toolMeta(entry.name, language);
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

      {/* Icon — Lucide component (T17) */}
      <div className="ai-tool-icon"><meta.Icon size={16} /></div>

      {/* Text body */}
      <div className="ai-tool-body">
        <div className="ai-tool-label" style={{ color: running ? meta.color : undefined }}>
          {running ? `${t("calling")} ${meta.label}…` : meta.label}
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
            ✓ {entry.duration !== undefined ? `${entry.duration}ms` : t("done")}
          </span>
        )}
      </div>
    </div>
  );
}

// ─── Live Activity Bar ────────────────────────────────────────────────────────

type ActivityPhase = "thinking" | "tool" | "streaming";

export function LiveActivityBar({
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
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    if (chatState === "idle") return;
    const id = setInterval(() => {
      setElapsed(streamStartTime ? Date.now() - streamStartTime : 0);
    }, 100);
    return () => clearInterval(id);
  }, [chatState, streamStartTime]);

  const runningTool = liveToolCalls.find(t => t.status === "running");
  const phase: ActivityPhase = runningTool ? "tool" : tokenCount > 0 ? "streaming" : "thinking";

  if (chatState === "idle") return null;

  const meta = runningTool ? toolMeta(runningTool.name, language) : null;

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
          {phase === "thinking" && <span className="lab-status">{t("thinking")}</span>}
          {phase === "tool" && meta && (
            <span className="lab-status">
              <span className="lab-tool-icon"><meta.Icon size={14} /></span>
              {` ${t("calling")} `}<strong>{meta.label}</strong>
            </span>
          )}
          {phase === "streaming" && (
            <span className="lab-status">
              {t("streamingResponse")}
              <span className="lab-token-count">{tokenCount} {t("tokens")}</span>
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
