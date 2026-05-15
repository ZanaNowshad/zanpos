import { useEffect, useState } from "react";
import type { SessionUser, Shift, TodaySummary } from "../types";
import { DEVICE } from "../types";
import { shiftOpen, shiftClose, reportToday } from "../tauri/commands";
import { formatMoney } from "../money";

interface Props {
  mode: "open" | "close";
  user: SessionUser;
  shift?: Shift;
  onShiftOpened: (shift: Shift) => void;
  onShiftClosed: () => void;
  onCancel?: () => void;
}

export default function ShiftModal({ mode, user, shift, onShiftOpened, onShiftClosed, onCancel }: Props) {
  const [openingCash, setOpeningCash] = useState("");
  const [countedCash, setCountedCash] = useState("");
  const [notes, setNotes] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [todaySummary, setTodaySummary] = useState<TodaySummary | null>(null);

  // Fetch today's Z-report when closing a shift
  useEffect(() => {
    if (mode !== "close") return;
    const today = new Date().toISOString().slice(0, 10);
    reportToday(DEVICE.branch_id, today)
      .then(setTodaySummary)
      .catch(() => {}); // non-fatal
  }, [mode]);

  const handleOpen = async () => {
    setLoading(true);
    setError(null);
    try {
      const cashMinor = openingCash ? Math.round(parseFloat(openingCash) * 1000) : 0;
      const opened = await shiftOpen(DEVICE.branch_id, DEVICE.device_id, user.user_id, cashMinor);
      onShiftOpened(opened);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to open shift");
    } finally {
      setLoading(false);
    }
  };

  const handleClose = async () => {
    if (!shift) return;
    setLoading(true);
    setError(null);
    try {
      const countedMinor = countedCash ? Math.round(parseFloat(countedCash) * 1000) : undefined;
      await shiftClose(shift.shift_id, countedMinor, notes || undefined);
      onShiftClosed();
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to close shift");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="modal-overlay">
      <div className="modal shift-modal">
        {mode === "open" ? (
          <>
            <h2 className="modal-title">Open Shift</h2>
            <p className="shift-info-text">
              Starting shift for <strong>{user.display_name}</strong> on {DEVICE.branch_name}
            </p>
            <label className="field-label">Opening Cash ({DEVICE.currency})</label>
            <input
              className="field-input"
              type="number"
              step="0.001"
              min="0"
              placeholder="0.000"
              value={openingCash}
              onChange={e => setOpeningCash(e.target.value)}
            />
            {error && <div className="modal-error">{error}</div>}
            <div className="modal-actions">
              {onCancel && (
                <button className="modal-btn-secondary" onClick={onCancel} disabled={loading}>
                  Cancel
                </button>
              )}
              <button className="modal-btn-primary" onClick={handleOpen} disabled={loading}>
                {loading ? "Opening…" : "Open Shift"}
              </button>
            </div>
          </>
        ) : (
          <>
            <h2 className="modal-title">Close Shift — Z-Report</h2>
            {shift && (
              <div className="shift-summary">
                <div className="shift-summary-row">
                  <span>Opened by</span>
                  <span>{shift.cashier_name}</span>
                </div>
                <div className="shift-summary-row">
                  <span>Opening cash</span>
                  <span>{DEVICE.currency} {formatMoney(shift.opening_cash_minor, DEVICE.currency_exponent)}</span>
                </div>
              </div>
            )}

            {todaySummary && (
              <div className="zreport">
                <div className="zreport-title">Today's Totals ({todaySummary.business_date})</div>
                <div className="zreport-grid">
                  <div className="zreport-row">
                    <span>Transactions</span>
                    <span>{todaySummary.transaction_count}</span>
                  </div>
                  <div className="zreport-row">
                    <span>Gross sales</span>
                    <span>{DEVICE.currency} {formatMoney(todaySummary.gross_total_minor, DEVICE.currency_exponent)}</span>
                  </div>
                  <div className="zreport-row">
                    <span>Discounts</span>
                    <span>- {DEVICE.currency} {formatMoney(todaySummary.discount_total_minor, DEVICE.currency_exponent)}</span>
                  </div>
                  <div className="zreport-row">
                    <span>Tax collected</span>
                    <span>{DEVICE.currency} {formatMoney(todaySummary.tax_total_minor, DEVICE.currency_exponent)}</span>
                  </div>
                  <div className="zreport-row zreport-row-total">
                    <span>Net total</span>
                    <span>{DEVICE.currency} {formatMoney(todaySummary.net_total_minor, DEVICE.currency_exponent)}</span>
                  </div>
                  <div className="zreport-row">
                    <span>Cash sales</span>
                    <span>{DEVICE.currency} {formatMoney(todaySummary.cash_total_minor, DEVICE.currency_exponent)}</span>
                  </div>
                  <div className="zreport-row">
                    <span>Card/other sales</span>
                    <span>{DEVICE.currency} {formatMoney(todaySummary.card_total_minor, DEVICE.currency_exponent)}</span>
                  </div>
                  {todaySummary.refund_count > 0 && (
                    <div className="zreport-row zreport-row-refund">
                      <span>Refunds ({todaySummary.refund_count})</span>
                      <span>- {DEVICE.currency} {formatMoney(todaySummary.refund_total_minor, DEVICE.currency_exponent)}</span>
                    </div>
                  )}
                  {/* Expected cash in drawer */}
                  <div className="zreport-row zreport-row-cash">
                    <span>Expected in drawer</span>
                    <span>{DEVICE.currency} {formatMoney(
                      shift ? shift.opening_cash_minor + todaySummary.cash_total_minor - todaySummary.refund_total_minor : 0,
                      DEVICE.currency_exponent
                    )}</span>
                  </div>
                </div>
              </div>
            )}
            <label className="field-label">Counted Cash ({DEVICE.currency})</label>
            <input
              className="field-input"
              type="number"
              step="0.001"
              min="0"
              placeholder="0.000"
              value={countedCash}
              onChange={e => setCountedCash(e.target.value)}
            />
            {countedCash && todaySummary && shift && (() => {
              const countedMinor = Math.round(parseFloat(countedCash) * 1000);
              const expectedMinor = shift.opening_cash_minor + todaySummary.cash_total_minor - todaySummary.refund_total_minor;
              const variance = countedMinor - expectedMinor;
              return (
                <div className={`cash-variance ${variance < 0 ? "cash-variance-under" : variance > 0 ? "cash-variance-over" : "cash-variance-exact"}`}>
                  {variance === 0 ? "✓ Cash balanced" : `Variance: ${variance > 0 ? "+" : ""}${DEVICE.currency} ${formatMoney(Math.abs(variance), DEVICE.currency_exponent)} ${variance > 0 ? "(over)" : "(short)"}`}
                </div>
              );
            })()}
            <label className="field-label">Notes (optional)</label>
            <textarea
              className="field-input"
              rows={2}
              value={notes}
              onChange={e => setNotes(e.target.value)}
              placeholder="End of day notes…"
            />
            {error && <div className="modal-error">{error}</div>}
            <div className="modal-actions">
              {onCancel && (
                <button className="modal-btn-secondary" onClick={onCancel} disabled={loading}>
                  Cancel
                </button>
              )}
              <button className="modal-btn-danger" onClick={handleClose} disabled={loading}>
                {loading ? "Closing…" : "Close Shift"}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
