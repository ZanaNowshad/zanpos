import { useEffect, useMemo, useState } from "react";
import type { TodaySummary } from "../types";
import { DEVICE } from "../types";
import { reportToday } from "../tauri/commands";
import { formatMoney } from "../money";
import { useLanguage } from "../hooks/useLanguage";
import { backOfficeTranslator } from "../i18n/backOfficeStrings";

interface Props {
  onClose: () => void;
  sessionUserId: string;
}

export default function TodayReportModal({ onClose, sessionUserId }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => backOfficeTranslator(language), [language]);
  const [summary, setSummary] = useState<TodaySummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // FIX: use Bahrain timezone — toISOString() returns UTC date which is wrong
  // between midnight and 03:00 Bahrain time (UTC+3)
  const today = new Date().toLocaleDateString("en-CA", { timeZone: "Asia/Bahrain" });

  useEffect(() => {
    let cancelled = false;
    reportToday(sessionUserId, DEVICE.branch_id, today)
      .then(data => { if (!cancelled) setSummary(data); })
      .catch(() => { if (!cancelled) setError(t("failedLoadReport")); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [today, sessionUserId, t]);

  const fmt = (minor: number) =>
    `${DEVICE.currency} ${formatMoney(minor, DEVICE.currency_exponent)}`;

  return (
    <button className="modal-overlay" type="button" onClick={onClose}>
      <div className="modal report-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2 className="modal-title">{t("todaysSales")} — {today}</h2>
          <button className="modal-close" onClick={onClose}>✕</button>
        </div>

        {loading && <div className="report-loading">{t("loading")}</div>}
        {error && <div className="modal-error">{error}</div>}

        {summary && (
          <div className="report-body">
            <div className="report-section">
              <div className="report-row">
                <span>{t("transactions")}</span>
                <strong>{summary.transaction_count}</strong>
              </div>
              <div className="report-row">
                <span>{t("grossSales")}</span>
                <strong>{fmt(summary.gross_total_minor)}</strong>
              </div>
              <div className="report-row">
                <span>{t("discounts")}</span>
                <strong className="report-negative">{fmt(summary.discount_total_minor)}</strong>
              </div>
              <div className="report-row">
                <span>{t("taxCollected")}</span>
                <strong>{fmt(summary.tax_total_minor)}</strong>
              </div>
              <div className="report-row report-row-total">
                <span>{t("netRevenue")}</span>
                <strong>{fmt(summary.net_total_minor)}</strong>
              </div>
            </div>

            <div className="report-section">
              <div className="report-section-title">{t("byPaymentMethod")}</div>
              <div className="report-row">
                <span>{t("cash")}</span>
                <strong>{fmt(summary.cash_total_minor)}</strong>
              </div>
              <div className="report-row">
                <span>{t("card")}</span>
                <strong>{fmt(summary.card_total_minor)}</strong>
              </div>
            </div>

            {summary.pending_delivery_count > 0 && (
              <div className="report-section report-section-warning">
                <div className="report-section-title">⏳ {t("pendingDeliveries")}</div>
                <div className="report-row">
                  <span>{t("pendingOrders")}</span>
                  <strong>{summary.pending_delivery_count}</strong>
                </div>
                <div className="report-row">
                  <span>{t("pendingRevenue")}</span>
                  <strong className="report-warning">{fmt(summary.pending_delivery_minor)}</strong>
                </div>
                <div className="report-hint">{t("pendingExcludedNote")}</div>
              </div>
            )}

            {summary.refund_count > 0 && (
              <div className="report-section">
                <div className="report-section-title">{t("refunds")}</div>
                <div className="report-row">
                  <span>{t("refundCount")}</span>
                  <strong>{summary.refund_count}</strong>
                </div>
                <div className="report-row">
                  <span>{t("refundTotal")}</span>
                  <strong className="report-negative">{fmt(summary.refund_total_minor)}</strong>
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </button>
  );
}
