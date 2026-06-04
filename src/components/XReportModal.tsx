import { useEffect, useState } from "react";
import type { CashDrawerSummary } from "../types";
import { DEVICE } from "../types";
import { cashXReport } from "../tauri/commands";
import { formatMoney } from "../money";

interface Props {
  shiftId:       string;
  actorUserId:   string;
  onClose:       () => void;
}

export default function XReportModal({ shiftId, actorUserId, onClose }: Props) {
  const [summary, setSummary] = useState<CashDrawerSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError]     = useState<string | null>(null);
  const [printedAt]           = useState(() => new Date().toLocaleString([], { dateStyle: "medium", timeStyle: "short" }));

  useEffect(() => {
    let cancelled = false;
    cashXReport(shiftId, actorUserId)
      .then(data => { if (!cancelled) setSummary(data); })
      .catch(e => { if (!cancelled) setError(typeof e === "string" ? e : "Failed to load X-Report"); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [shiftId, actorUserId]);

  const fmt = (n: number) => `${DEVICE.currency} ${formatMoney(n, DEVICE.currency_exponent)}`;

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal xreport-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2 className="modal-title">📊 X-Report (Mid-Shift)</h2>
          <button className="modal-close" onClick={onClose}>✕</button>
        </div>

        {loading && <div className="bo-empty">Loading…</div>}
        {error   && <div className="modal-error">{error}</div>}

        {summary && (
          <div className="xreport-body">
            <div className="xreport-printed">Printed: {printedAt}</div>

            <table className="xreport-table">
              <tbody>
                <tr><td>Opening Float</td><td className="xreport-val">{fmt(summary.opening_minor)}</td></tr>
                <tr className="xreport-plus"><td>+ Cash Sales</td><td className="xreport-val">{fmt(summary.cash_sales_minor)}</td></tr>
                {summary.pending_delivery_cash_minor > 0 && (
                  <tr className="xreport-pending-delivery">
                    <td>⏳ Pending Delivery Cash</td>
                    <td className="xreport-val xreport-warning">{fmt(summary.pending_delivery_cash_minor)}</td>
                  </tr>
                )}
                <tr className="xreport-minus"><td>− Cash Refunds</td><td className="xreport-val">{fmt(summary.cash_refunds_minor)}</td></tr>
                <tr className="xreport-plus"><td>+ Paid In</td><td className="xreport-val">{fmt(summary.paid_in_minor)}</td></tr>
                <tr className="xreport-minus"><td>− Paid Out</td><td className="xreport-val">{fmt(summary.paid_out_minor)}</td></tr>
                <tr className="xreport-expected">
                  <td><strong>Expected in Drawer</strong></td>
                  <td className="xreport-val"><strong>{fmt(summary.expected_minor)}</strong></td>
                </tr>
              </tbody>
            </table>

            {summary.events.length > 0 && (
              <>
                <div className="xreport-events-title">Cash Events</div>
                <div className="xreport-events">
                  {summary.events.map(ev => (
                    <div key={ev.cash_event_id} className="xreport-event-row">
                      <span className={`xreport-event-type ${ev.event_type === "paid_in" ? "xreport-in" : "xreport-out"}`}>
                        {ev.event_type === "paid_in" ? "+" : "−"} {fmt(ev.amount_minor)}
                      </span>
                      <span className="xreport-event-note">{ev.note ?? ev.event_type}</span>
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
          <button className="btn-secondary" onClick={() => window.print()}>🖨 Print</button>
          <button className="btn-primary" onClick={onClose}>Close</button>
        </div>
      </div>
    </div>
  );
}
