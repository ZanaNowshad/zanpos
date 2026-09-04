import { useCallback, useEffect, useMemo, useState } from "react";
import { BarChart2, BarChart3, Calculator, Receipt, Trophy } from "lucide-react";
import { save } from "@tauri-apps/plugin-dialog";
import type { RangeSummary, SaleListRow, SaleListPage, TopProduct } from "../types";
import { DEVICE } from "../types";
import { formatMoney } from "../money";
import * as cmd from "../tauri/commands";
import ManagerApprovalModal from "./ManagerApprovalModal";
import { useLanguage } from "../hooks/useLanguage";
import {
  backOfficeTranslator,
  reportPaymentMethodsText,
  reportSaleStatusText,
} from "../i18n/backOfficeStrings";
import ReportSummaryCard from "./ReportSummaryCard";
import { reportCsvExports } from "./reportCsvExports";
import { EmptyState } from "./templates";

const BRANCH_ID = DEVICE.branch_id;
const TAB_ICON  = { size: 14, strokeWidth: 1.75, "aria-hidden": true } as const;
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
  const { language } = useLanguage();
  const t = useMemo(() => backOfficeTranslator(language), [language]);
  const [preset, setPreset]         = useState<Preset>("month");
  const [from, setFrom]             = useState(defaultRange().from);
  const [to, setTo]                 = useState(defaultRange().to);
  const [summary, setSummary]       = useState<RangeSummary | null>(null);
  const [topProducts, setTopProducts] = useState<TopProduct[]>([]);
  const [salesPage, setSalesPage]   = useState<SaleListPage>({ items: [], total: 0, offset: 0, limit: 200 });
  const [salesOffset, setSalesOffset] = useState(0);
  const sales = salesPage.items;
  const [taxRows, setTaxRows]       = useState<TaxRow[]>([]);
  const [loading, setLoading]       = useState(false);
  const [loadError, setLoadError]   = useState<string | null>(null);
  const [activeSection, setActiveSection] = useState<"summary" | "products" | "sales" | "tax">("summary");
  const [voidingId, setVoidingId]   = useState<string | null>(null);
  const [voidConfirm, setVoidConfirm] = useState<SaleListRow | null>(null);
  const [stockWarning, setStockWarning] = useState<string | null>(null);
  const [exportingSales, setExportingSales] = useState(false);

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
      setLoadError(typeof e === "string" ? e : t("failedLoadReport"));
    } finally {
      setLoading(false);
    }
  }, [from, to, sessionUserId, t]);

  useEffect(() => { load(); }, [load]);

  const handlePreset = (p: Preset) => { applyPreset(p); };

  const handleVoid = async (sale: SaleListRow) => {
    setVoidConfirm(sale);
  };

  // Voiding reverses money and stock, so it takes a manager's PIN rather than
  // the id of whoever happens to be signed in.
  const [voidPinOpen, setVoidPinOpen] = useState(false);

  const executeVoid = async (managerOverrideToken: string) => {
    const sale = voidConfirm;
    if (!sale) return;
    setVoidConfirm(null);
    setStockWarning(null);
    setVoidingId(sale.sale_id);
    try {
      const result = await cmd.posVoidSale(sale.sale_id, managerOverrideToken);
      setSalesPage(prev => ({ ...prev, items: prev.items.map(s => s.sale_id === sale.sale_id ? { ...s, status: "voided" } : s) }));
      if (result.stock_warning) {
        setStockWarning(result.stock_warning);
      }
    } catch (e: unknown) {
      const msg = typeof e === "string" ? e : t("failedVoidSale");
      const display = msg.toLowerCase().includes("not permitted") || msg.toLowerCase().includes("permission")
        ? t("voidPermissionRequired")
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

  const handleExportCSV = async () => {
    const dest = await save({
      defaultPath: `zanpos-sales-${from}-${to}.csv`,
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (!dest) return;
    setExportingSales(true);
    setLoadError(null);
    try {
      await cmd.reportSalesExportCsv(sessionUserId, BRANCH_ID, from, to, dest);
    } catch (error: unknown) {
      setLoadError(typeof error === "string" ? error : t("failedLoadReport"));
    } finally {
      setExportingSales(false);
    }
  };

  const taxTotal = taxRows.reduce((s, r) => s + r.tax_minor, 0);
  const taxTxCount = taxRows.reduce((s, r) => s + r.transaction_count, 0);

  const { handleExportProductsCSV, handleExportTaxCSV, handleExportSummaryCSV } =
    reportCsvExports(
      { t, downloadCSV, from, to, currencyExponent: EXP },
      { topProducts, taxRows, taxTxCount, taxTotal, summary },
    );

  return (
    <div className="rpt-layout">
      {/* Reports previously opened straight into a filter bar with no page
          heading, so the domain had no <h1> and screen-reader users landed on
          a control strip. The header matches the shell's grammar without
          restructuring the report body. */}
      <header className="oa-topbar rpt-topbar">
        <div className="oa-title-block">
          <span className="oa-title-icon"><BarChart3 size={18} strokeWidth={1.7} aria-hidden="true" /></span>
          <div>
            <h1 className="oa-title">{t("reports")}</h1>
            <p className="oa-subtitle">{from} → {to}</p>
          </div>
        </div>
      </header>

      {/* ── Controls bar ── */}
      <div className="rpt-controls">
        <div className="rpt-presets2">
          {(["today", "week", "month"] as Preset[]).map(p => (
            <button
              key={p}
              className={`rpt-preset2-btn ${preset === p ? "rpt-preset2-active" : ""}`}
              onClick={() => handlePreset(p)}
            >
              {p === "today" ? t("today") : p === "week" ? t("thisWeek") : t("thisMonth")}
            </button>
          ))}
        </div>
        <div className="rpt-date-range">
          <input type="date" className="rpt-date-input" value={from}
            onChange={e => { setFrom(e.target.value); setPreset("custom"); }} />
          <span className="rpt-date-arrow icon-directional" aria-hidden="true">→</span>
          <input type="date" className="rpt-date-input" value={to}
            onChange={e => { setTo(e.target.value); setPreset("custom"); }} />
          <button className="btn-primary rpt-run-btn2" onClick={load} disabled={loading}>
            {loading ? t("loading") : `▶ ${t("runReport")}`}
          </button>
          {activeSection === "sales" && sales.length > 0 && (
            <button className="btn-secondary rpt-export-btn2" onClick={handleExportCSV} disabled={exportingSales}>
              ↓ {exportingSales ? t("loading") : t("exportCsv")}
            </button>
          )}
          {activeSection === "products" && topProducts.length > 0 && (
            <button className="btn-secondary rpt-export-btn2" onClick={handleExportProductsCSV}>↓ {t("exportCsv")}</button>
          )}
          {activeSection === "tax" && taxRows.length > 0 && (
            <button className="btn-secondary rpt-export-btn2" onClick={handleExportTaxCSV}>↓ {t("exportCsv")}</button>
          )}
          {activeSection === "summary" && summary && (
            <button className="btn-secondary rpt-export-btn2" onClick={handleExportSummaryCSV}>↓ {t("exportCsv")}</button>
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
            {/* lucide, matching every other tab strip in the shell */}
            {s === "summary" ? <><BarChart3 {...TAB_ICON} /> {t("reportSummary")}</>
              : s === "products" ? <><Trophy {...TAB_ICON} /> {t("topProducts")}</>
              : s === "sales" ? <><Receipt {...TAB_ICON} /> {t("salesList")}</>
              : <><Calculator {...TAB_ICON} /> {t("taxReport")}</>}
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
            <button className="rpt-error-retry" onClick={load}>{t("retry")}</button>
          </div>
        )}
        {stockWarning && (
          <div className="rpt-error-banner" role="alert" style={{ background: "var(--warning-bg, #fef3c7)" }}>
            ⚠ {stockWarning}
            <button className="rpt-error-retry" onClick={() => setStockWarning(null)}>{t("dismiss")}</button>
          </div>
        )}
        {/* ── Summary cards ── */}
        {/* A period with no sales is not a period of zero performance. Eight
            zero cards read as a result; this states plainly that nothing was
            sold in the selected range. */}
        {activeSection === "summary" && summary && summary.transaction_count === 0 && (
          <EmptyState
            variant="no-results"
            title={t("noSalesPeriod")}
            description={t("adjustDateRange")}
          />
        )}

        {activeSection === "summary" && summary && summary.transaction_count > 0 && (
          <div className="rpt-summary2">
            <ReportSummaryCard metric="transactions" label={t("transactions")} value={String(summary.transaction_count)} accent />
            <ReportSummaryCard metric="grossSales" label={t("grossSales")} value={fmt(summary.gross_total_minor)} />
            <ReportSummaryCard metric="discounts" label={t("discounts")} value={`− ${fmt(summary.discount_total_minor)}`} dim />
            <ReportSummaryCard metric="tax" label={t("taxReport")} value={fmt(summary.tax_total_minor)} dim />
            <ReportSummaryCard metric="netRevenue" label={t("netRevenue")} value={fmt(summary.net_total_minor)} accent />
            <ReportSummaryCard metric="cash" label={t("cash")} value={fmt(summary.cash_total_minor)} />
            <ReportSummaryCard metric="card" label={t("card")} value={fmt(summary.card_total_minor)} />
            <ReportSummaryCard metric="refunds" label={t("refunds")} value={`${summary.refund_count} (${fmt(summary.refund_total_minor)})`} dim />
            {summary.pending_delivery_count > 0 && (
              <ReportSummaryCard
                metric="pendingDeliveries"
                label={t("pendingDeliveries")}
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
                <div className="bo-empty-icon"><BarChart2 size={40} strokeWidth={1.5} /></div>
                <p className="bo-empty-title">{t("noDataForPeriod")}</p>
                <p className="bo-empty-hint">{t("adjustDateRange")}</p>
              </div>
            ) : (
              <table className="rpt-table">
                <thead>
                  <tr>
                    <th>#</th>
                    <th>{t("product")}</th>
                    <th className="rpt-num">{t("quantitySold")}</th>
                    <th className="rpt-num">{t("transactions")}</th>
                    <th className="rpt-num">{t("revenue")}</th>
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
                <div className="bo-empty-icon"><BarChart2 size={40} strokeWidth={1.5} /></div>
                <p className="bo-empty-title">{t("noDataForPeriod")}</p>
                <p className="bo-empty-hint">{t("adjustDateRange")}</p>
              </div>
            ) : (
              <table className="rpt-table">
                <thead>
                  <tr>
                    <th>{t("receipt")}</th>
                    <th>{t("dateTime")}</th>
                    <th>{t("cashier")}</th>
                    <th>{t("payment")}</th>
                    <th className="rpt-num">{t("discounts")}</th>
                    <th className="rpt-num">{t("total")}</th>
                    <th>{t("status")}</th>
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
                      <td className="rpt-method">{reportPaymentMethodsText(language, s.payment_methods)}</td>
                      <td className="rpt-num rpt-dim">
                        {s.discount_total_minor > 0 ? `− ${fmt(s.discount_total_minor)}` : "—"}
                      </td>
                      <td className="rpt-num rpt-money">{fmt(s.net_total_minor)}</td>
                      <td>
                        <span className={`rpt-badge rpt-badge-${s.status}`}>{reportSaleStatusText(language, s.status)}</span>
                      </td>
                      <td>
                        {s.status === "completed" && (
                          <button
                            className="rpt-void-btn"
                            onClick={() => handleVoid(s)}
                            disabled={voidingId === s.sale_id}
                            title={t("voidAction")}
                          >
                            {voidingId === s.sale_id ? "…" : t("voidAction")}
                          </button>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
            {salesPage.total > 100 && (
              <div className="bo-pagination" style={{ marginTop: 12 }}>
                <button className="btn-secondary btn-sm" disabled={salesOffset === 0}
                  onClick={async () => {
                    const newOff = Math.max(0, salesOffset - 100);
                    setSalesOffset(newOff);
                    const pg = await cmd.reportSalesList(sessionUserId, BRANCH_ID, from, to, newOff, 100);
                    setSalesPage(pg as SaleListPage);
                  }}><span className="icon-directional" aria-hidden="true">←</span> {t("previous")}</button>
                <span className="bo-pagination-info">
                  {salesOffset + 1}–{Math.min(salesOffset + sales.length, salesPage.total)} {t("of")} {salesPage.total}
                </span>
                <button className="btn-secondary btn-sm" disabled={salesOffset + sales.length >= salesPage.total}
                  onClick={async () => {
                    const newOff = salesOffset + 100;
                    setSalesOffset(newOff);
                    const pg = await cmd.reportSalesList(sessionUserId, BRANCH_ID, from, to, newOff, 100);
                    setSalesPage(pg as SaleListPage);
                  }}>{t("next")} <span className="icon-directional" aria-hidden="true">→</span></button>
              </div>
            )}
          </div>
        )}

        {/* ── Tax report ── */}
        {activeSection === "tax" && (
          <div className="rpt-table-wrap">
            {taxRows.length === 0 ? (
              <div className="bo-empty">
                <div className="bo-empty-icon"><Receipt size={40} strokeWidth={1.5} /></div>
                <p className="bo-empty-title">{t("noTaxableSales")}</p>
                <p className="bo-empty-hint">{t("noTaxableTransactions")}</p>
              </div>
            ) : (
              <table className="rpt-table">
                <thead>
                  <tr>
                    <th>{t("date")}</th>
                    <th className="rpt-num">{t("transactions")}</th>
                    <th className="rpt-num">{t("vatCollected")}</th>
                    <th className="rpt-num">{t("cumulativeVat")}</th>
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
                    <td><strong>{t("total")}</strong></td>
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
        <div role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}  className="settings-confirm-overlay" onClick={() => setVoidConfirm(null)}>
          <div role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}  className="settings-confirm-dialog" onClick={e => e.stopPropagation()}>
            <div className="settings-confirm-header">{t("confirmVoidSale")}</div>
            <p className="settings-confirm-msg">
              #{voidConfirm.receipt_number} ({fmt(voidConfirm.net_total_minor)})
              <br /><strong>{t("voidSalePrompt")}</strong>
            </p>
            <div className="settings-confirm-buttons">
              <button className="btn-primary" style={{ background: "var(--error, #ef4444)" }} onClick={() => setVoidPinOpen(true)} disabled={voidingId !== null}>
                {voidingId ? t("voiding") : t("yesVoidSale")}
              </button>
              <button className="btn-secondary" onClick={() => setVoidConfirm(null)} disabled={voidingId !== null}>{t("cancel")}</button>
            </div>
          </div>
        </div>
      )}

      {voidConfirm && voidPinOpen && (
        <ManagerApprovalModal
          action={`Void sale #${voidConfirm.receipt_number}`}
          onApproved={async token => {
            setVoidPinOpen(false);
            await executeVoid(token);
          }}
          onCancel={() => setVoidPinOpen(false)}
        />
      )}
    </div>
  );
}
