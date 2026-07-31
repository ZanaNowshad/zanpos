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

const META: Record<ReportMetric, { icon: string; color: string }> = {
  transactions: { icon: "🧾", color: "var(--accent)" },
  grossSales: { icon: "💵", color: "var(--success, #22c55e)" },
  discounts: { icon: "🏷️", color: "var(--warning, #f59e0b)" },
  tax: { icon: "📋", color: "var(--text-dim)" },
  netRevenue: { icon: "✅", color: "var(--success, #22c55e)" },
  cash: { icon: "💵", color: "var(--text)" },
  card: { icon: "💳", color: "var(--text)" },
  refunds: { icon: "↩️", color: "var(--error, #ef4444)" },
  pendingDeliveries: { icon: "🛵", color: "var(--warning, #f59e0b)" },
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
