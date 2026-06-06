import { useEffect, useState } from "react";
import type { TodaySummary } from "../types";
import { DEVICE } from "../types";
import { reportToday } from "../tauri/commands";
import { formatMoney } from "../money";

interface Props {
  onClose: () => void;
}

export default function TodayReportModal({ onClose }: Props) {
  const [summary, setSummary] = useState<TodaySummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const today = new Date().toISOString().split("T")[0];

  useEffect(() => {
    let cancelled = false;
    reportToday("", DEVICE.branch_id, today)
      .then(data => { if (!cancelled) setSummary(data); })
      .catch(() => { if (!cancelled) setError("Failed to load report"); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [today]);

  const fmt = (minor: number) =>
    `${DEVICE.currency} ${formatMoney(minor, DEVICE.currency_exponent)}`;

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal report-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2 className="modal-title">Today's Sales — {today}</h2>
          <button className="modal-close" onClick={onClose}>✕</button>
        </div>

        {loading && <div className="report-loading">Loading…</div>}
        {error && <div className="modal-error">{error}</div>}

        {summary && (
          <div className="report-body">
            <div className="report-section">
              <div className="report-row">
                <span>Transactions</span>
                <strong>{summary.transaction_count}</strong>
              </div>
              <div className="report-row">
                <span>Gross Sales</span>
                <strong>{fmt(summary.gross_total_minor)}</strong>
              </div>
              <div className="report-row">
                <span>Discounts</span>
                <strong className="report-negative">{fmt(summary.discount_total_minor)}</strong>
              </div>
              <div className="report-row">
                <span>Tax Collected</span>
                <strong>{fmt(summary.tax_total_minor)}</strong>
              </div>
              <div className="report-row report-row-total">
                <span>Net Revenue</span>
                <strong>{fmt(summary.net_total_minor)}</strong>
              </div>
            </div>

            <div className="report-section">
              <div className="report-section-title">By Payment Method</div>
              <div className="report-row">
                <span>Cash</span>
                <strong>{fmt(summary.cash_total_minor)}</strong>
              </div>
              <div className="report-row">
                <span>Card</span>
                <strong>{fmt(summary.card_total_minor)}</strong>
              </div>
            </div>

            {summary.pending_delivery_count > 0 && (
              <div className="report-section report-section-warning">
                <div className="report-section-title">⏳ Pending Deliveries (Unpaid)</div>
                <div className="report-row">
                  <span>Pending Orders</span>
                  <strong>{summary.pending_delivery_count}</strong>
                </div>
                <div className="report-row">
                  <span>Pending Revenue</span>
                  <strong className="report-warning">{fmt(summary.pending_delivery_minor)}</strong>
                </div>
                <div className="report-hint">These sales are excluded from totals above until payment is confirmed.</div>
              </div>
            )}

            {summary.refund_count > 0 && (
              <div className="report-section">
                <div className="report-section-title">Refunds</div>
                <div className="report-row">
                  <span>Refund Count</span>
                  <strong>{summary.refund_count}</strong>
                </div>
                <div className="report-row">
                  <span>Refund Total</span>
                  <strong className="report-negative">{fmt(summary.refund_total_minor)}</strong>
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
