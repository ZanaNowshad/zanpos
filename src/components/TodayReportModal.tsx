import { useEffect, useMemo, useState } from "react";
import { Banknote, BarChart3, Printer, X } from "lucide-react";
import type { CashDrawerSummary, SessionToken, TodaySummary } from "../types";
import { DEVICE } from "../types";
import { cashXReport, reportToday } from "../tauri/commands";
import { formatMoney } from "../money";
import { useLanguage } from "../hooks/useLanguage";
import { backOfficeTranslator, cashEventTypeText } from "../i18n/backOfficeStrings";

interface Props {
  onClose: () => void;
  sessionUserId: string;
  shiftId: string;
  sessionToken: SessionToken;
  includeCashDrawer?: boolean;
}

export default function TodayReportModal({ onClose, sessionUserId, shiftId, sessionToken, includeCashDrawer = true }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => backOfficeTranslator(language), [language]);
  const [sales, setSales] = useState<TodaySummary | null>(null);
  const [drawer, setDrawer] = useState<CashDrawerSummary | null>(null);
  const [salesError, setSalesError] = useState<string | null>(null);
  const [drawerError, setDrawerError] = useState<string | null>(null);
  const today = new Date().toLocaleDateString("en-CA", { timeZone: "Asia/Bahrain" });
  const generatedAt = new Date().toLocaleString(language === "ar" ? "ar-BH" : "en-BH", { dateStyle: "medium", timeStyle: "short" });

  useEffect(() => {
    let cancelled = false;
    reportToday(sessionToken, DEVICE.branch_id, today)
      .then(value => { if (!cancelled) setSales(value); })
      .catch(() => { if (!cancelled) setSalesError(t("failedLoadReport")); });
    if (includeCashDrawer) {
      cashXReport(shiftId, sessionToken)
        .then(value => { if (!cancelled) setDrawer(value); })
        .catch(error => { if (!cancelled) setDrawerError(typeof error === "string" ? error : t("failedLoadReport")); });
    }
    return () => { cancelled = true; };
  }, [sessionToken, includeCashDrawer, sessionUserId, shiftId, t, today]);

  const fmt = (minor: number) => `${DEVICE.currency} ${formatMoney(minor, DEVICE.currency_exponent)}`;

  return (
    <button className="modal-overlay" type="button" onClick={onClose}>
      <div className="modal pos-reports-modal" role="dialog" aria-modal="true" aria-labelledby="pos-reports-title" onClick={event => event.stopPropagation()}>
        <div className="modal-header pos-reports-header">
          <div><span>Live shift snapshot</span><h2 id="pos-reports-title" className="modal-title">POS reports</h2><small>{today} · generated {generatedAt}</small></div>
          <button className="modal-close" onClick={onClose} aria-label="Close reports"><X size={18} /></button>
        </div>

        <div className={`pos-reports-grid${includeCashDrawer ? "" : " pos-reports-grid-single"}`}>
          <section className="pos-report-column" aria-labelledby="sales-report-title">
            <div className="pos-report-column-title"><BarChart3 size={18} /><div><h3 id="sales-report-title">Today’s sales</h3><span>Revenue and tenders</span></div></div>
            {!sales && !salesError && <div className="report-loading">{t("loading")}</div>}
            {salesError && <div className="modal-error">{salesError}</div>}
            {sales && (
              <div className="report-body">
                <div className="report-section">
                  <div className="report-row"><span>{t("transactions")}</span><strong>{sales.transaction_count}</strong></div>
                  <div className="report-row"><span>{t("grossSales")}</span><strong>{fmt(sales.gross_total_minor)}</strong></div>
                  <div className="report-row"><span>{t("discounts")}</span><strong className="report-negative">−{fmt(sales.discount_total_minor)}</strong></div>
                  <div className="report-row"><span>{t("taxCollected")}</span><strong>{fmt(sales.tax_total_minor)}</strong></div>
                  <div className="report-row report-row-total"><span>{t("netRevenue")}</span><strong>{fmt(sales.net_total_minor)}</strong></div>
                </div>
                <div className="report-section">
                  <div className="report-section-title">{t("byPaymentMethod")}</div>
                  <div className="report-row"><span>{t("cash")}</span><strong>{fmt(sales.cash_total_minor)}</strong></div>
                  <div className="report-row"><span>{t("card")}</span><strong>{fmt(sales.card_total_minor)}</strong></div>
                </div>
                {sales.pending_delivery_count > 0 && <div className="report-section report-section-warning"><div className="report-row"><span>{t("pendingDeliveries")}</span><strong>{sales.pending_delivery_count}</strong></div><div className="report-row"><span>{t("pendingRevenue")}</span><strong>{fmt(sales.pending_delivery_minor)}</strong></div></div>}
                {sales.refund_count > 0 && <div className="report-section"><div className="report-section-title">{t("refunds")}</div><div className="report-row"><span>{t("refundCount")}</span><strong>{sales.refund_count}</strong></div><div className="report-row"><span>{t("refundTotal")}</span><strong className="report-negative">−{fmt(sales.refund_total_minor)}</strong></div></div>}
              </div>
            )}
          </section>

          {includeCashDrawer && <section className="pos-report-column" aria-labelledby="drawer-report-title">
            <div className="pos-report-column-title"><Banknote size={18} /><div><h3 id="drawer-report-title">Cash drawer</h3><span>Expected balance this shift</span></div></div>
            {!drawer && !drawerError && <div className="report-loading">{t("loading")}</div>}
            {drawerError && <div className="modal-error">{drawerError}</div>}
            {drawer && (
              <div className="xreport-body">
                <table className="xreport-table"><tbody>
                  <tr><td>{t("openingFloat")}</td><td className="xreport-val">{fmt(drawer.opening_minor)}</td></tr>
                  <tr className="xreport-plus"><td>+ {t("cashSales")}</td><td className="xreport-val">{fmt(drawer.cash_sales_minor)}</td></tr>
                  {drawer.pending_delivery_cash_minor > 0 && <tr className="xreport-pending-delivery"><td>{t("pendingDeliveryCash")}</td><td className="xreport-val xreport-warning">{fmt(drawer.pending_delivery_cash_minor)}</td></tr>}
                  <tr className="xreport-minus"><td>− {t("cashRefunds")}</td><td className="xreport-val">{fmt(drawer.cash_refunds_minor)}</td></tr>
                  <tr className="xreport-plus"><td>+ {t("paidIn")}</td><td className="xreport-val">{fmt(drawer.paid_in_minor)}</td></tr>
                  <tr className="xreport-minus"><td>− {t("paidOut")}</td><td className="xreport-val">{fmt(drawer.paid_out_minor)}</td></tr>
                  <tr className="xreport-expected"><td><strong>{t("expectedInDrawer")}</strong></td><td className="xreport-val"><strong>{fmt(drawer.expected_minor)}</strong></td></tr>
                </tbody></table>
                {drawer.events.length > 0 && <><div className="xreport-events-title">{t("cashEvents")}</div><div className="xreport-events">{drawer.events.map(event => <div key={event.cash_event_id} className="xreport-event-row"><span className={`xreport-event-type ${event.event_type === "paid_in" ? "xreport-in" : "xreport-out"}`}>{event.event_type === "paid_in" ? "+" : "−"} {fmt(event.amount_minor)}</span><span className="xreport-event-note">{event.note ?? cashEventTypeText(language, event.event_type)}</span><span className="xreport-event-time">{new Date(event.created_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</span></div>)}</div></>}
              </div>
            )}
          </section>}
        </div>

        <div className="modal-actions pos-reports-actions"><button className="btn-secondary" onClick={() => window.print()}><Printer size={15} /> {t("print")}</button><button className="btn-primary" onClick={onClose}>{t("close")}</button></div>
      </div>
    </button>
  );
}
