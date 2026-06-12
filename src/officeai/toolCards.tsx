import type { ToolCallEntry, ToolMetaEntry } from "./officeAiTypes";
import {
  Search, Tag, Pencil, RefreshCw, Package, AlertTriangle, Banknote, Undo2,
  ClipboardList, Cloud, CalendarDays, TrendingUp, Trophy, Clock, FolderOpen,
  Wallet, Lock, Link, AlarmClock, BarChart2, User, Users, PlusCircle, Truck,
  Receipt, Globe, ArrowLeftRight, Building2, Flag, Store, Settings, Monitor,
  Database, QrCode, ShoppingCart,
  type LucideProps,
} from "lucide-react";
import React from "react";

export const TOOL_META: Record<string, ToolMetaEntry> = {
  get_today_summary:     { Icon: BarChart2,  label: "Today's Summary",    color: "var(--ai-indigo)" },
  list_products:         { Icon: Tag,        label: "Product List",       color: "var(--info)"      },
  search_products:       { Icon: Search,     label: "Product Search",     color: "var(--info)"      },
  get_product:           { Icon: Tag,        label: "Product Details",    color: "var(--info)"      },
  update_product_price:  { Icon: Pencil,     label: "Updating Price",     color: "var(--warning)"   },
  set_product_active:    { Icon: RefreshCw,  label: "Toggling Product",   color: "var(--warning)"   },
  update_product_name:   { Icon: Pencil,     label: "Renaming Product",   color: "var(--warning)"   },
  get_stock_levels:      { Icon: Package,    label: "Stock Levels",       color: "var(--success)"   },
  get_low_stock:         { Icon: AlertTriangle, label: "Low Stock Check", color: "var(--warning)"   },
  get_cash_summary:      { Icon: Banknote,   label: "Cash Drawer",        color: "var(--success)"   },
  get_recent_refunds:    { Icon: Undo2,      label: "Recent Refunds",     color: "var(--ai-purple)" },
  get_audit_log:         { Icon: ClipboardList, label: "Audit Log",       color: "var(--error)"     },
  get_sync_status:       { Icon: Cloud,      label: "Sync Status",        color: "var(--info)"      },
  get_daily_report:      { Icon: CalendarDays, label: "Daily Report",     color: "var(--ai-indigo)" },
  get_date_range_report: { Icon: TrendingUp, label: "Date Range Report",  color: "var(--ai-indigo)" },
  get_top_products:      { Icon: Trophy,     label: "Top Products",       color: "var(--warning)"   },
  get_shift_history:     { Icon: Clock,      label: "Shift History",      color: "var(--ai-purple)" },
  list_categories:       { Icon: FolderOpen, label: "Categories",         color: "var(--info)"      },
  list_safe_drops:       { Icon: Wallet,     label: "Safe Drops",         color: "var(--success)"   },
  list_no_sale_events:   { Icon: Lock,       label: "No-Sale Events",     color: "var(--error)"     },
  get_audit_chain_status:{ Icon: Link,       label: "Audit Chain",        color: "var(--error)"     },
  get_hourly_sales:      { Icon: AlarmClock, label: "Hourly Sales",       color: "var(--ai-indigo)" },
  get_sales_by_category: { Icon: BarChart2,  label: "Sales by Category",  color: "var(--ai-indigo)" },
  get_cashier_performance:{ Icon: User,      label: "Cashier Performance",color: "var(--ai-purple)" },
  update_reorder_point:  { Icon: Package,    label: "Update Reorder Point",color: "var(--warning)"   },
  create_product:        { Icon: PlusCircle, label: "Creating Product",   color: "var(--warning)"   },
  list_customers:        { Icon: Users,      label: "Customer List",      color: "var(--ai-purple)" },
  get_customer:          { Icon: User,       label: "Customer Details",   color: "var(--ai-purple)" },
  create_customer:       { Icon: User,       label: "Creating Customer",  color: "var(--warning)"   },
  update_customer:       { Icon: Pencil,     label: "Updating Customer",  color: "var(--warning)"   },
  list_deliveries:       { Icon: Truck,      label: "Deliveries",         color: "var(--warning)"   },
  list_users:            { Icon: Users,      label: "Users",              color: "var(--ai-purple)" },
  get_stock_movements:   { Icon: Package,    label: "Stock Movements",    color: "var(--success)"   },
  get_tax_report:        { Icon: Receipt,    label: "Tax Report",         color: "var(--ai-indigo)" },
  search_market_prices:  { Icon: Globe,      label: "Market Prices",      color: "var(--info)"      },
  get_exchange_rates:    { Icon: ArrowLeftRight, label: "Exchange Rates", color: "var(--info)"      },
  get_prayer_times:      { Icon: Building2,  label: "Prayer Times",       color: "var(--info)"      },
  get_bahrain_holidays:  { Icon: Flag,       label: "Bahrain Holidays",   color: "var(--info)"      },
  list_roles:            { Icon: Users,      label: "Roles",              color: "var(--ai-purple)" },
  list_tax_rules:        { Icon: Receipt,    label: "Tax Rules",          color: "var(--ai-indigo)" },
  get_store_settings:    { Icon: Store,      label: "Store Settings",     color: "var(--ai-indigo)" },
  get_business_rules:    { Icon: Settings,   label: "Business Rules",     color: "var(--ai-indigo)" },
  list_devices:          { Icon: Monitor,    label: "Devices",            color: "var(--info)"      },
  get_session_timeout:   { Icon: Clock,      label: "Session Timeout",    color: "var(--ai-purple)" },
  create_category:       { Icon: FolderOpen, label: "Creating Category",  color: "var(--warning)"   },
  update_category:       { Icon: FolderOpen, label: "Updating Category",  color: "var(--warning)"   },
  create_user:           { Icon: User,       label: "Creating User",      color: "var(--warning)"   },
  update_user:           { Icon: User,       label: "Updating User",      color: "var(--warning)"   },
  create_tax_rule:       { Icon: Receipt,    label: "Creating Tax Rule",  color: "var(--warning)"   },
  update_tax_rule:       { Icon: Receipt,    label: "Updating Tax Rule",  color: "var(--warning)"   },
  update_product_full:   { Icon: Tag,        label: "Updating Product",   color: "var(--warning)"   },
  update_store_settings: { Icon: Store,      label: "Updating Store",     color: "var(--warning)"   },
  update_business_rules: { Icon: Settings,   label: "Updating Business Rules", color: "var(--warning)" },
  confirm_delivery_payment:{ Icon: Truck,    label: "Confirming Payment", color: "var(--warning)"   },
  cancel_delivery:       { Icon: Truck,      label: "Cancelling Delivery",color: "var(--error)"     },
  backup_database:       { Icon: Database,   label: "Database Backup",    color: "var(--warning)"   },
  smart_barcode_lookup:  { Icon: QrCode,     label: "Scanning Barcode",   color: "var(--info)"      },
  compare_store_prices:  { Icon: ShoppingCart, label: "Price Comparison", color: "var(--info)"      },
  bahrain_market_price_check: { Icon: ShoppingCart, label: "Market Check", color: "var(--info)"    },
  open_tab:              { Icon: Monitor,    label: "Navigate Tab",       color: "var(--info)"      },
};

export function toolMeta(name: string): ToolMetaEntry {
  return TOOL_META[name] ?? {
    Icon: Settings,
    label: name.replace(/_/g, " ").replace(/\b\w/g, c => c.toUpperCase()),
    color: "var(--text-dim)",
  };
}

export function ToolCallCard({ entry }: { entry: ToolCallEntry }) {
  const meta = toolMeta(entry.name);
  const Icon = meta.Icon;
  return (
    <div className={`ai-tool-card ${entry.status}`}>
      <Icon size={14} style={{ color: meta.color }} />
      <span className="ai-tool-label">{meta.label}</span>
      <span className="ai-tool-status">{entry.status === "running" ? "…" : `✓ ${entry.duration}ms`}</span>
    </div>
  );
}

export function LiveActivityBar({ calls }: { calls: ToolCallEntry[] }) {
  if (calls.length === 0) return null;
  return (
    <div className="ai-live-bar">
      {calls.map(c => <ToolCallCard key={c.id} entry={c} />)}
    </div>
  );
}
