import { useEffect, useMemo, useState } from "react";
import type { CashDrawerSummary, SessionToken } from "../types";
import { DEVICE } from "../types";
import { cashXReport } from "../tauri/commands";
import { formatMoney } from "../money";
import { useLanguage } from "../hooks/useLanguage";
import { backOfficeTranslator, cashEventTypeText } from "../i18n/backOfficeStrings";

interface Props {
  shiftId:       string;
  sessionToken:  SessionToken;
  onClose:       () => void;
}

export default function XReportModal({ shiftId, sessionToken, onClose }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => backOfficeTranslator(language), [language]);
  const [summary, setSummary] = useState<CashDrawerSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError]     = useState<string | null>(null);
  const [printedAt]           = useState(() => new Date().toLocaleString(language === "ar" ? "ar-BH" : "en-BH", { dateStyle: "medium", timeStyle: "short" }));

  useEffect(() => {
    let cancelled = false;
    cashXReport(shiftId, sessionToken)
      .then(data => { if (!cancelled) setSummary(data); })
      .catch(e => { if (!cancelled) setError(typeof e === "string" ? e : t("failedLoadReport")); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [shiftId, sessionToken, t]);

  const fmt = (n: number) => `${DEVICE.currency} ${formatMoney(n, DEVICE.currency_exponent)}`;

  return (
    <button className="modal-overlay" type="button" onClick={onClose}>
      <div role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}  className="modal xreport-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2 className="modal-title">📊 {t("xReportMidShift")}</h2>
          <button className="modal-close" onClick={onClose}>✕</button>
        </div>

        {loading && <div className="bo-empty">{t("loading")}</div>}
        {error   && <div className="modal-error">{error}</div>}

        {summary && (
          <div className="xreport-body">
            <div className="xreport-printed">{t("printedAt")}: {printedAt}</div>

            <table className="xreport-table">
              <tbody>
                <tr><td>{t("openingFloat")}</td><td className="xreport-val">{fmt(summary.opening_minor)}</td></tr>
                <tr className="xreport-plus"><td>+ {t("cashSales")}</td><td className="xreport-val">{fmt(summary.cash_sales_minor)}</td></tr>
                {summary.pending_delivery_cash_minor > 0 && (
                  <tr className="xreport-pending-delivery">
                    <td>⏳ {t("pendingDeliveryCash")}</td>
                    <td className="xreport-val xreport-warning">{fmt(summary.pending_delivery_cash_minor)}</td>
                  </tr>
                )}
                <tr className="xreport-minus"><td>− {t("cashRefunds")}</td><td className="xreport-val">{fmt(summary.cash_refunds_minor)}</td></tr>
                <tr className="xreport-plus"><td>+ {t("paidIn")}</td><td className="xreport-val">{fmt(summary.paid_in_minor)}</td></tr>
                <tr className="xreport-minus"><td>− {t("paidOut")}</td><td className="xreport-val">{fmt(summary.paid_out_minor)}</td></tr>
                <tr className="xreport-expected">
                  <td><strong>{t("expectedInDrawer")}</strong></td>
                  <td className="xreport-val"><strong>{fmt(summary.expected_minor)}</strong></td>
                </tr>
              </tbody>
            </table>

            {summary.events.length > 0 && (
              <>
                <div className="xreport-events-title">{t("cashEvents")}</div>
                <div className="xreport-events">
                  {summary.events.map(ev => (
                    <div key={ev.cash_event_id} className="xreport-event-row">
                      <span className={`xreport-event-type ${ev.event_type === "paid_in" ? "xreport-in" : "xreport-out"}`}>
                        {ev.event_type === "paid_in" ? "+" : "−"} {fmt(ev.amount_minor)}
                      </span>
                      <span className="xreport-event-note">{ev.note ?? cashEventTypeText(language, ev.event_type)}</span>
                      <span className="xreport-event-time">
                        {new Date(ev.created_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
                      </span>
                    </div>
                  ))}
                </div>
              </>
            )}
          </div>
        )}

        <div className="modal-actions">
          <button className="btn-secondary" onClick={() => window.print()}>🖨 {t("print")}</button>
          <button className="btn-primary" onClick={onClose}>{t("close")}</button>
        </div>
      </div>
    </button>
  );
}
