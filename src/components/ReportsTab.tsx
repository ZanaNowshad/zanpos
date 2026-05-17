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

interface TaxRow {
  day: string;
  transaction_count: number;
  tax_minor: number;
  cumulative_minor: number;
}

interface Props {
  sessionUserId: string;
}

export default function ReportsTab({ sessionUserId }: Props) {
  const [preset, setPreset]         = useState<Preset>("month");
  const [from, setFrom]             = useState(defaultRange().from);
  const [to, setTo]                 = useState(defaultRange().to);
  const [summary, setSummary]       = useState<RangeSummary | null>(null);
  const [topProducts, setTopProducts] = useState<TopProduct[]>([]);
  const [sales, setSales]           = useState<SaleListRow[]>([]);
  const [taxRows, setTaxRows]       = useState<TaxRow[]>([]);
  const [loading, setLoading]       = useState(false);
  const [activeSection, setActiveSection] = useState<"summary" | "products" | "sales" | "tax">("summary");
  const [voidingId, setVoidingId]   = useState<string | null>(null);

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
      const [s, tp, sl, tx] = await Promise.all([
        cmd.reportDateRange(BRANCH_ID, from, to),
        cmd.reportTopProducts(BRANCH_ID, from, to),
        cmd.reportSalesList(BRANCH_ID, from, to),
        cmd.reportTaxByDay(BRANCH_ID, from, to),
      ]);
      setSummary(s); setTopProducts(tp); setSales(sl); setTaxRows(tx as TaxRow[]);
    } finally {
      setLoading(false);
    }
  }, [from, to]);

  useEffect(() => { load(); }, [load]);

  const handlePreset = (p: Preset) => { applyPreset(p); };

  const handleVoid = async (sale: SaleListRow) => {
    if (!confirm(`Void sale #${sale.receipt_number} (${fmt(sale.net_total_minor)})? This cannot be undone.`)) return;
    setVoidingId(sale.sale_id);
    try {
      await cmd.posVoidSale(sale.sale_id, sessionUserId);
      setSales(prev => prev.map(s => s.sale_id === sale.sale_id ? { ...s, status: "voided" } : s));
    } catch (e: unknown) {
      alert(typeof e === "string" ? e : "Failed to void sale");
    } finally {
      setVoidingId(null);
    }
  };

  const handleExportCSV = () => {
    const header = ["Receipt#","Date","Cashier","Method","Discount","Tax","Total","Status"];
    const rows = sales.map(s => [
      `#${s.receipt_number}`,
      new Date(s.sold_at).toLocaleString(),
      s.cashier_name,
      s.payment_methods,
      formatMoney(s.discount_total_minor, EXP),
      "", // no tax in SaleListRow — placeholder
      formatMoney(s.net_total_minor, EXP),
      s.status,
    ]);
    const csv = [header, ...rows]
      .map(r => r.map(cell => `"${String(cell).replace(/"/g, '""')}"`).join(","))
      .join("\n");
    const blob = new Blob([csv], { type: "text/csv" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `zanpos-sales-${from}-${to}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const taxTotal = taxRows.reduce((s, r) => s + r.tax_minor, 0);
  const taxTxCount = taxRows.reduce((s, r) => s + r.transaction_count, 0);

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
          {activeSection === "sales" && sales.length > 0 && (
            <button className="btn-secondary rpt-export-btn" onClick={handleExportCSV} title="Export CSV">
              ⬇ Export CSV
            </button>
          )}
        </div>
      </div>

      {/* ── Section tabs ── */}
      <div className="rpt-section-tabs">
        {(["summary", "products", "sales", "tax"] as const).map(s => (
          <button
            key={s}
            className={`rpt-section-tab ${activeSection === s ? "rpt-section-active" : ""}`}
            onClick={() => setActiveSection(s)}
          >
            {s === "summary" ? "Summary" : s === "products" ? "Top Products" : s === "sales" ? "Sales List" : "Tax"}
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
                    <th></th>
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
                      <td>
                        {s.status === "completed" && (
                          <button
                            className="rpt-void-btn"
                            onClick={() => handleVoid(s)}
                            disabled={voidingId === s.sale_id}
                            title="Void this sale"
                          >
                            {voidingId === s.sale_id ? "…" : "Void"}
                          </button>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        )}

        {/* ── Tax report ── */}
        {activeSection === "tax" && (
          <div className="rpt-table-wrap">
            {taxRows.length === 0 ? (
              <div className="bo-empty">No taxable sales in this period.</div>
            ) : (
              <table className="rpt-table">
                <thead>
                  <tr>
                    <th>Date</th>
                    <th className="rpt-num">Transactions</th>
                    <th className="rpt-num">VAT Collected</th>
                    <th className="rpt-num">Cumulative VAT</th>
                  </tr>
                </thead>
                <tbody>
                  {taxRows.map(r => (
                    <tr key={r.day}>
                      <td>{r.day}</td>
                      <td className="rpt-num">{r.transaction_count}</td>
                      <td className="rpt-num rpt-money">{fmt(r.tax_minor)}</td>
                      <td className="rpt-num rpt-dim">{fmt(r.cumulative_minor)}</td>
                    </tr>
                  ))}
                  <tr className="rpt-tax-total-row">
                    <td><strong>Total</strong></td>
                    <td className="rpt-num"><strong>{taxTxCount}</strong></td>
                    <td className="rpt-num rpt-money"><strong>{fmt(taxTotal)}</strong></td>
                    <td></td>
                  </tr>
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
