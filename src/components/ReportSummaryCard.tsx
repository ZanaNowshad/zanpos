import {
  Banknote, CreditCard, ClipboardList, Receipt, Scooter, Tag, TrendingUp, Undo2,
} from "lucide-react";
import type { ReactNode } from "react";

export type ReportMetric =
  | "transactions"
  | "grossSales"
  | "discounts"
  | "tax"
  | "netRevenue"
  | "cash"
  | "card"
  | "refunds"
  | "pendingDeliveries";

/**
 * Metric icons come from lucide, the icon set every other workspace uses.
 * These cards previously rendered literal emoji, which changed shape with the
 * platform font, ignored the theme's colour tokens, and were the most visible
 * remnant of the pre-redesign reporting screen.
 */
const ICON = { size: 16, strokeWidth: 1.75, "aria-hidden": true } as const;

const META: Record<ReportMetric, { icon: ReactNode; color: string }> = {
  transactions:      { icon: <Receipt {...ICON} />,       color: "var(--accent)" },
  grossSales:        { icon: <Banknote {...ICON} />,      color: "var(--success, #22c55e)" },
  discounts:         { icon: <Tag {...ICON} />,           color: "var(--warning, #f59e0b)" },
  tax:               { icon: <ClipboardList {...ICON} />, color: "var(--text-dim)" },
  netRevenue:        { icon: <TrendingUp {...ICON} />,    color: "var(--success, #22c55e)" },
  cash:              { icon: <Banknote {...ICON} />,      color: "var(--text)" },
  card:              { icon: <CreditCard {...ICON} />,    color: "var(--text)" },
  refunds:           { icon: <Undo2 {...ICON} />,         color: "var(--error, #ef4444)" },
  pendingDeliveries: { icon: <Scooter {...ICON} />,       color: "var(--warning, #f59e0b)" },
};

export default function ReportSummaryCard({
  metric,
  label,
  value,
  accent,
  dim,
  warning,
}: {
  metric: ReportMetric;
  label: string;
  value: string;
  accent?: boolean;
  dim?: boolean;
  warning?: boolean;
}) {
  const meta = META[metric];
  return (
    <div className={`rpt-card2 ${accent ? "rpt-card2-accent" : ""} ${dim ? "rpt-card2-dim" : ""} ${warning ? "rpt-card2-warning" : ""}`}>
      <div className="rpt-card2-icon" style={{ color: meta.color }}>{meta.icon}</div>
      <div className="rpt-card2-label">{label}</div>
      <div className="rpt-card2-value">{value}</div>
    </div>
  );
}
