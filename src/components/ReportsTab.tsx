import { useCallback, useEffect, useState } from "react";
import type { RangeSummary, SaleListRow, TopProduct } from "../types";
import { DEVICE } from "../types";
import { formatMoney } from "../money";
import * as cmd from "../tauri/commands";

const BRANCH_ID = DEVICE.branch_id;
const EXP       = DEVICE.currency_exponent;
const CUR       = DEVICE.currency;

function fmt(n: number) { return `${CUR} ${formatMoney(n, EXP)}`; }

function isoDate(d: Date) { return d.toISOString().slice(0, 10); }

function defaultRange() {
  const to   = new Date();
  const from = new Date();
  from.setDate(from.getDate() - 29);   // last 30 days
  return { from: isoDate(from), to: isoDate(to) };
}

type Preset = "today" | "week" | "month" | "custom";

export default function ReportsTab() {
  const [preset, setPreset]         = useState<Preset>("month");
  const [from, setFrom]             = useState(defaultRange().from);
  const [to, setTo]                 = useState(defaultRange().to);
  const [summary, setSummary]       = useState<RangeSummary | null>(null);
  const [topProducts, setTopProducts] = useState<TopProduct[]>([]);
  const [sales, setSales]           = useState<SaleListRow[]>([]);
  const [loading, setLoading]       = useState(false);
  const [activeSection, setActiveSection] = useState<"summary" | "products" | "sales">("summary");

  const applyPreset = useCallback((p: Preset) => {
    const today = new Date();
    setPreset(p);
    if (p === "today") {
      const d = isoDate(today);
      setFrom(d); setTo(d);
    } else if (p === "week") {
      const mon = new Date(today);
      mon.setDate(today.getDate() - today.getDay() + 1);
      setFrom(isoDate(mon)); setTo(isoDate(today));
    } else if (p === "month") {
      const start = new Date(today.getFullYear(), today.getMonth(), 1);
      setFrom(isoDate(start)); setTo(isoDate(today));
    }
  }, []);

  const load = useCallback(async () => {
    if (!from || !to || from > to) return;
    setLoading(true);
    try {
      const [s, tp, sl] = await Promise.all([
        cmd.reportDateRange(BRANCH_ID, from, to),
        cmd.reportTopProducts(BRANCH_ID, from, to),
        cmd.reportSalesList(BRANCH_ID, from, to),
      ]);
      setSummary(s); setTopProducts(tp); setSales(sl);
    } finally {
      setLoading(false);
    }
  }, [from, to]);

  useEffect(() => { load(); }, [load]);

  const handlePreset = (p: Preset) => { applyPreset(p); };

  return (
    <div className="rpt-layout">
      {/* ── Date range bar ── */}
      <div className="rpt-topbar">
        <div className="rpt-presets">
          {(["today", "week", "month"] as Preset[]).map(p => (
            <button
              key={p}
              className={`rpt-preset-btn ${preset === p ? "rpt-preset-active" : ""}`}
              onClick={() => handlePreset(p)}
            >
              {p === "today" ? "Today" : p === "week" ? "This Week" : "This Month"}
            </button>
          ))}
        </div>
        <div className="rpt-date-inputs">
          <input type="date" className="rpt-date-input" value={from}
            onChange={e => { setFrom(e.target.value); setPreset("custom"); }} />
          <span className="rpt-date-sep">→</span>
          <input type="date" className="rpt-date-input" value={to}
            onChange={e => { setTo(e.target.value); setPreset("custom"); }} />
          <button className="btn-primary rpt-run-btn" onClick={load} disabled={loading}>
            {loading ? "…" : "Run"}
          </button>
        </div>
      </div>

      {/* ── Section tabs ── */}
      <div className="rpt-section-tabs">
        {(["summary", "products", "sales"] as const).map(s => (
          <button
            key={s}
            className={`rpt-section-tab ${activeSection === s ? "rpt-section-active" : ""}`}
            onClick={() => setActiveSection(s)}
          >
            {s === "summary" ? "Summary" : s === "products" ? "Top Products" : "Sales List"}
            {s === "sales" && sales.length > 0 && <span className="rpt-count">{sales.length}</span>}
          </button>
        ))}
      </div>

      <div className="rpt-body">
        {/* ── Summary cards ── */}
        {activeSection === "summary" && summary && (
          <div className="rpt-summary">
            <SummaryCard label="Transactions"   value={String(summary.transaction_count)} accent />
            <SummaryCard label="Gross Sales"    value={fmt(summary.gross_total_minor)} />
            <SummaryCard label="Discounts"      value={`− ${fmt(summary.discount_total_minor)}`} dim />
            <SummaryCard label="Tax"            value={fmt(summary.tax_total_minor)} dim />
            <SummaryCard label="Net Revenue"    value={fmt(summary.net_total_minor)} accent />
            <SummaryCard label="Cash"           value={fmt(summary.cash_total_minor)} />
            <SummaryCard label="Card"           value={fmt(summary.card_total_minor)} />
            <SummaryCard label="Refunds"        value={`${summary.refund_count} (${fmt(summary.refund_total_minor)})`} dim />
          </div>
        )}

        {/* ── Top products ── */}
        {activeSection === "products" && (
          <div className="rpt-table-wrap">
            {topProducts.length === 0 ? (
              <div className="bo-empty">No sales in this period.</div>
            ) : (
              <table className="rpt-table">
                <thead>
                  <tr>
                    <th>#</th>
                    <th>Product</th>
                    <th className="rpt-num">Qty Sold</th>
                    <th className="rpt-num">Transactions</th>
                    <th className="rpt-num">Revenue</th>
                  </tr>
                </thead>
                <tbody>
                  {topProducts.map((p, i) => (
                    <tr key={i}>
                      <td className="rpt-rank">{i + 1}</td>
                      <td>{p.product_name}</td>
                      <td className="rpt-num">{parseFloat(p.total_quantity).toLocaleString()}</td>
                      <td className="rpt-num">{p.transaction_count}</td>
                      <td className="rpt-num rpt-money">{fmt(p.revenue_minor)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        )}

        {/* ── Sales list ── */}
        {activeSection === "sales" && (
          <div className="rpt-table-wrap">
            {sales.length === 0 ? (
              <div className="bo-empty">No sales in this period.</div>
            ) : (
              <table className="rpt-table">
                <thead>
                  <tr>
                    <th>Receipt</th>
                    <th>Date / Time</th>
                    <th>Cashier</th>
                    <th>Payment</th>
                    <th className="rpt-num">Discount</th>
                    <th className="rpt-num">Total</th>
                    <th>Status</th>
                  </tr>
                </thead>
                <tbody>
                  {sales.map(s => (
                    <tr key={s.sale_id} className={s.status !== "completed" ? "rpt-row-dim" : ""}>
                      <td className="rpt-receipt">#{s.receipt_number}</td>
                      <td className="rpt-date">{new Date(s.sold_at).toLocaleString([], {
                        month: "short", day: "numeric",
                        hour: "2-digit", minute: "2-digit"
                      })}</td>
                      <td>{s.cashier_name}</td>
                      <td className="rpt-method">{s.payment_methods}</td>
                      <td className="rpt-num rpt-dim">
                        {s.discount_total_minor > 0 ? `− ${fmt(s.discount_total_minor)}` : "—"}
                      </td>
                      <td className="rpt-num rpt-money">{fmt(s.net_total_minor)}</td>
                      <td>
                        <span className={`rpt-status rpt-status-${s.status}`}>{s.status}</span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function SummaryCard({ label, value, accent, dim }: {
  label: string; value: string; accent?: boolean; dim?: boolean;
}) {
  return (
    <div className={`rpt-card ${accent ? "rpt-card-accent" : ""} ${dim ? "rpt-card-dim" : ""}`}>
      <div className="rpt-card-label">{label}</div>
      <div className="rpt-card-value">{value}</div>
    </div>
  );
}
