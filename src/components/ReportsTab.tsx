import { useCallback, useEffect, useState } from "react";
import type { RangeSummary, SaleListRow, SaleListPage, TopProduct } from "../types";
import { DEVICE } from "../types";
import { formatMoney } from "../money";
import * as cmd from "../tauri/commands";

const BRANCH_ID = DEVICE.branch_id;
const EXP       = DEVICE.currency_exponent;
const CUR       = DEVICE.currency;

function fmt(n: number) { return `${CUR} ${formatMoney(n, EXP)}`; }

function isoDate(d: Date) { return d.toLocaleDateString("en-CA", { timeZone: "Asia/Bahrain" }); }

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
  const [salesPage, setSalesPage]   = useState<SaleListPage>({ items: [], total: 0, offset: 0, limit: 200 });
  const sales = salesPage.items;
  const [taxRows, setTaxRows]       = useState<TaxRow[]>([]);
  const [loading, setLoading]       = useState(false);
  const [loadError, setLoadError]   = useState<string | null>(null);
  const [activeSection, setActiveSection] = useState<"summary" | "products" | "sales" | "tax">("summary");
  const [voidingId, setVoidingId]   = useState<string | null>(null);
  const [voidConfirm, setVoidConfirm] = useState<SaleListRow | null>(null);
  const [stockWarning, setStockWarning] = useState<string | null>(null);

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
    setLoadError(null);
    try {
      const [s, tp, sl, tx] = await Promise.all([
        cmd.reportDateRange(sessionUserId, BRANCH_ID, from, to),
        cmd.reportTopProducts(sessionUserId, BRANCH_ID, from, to),
        cmd.reportSalesList(sessionUserId, BRANCH_ID, from, to),
        cmd.reportTaxByDay(sessionUserId, BRANCH_ID, from, to),
      ]);
      setSummary(s); setTopProducts(tp); setSalesPage(sl); setTaxRows(tx as TaxRow[]);
    } catch (e: unknown) {
      setLoadError(typeof e === "string" ? e : "Failed to load report. Check dates and try again.");
    } finally {
      setLoading(false);
    }
  }, [from, to, sessionUserId]);

  useEffect(() => { load(); }, [load]);

  const handlePreset = (p: Preset) => { applyPreset(p); };

  const handleVoid = async (sale: SaleListRow) => {
    setVoidConfirm(sale);
  };

  const executeVoid = async () => {
    const sale = voidConfirm;
    if (!sale) return;
    setVoidConfirm(null);
    setStockWarning(null);
    setVoidingId(sale.sale_id);
    try {
      const result = await cmd.posVoidSale(sale.sale_id, sessionUserId);
      setSalesPage(prev => ({ ...prev, items: prev.items.map(s => s.sale_id === sale.sale_id ? { ...s, status: "voided" } : s) }));
      if (result.stock_warning) {
        setStockWarning(result.stock_warning);
      }
    } catch (e: unknown) {
      const msg = typeof e === "string" ? e : "Failed to void sale";
      const display = msg.toLowerCase().includes("not permitted") || msg.toLowerCase().includes("permission")
        ? "Only managers and owners can void sales."
        : msg;
      setLoadError(display);
    } finally {
      setVoidingId(null);
    }
  };

  function downloadCSV(filename: string, rows: (string | number)[][]) {
    const csv = rows
      .map(r => r.map(cell => `"${String(cell).replace(/"/g, '""')}"`).join(","))
      .join("\n");
    const blob = new Blob(["﻿" + csv], { type: "text/csv;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url; a.download = filename; a.click();
    URL.revokeObjectURL(url);
  }

  const handleExportCSV = () => {
    const header = ["Receipt#","Date","Cashier","Method","Discount","Total","Status"];
    const rows = sales.map(s => [
      `#${s.receipt_number}`,
      new Date(s.sold_at).toLocaleString(),
      s.cashier_name,
      s.payment_methods,
      formatMoney(s.discount_total_minor, EXP),
      formatMoney(s.net_total_minor, EXP),
      s.status,
    ]);
    downloadCSV(`zanpos-sales-${from}-${to}.csv`, [header, ...rows]);
  };

  const handleExportProductsCSV = () => {
    const header = ["Rank","Product","Qty Sold","Transactions","Revenue"];
    const rows = topProducts.map((p, i) => [
      i + 1,
      p.product_name,
      parseFloat(p.total_quantity),
      p.transaction_count,
      formatMoney(p.revenue_minor, EXP),
    ]);
    downloadCSV(`zanpos-top-products-${from}-${to}.csv`, [header, ...rows]);
  };

  const handleExportTaxCSV = () => {
    const header = ["Date","Transactions","VAT Collected","Cumulative VAT"];
    const rows = taxRows.map(r => [
      r.day,
      r.transaction_count,
      formatMoney(r.tax_minor, EXP),
      formatMoney(r.cumulative_minor, EXP),
    ]);
    const totalRow = ["Total", taxTxCount, formatMoney(taxTotal, EXP), ""];
    downloadCSV(`zanpos-tax-${from}-${to}.csv`, [header, ...rows, totalRow]);
  };

  const handleExportSummaryCSV = () => {
    if (!summary) return;
    const rows: (string | number)[][] = [
      ["Metric", "Value"],
      ["Period", `${from} to ${to}`],
      ["Transactions", summary.transaction_count],
      ["Gross Sales", formatMoney(summary.gross_total_minor, EXP)],
      ["Discounts", formatMoney(summary.discount_total_minor, EXP)],
      ["Tax", formatMoney(summary.tax_total_minor, EXP)],
      ["Net Revenue", formatMoney(summary.net_total_minor, EXP)],
      ["Cash", formatMoney(summary.cash_total_minor, EXP)],
      ["Card", formatMoney(summary.card_total_minor, EXP)],
      ["Refunds", `${summary.refund_count} (${formatMoney(summary.refund_total_minor, EXP)})`],
    ];
    downloadCSV(`zanpos-summary-${from}-${to}.csv`, rows);
  };

  const taxTotal = taxRows.reduce((s, r) => s + r.tax_minor, 0);
  const taxTxCount = taxRows.reduce((s, r) => s + r.transaction_count, 0);

  return (
    <div className="rpt-layout">
      {/* ── Controls bar ── */}
      <div className="rpt-controls">
        <div className="rpt-presets2">
          {(["today", "week", "month"] as Preset[]).map(p => (
            <button
              key={p}
              className={`rpt-preset2-btn ${preset === p ? "rpt-preset2-active" : ""}`}
              onClick={() => handlePreset(p)}
            >
              {p === "today" ? "Today" : p === "week" ? "This Week" : "This Month"}
            </button>
          ))}
        </div>
        <div className="rpt-date-range">
          <input type="date" className="rpt-date-input" value={from}
            onChange={e => { setFrom(e.target.value); setPreset("custom"); }} />
          <span className="rpt-date-arrow">→</span>
          <input type="date" className="rpt-date-input" value={to}
            onChange={e => { setTo(e.target.value); setPreset("custom"); }} />
          <button className="btn-primary rpt-run-btn2" onClick={load} disabled={loading}>
            {loading ? "Loading…" : "▶ Run"}
          </button>
          {activeSection === "sales" && sales.length > 0 && (
            <button className="btn-secondary rpt-export-btn2" onClick={handleExportCSV}>↓ CSV</button>
          )}
          {activeSection === "products" && topProducts.length > 0 && (
            <button className="btn-secondary rpt-export-btn2" onClick={handleExportProductsCSV}>↓ CSV</button>
          )}
          {activeSection === "tax" && taxRows.length > 0 && (
            <button className="btn-secondary rpt-export-btn2" onClick={handleExportTaxCSV}>↓ CSV</button>
          )}
          {activeSection === "summary" && summary && (
            <button className="btn-secondary rpt-export-btn2" onClick={handleExportSummaryCSV}>↓ CSV</button>
          )}
        </div>
      </div>

      <div className="rpt-tabs2">
        {(["summary", "products", "sales", "tax"] as const).map(s => (
          <button
            key={s}
            className={`rpt-tab2 ${activeSection === s ? "rpt-tab2-active" : ""}`}
            onClick={() => setActiveSection(s)}
          >
            {s === "summary" ? "📊 Summary"
              : s === "products" ? "🏆 Top Products"
              : s === "sales" ? "🧾 Sales List"
              : "🧮 Tax"}
            {s === "sales" && sales.length > 0 && (
              <span className="rpt-tab2-badge">{sales.length}</span>
            )}
          </button>
        ))}
      </div>

      <div className="rpt-body">
        {/* ── Load error banner ── */}
        {loadError && (
          <div className="rpt-error-banner" role="alert">
            ⚠ {loadError}
            <button className="rpt-error-retry" onClick={load}>Retry</button>
          </div>
        )}
        {stockWarning && (
          <div className="rpt-error-banner" role="alert" style={{ background: "var(--warning-bg, #fef3c7)" }}>
            ⚠ {stockWarning}
            <button className="rpt-error-retry" onClick={() => setStockWarning(null)}>Dismiss</button>
          </div>
        )}
        {/* ── Summary cards ── */}
        {activeSection === "summary" && summary && (
          <div className="rpt-summary2">
            <SummaryCard label="Transactions"   value={String(summary.transaction_count)} accent />
            <SummaryCard label="Gross Sales"    value={fmt(summary.gross_total_minor)} />
            <SummaryCard label="Discounts"      value={`− ${fmt(summary.discount_total_minor)}`} dim />
            <SummaryCard label="Tax"            value={fmt(summary.tax_total_minor)} dim />
            <SummaryCard label="Net Revenue"    value={fmt(summary.net_total_minor)} accent />
            <SummaryCard label="Cash"           value={fmt(summary.cash_total_minor)} />
            <SummaryCard label="Card"           value={fmt(summary.card_total_minor)} />
            <SummaryCard label="Refunds"        value={`${summary.refund_count} (${fmt(summary.refund_total_minor)})`} dim />
            {summary.pending_delivery_count > 0 && (
              <SummaryCard
                label="Pending Deliveries"
                value={`${summary.pending_delivery_count} (${fmt(summary.pending_delivery_minor)})`}
                warning
              />
            )}
          </div>
        )}

        {/* ── Top products ── */}
        {activeSection === "products" && (
          <div className="rpt-table-wrap">
            {topProducts.length === 0 ? (
              <div className="bo-empty">
                <div className="bo-empty-icon">📊</div>
                <p className="bo-empty-title">No data for this period</p>
                <p className="bo-empty-hint">Adjust the date range and run the report again.</p>
              </div>
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
              <div className="bo-empty">
                <div className="bo-empty-icon">📊</div>
                <p className="bo-empty-title">No data for this period</p>
                <p className="bo-empty-hint">Adjust the date range and run the report again.</p>
              </div>
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
                        <span className={`rpt-badge rpt-badge-${s.status}`}>{s.status}</span>
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
            {salesPage.total > salesPage.items.length && (
              <p className="rpt-row-limit-notice">
                ⚠ Showing {salesPage.items.length} of {salesPage.total} sales. Use <strong>↓ CSV</strong> to export the full list, or narrow your date range.
              </p>
            )}
          </div>
        )}

        {/* ── Tax report ── */}
        {activeSection === "tax" && (
          <div className="rpt-table-wrap">
            {taxRows.length === 0 ? (
              <div className="bo-empty">
                <div className="bo-empty-icon">🧾</div>
                <p className="bo-empty-title">No taxable sales</p>
                <p className="bo-empty-hint">No taxable transactions were recorded in this period.</p>
              </div>
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

      {/* ── Void confirm dialog ── */}
      {voidConfirm && (
        <div className="settings-confirm-overlay" onClick={() => setVoidConfirm(null)}>
          <div className="settings-confirm-dialog" onClick={e => e.stopPropagation()}>
            <div className="settings-confirm-header">Confirm Void</div>
            <p className="settings-confirm-msg">
              Void sale #{voidConfirm.receipt_number} ({fmt(voidConfirm.net_total_minor)})?
              <br /><strong>This cannot be undone.</strong>
            </p>
            <div className="settings-confirm-buttons">
              <button className="btn-primary" style={{ background: "var(--error, #ef4444)" }} onClick={executeVoid} disabled={voidingId !== null}>
                {voidingId ? "Voiding…" : "Yes, Void Sale"}
              </button>
              <button className="btn-secondary" onClick={() => setVoidConfirm(null)} disabled={voidingId !== null}>Cancel</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

const CARD_META: Record<string, { icon: string; color: string }> = {
  "Transactions":        { icon: "🧾", color: "var(--accent)" },
  "Gross Sales":         { icon: "💵", color: "var(--success, #22c55e)" },
  "Net Revenue":         { icon: "✅", color: "var(--success, #22c55e)" },
  "Discounts":           { icon: "🏷️", color: "var(--warning, #f59e0b)" },
  "Tax":                 { icon: "📋", color: "var(--text-dim)" },
  "Cash":                { icon: "💵", color: "var(--text)" },
  "Card":                { icon: "💳", color: "var(--text)" },
  "Wallet":              { icon: "📱", color: "var(--text)" },
  "Refunds":             { icon: "↩️", color: "var(--error, #ef4444)" },
  "Pending Deliveries":  { icon: "🛵", color: "var(--warning, #f59e0b)" },
};

function SummaryCard({ label, value, accent, dim, warning }: {
  label: string; value: string; accent?: boolean; dim?: boolean; warning?: boolean;
}) {
  const meta = CARD_META[label] ?? { icon: "📊", color: "var(--text-dim)" };
  return (
    <div className={`rpt-card2 ${accent ? "rpt-card2-accent" : ""} ${dim ? "rpt-card2-dim" : ""} ${warning ? "rpt-card2-warning" : ""}`}>
      <div className="rpt-card2-icon" style={{ color: meta.color }}>{meta.icon}</div>
      <div className="rpt-card2-label">{label}</div>
      <div className="rpt-card2-value">{value}</div>
    </div>
  );
}
