import { useEffect, useMemo, useState } from "react";
import type { CashDrawerSummary, SessionUser, Shift, TodaySummary } from "../types";
import { DEVICE } from "../types";
import { shiftOpen, shiftClose, reportToday, cashDrawerSummary, printReceiptRaw } from "../tauri/commands";
import { formatMoney } from "../money";

// ─── Denomination sets (minor units) per currency ─────────────────────────────
const DENOM_SETS: Record<string, { label: string; minor: number }[]> = {
  BHD: [
    { label: "BD 20",      minor: 20000 }, { label: "BD 10",      minor: 10000 },
    { label: "BD 5",       minor:  5000 }, { label: "BD 1",       minor:  1000 },
    { label: "500 fils",   minor:   500 }, { label: "100 fils",   minor:   100 },
    { label: "50 fils",    minor:    50 }, { label: "25 fils",    minor:    25 },
    { label: "10 fils",    minor:    10 }, { label: "5 fils",     minor:     5 },
  ],
  USD: [
    { label: "$100",  minor: 10000 }, { label: "$50",   minor:  5000 },
    { label: "$20",   minor:  2000 }, { label: "$10",   minor:  1000 },
    { label: "$5",    minor:   500 }, { label: "$1",    minor:   100 },
    { label: "50¢",   minor:    50 }, { label: "25¢",   minor:    25 },
    { label: "10¢",   minor:    10 }, { label: "5¢",    minor:     5 },
    { label: "1¢",    minor:     1 },
  ],
  EUR: [
    { label: "€200", minor: 20000 }, { label: "€100", minor: 10000 },
    { label: "€50",  minor:  5000 }, { label: "€20",  minor:  2000 },
    { label: "€10",  minor:  1000 }, { label: "€5",   minor:   500 },
    { label: "€2",   minor:   200 }, { label: "€1",   minor:   100 },
    { label: "50c",  minor:    50 }, { label: "20c",  minor:    20 },
    { label: "10c",  minor:    10 }, { label: "5c",   minor:     5 },
  ],
  GBP: [
    { label: "£50",  minor:  5000 }, { label: "£20",  minor:  2000 },
    { label: "£10",  minor:  1000 }, { label: "£5",   minor:   500 },
    { label: "£2",   minor:   200 }, { label: "£1",   minor:   100 },
    { label: "50p",  minor:    50 }, { label: "20p",  minor:    20 },
    { label: "10p",  minor:    10 }, { label: "5p",   minor:     5 },
  ],
  SAR: [
    { label: "SR 500", minor: 50000 }, { label: "SR 100", minor: 10000 },
    { label: "SR 50",  minor:  5000 }, { label: "SR 10",  minor:  1000 },
    { label: "SR 5",   minor:   500 }, { label: "SR 1",   minor:   100 },
    { label: "50 hal", minor:    50 }, { label: "25 hal", minor:    25 },
  ],
};

function denomsForCurrency(currency: string) {
  return DENOM_SETS[currency] ?? null;
}

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
  const [drawerSummary, setDrawerSummary] = useState<CashDrawerSummary | null>(null);

  // Denomination count state: maps denomination minor-value → count string
  const [denomCounts, setDenomCounts] = useState<Record<number, string>>({});
  const denomSet = useMemo(() => denomsForCurrency(DEVICE.currency), []);
  const denomTotalMinor = useMemo(() => {
    if (!denomSet) return 0;
    return denomSet.reduce((sum, d) => {
      const cnt = parseInt(denomCounts[d.minor] || "0", 10);
      return sum + (isNaN(cnt) ? 0 : cnt) * d.minor;
    }, 0);
  }, [denomSet, denomCounts]);

  // When denom grid produces a non-zero total, sync it to countedCash
  useEffect(() => {
    if (!denomSet) return;
    const allEmpty = Object.values(denomCounts).every(v => !v || v === "0");
    if (!allEmpty) {
      setCountedCash((denomTotalMinor / Math.pow(10, DEVICE.currency_exponent)).toFixed(DEVICE.currency_exponent));
    }
  }, [denomTotalMinor, denomSet, denomCounts]);

  // Fetch today's Z-report and cash drawer summary when closing a shift
  useEffect(() => {
    if (mode !== "close") return;
    const today = new Date().toISOString().slice(0, 10);
    reportToday(DEVICE.branch_id, today)
      .then(setTodaySummary)
      .catch(() => {}); // non-fatal
    if (shift) {
      cashDrawerSummary(shift.shift_id)
        .then(setDrawerSummary)
        .catch(() => {}); // non-fatal
    }
  }, [mode, shift]);

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

  const handlePrintZReport = async () => {
    if (!todaySummary || !shift) return;
    const exp = DEVICE.currency_exponent;
    const cur = DEVICE.currency;
    const fm = (v: number) => `${cur} ${formatMoney(v, exp)}`;

    const lines: string[] = [
      "================================",
      "          Z-REPORT",
      "================================",
      `Date: ${todaySummary.business_date}`,
      `Cashier: ${shift.cashier_name}`,
      `Opened: ${new Date(shift.opened_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`,
      "--------------------------------",
      `Transactions:   ${todaySummary.transaction_count}`,
      `Gross Sales:    ${fm(todaySummary.gross_total_minor)}`,
      `Discounts:    - ${fm(todaySummary.discount_total_minor)}`,
      `Tax Collected:  ${fm(todaySummary.tax_total_minor)}`,
      `Net Total:      ${fm(todaySummary.net_total_minor)}`,
      "--------------------------------",
      `Cash:           ${fm(todaySummary.cash_total_minor)}`,
      `Card/Other:     ${fm(todaySummary.card_total_minor)}`,
      ...(todaySummary.refund_count > 0
        ? [`Refunds(${todaySummary.refund_count}):  - ${fm(todaySummary.refund_total_minor)}`]
        : []),
    ];

    if (drawerSummary) {
      lines.push("================================");
      lines.push("     CASH RECONCILIATION");
      lines.push("================================");
      lines.push(`Opening Float:  ${fm(drawerSummary.opening_minor)}`);
      lines.push(`Cash Sales:   + ${fm(drawerSummary.cash_sales_minor)}`);
      if (drawerSummary.cash_refunds_minor > 0)
        lines.push(`Cash Refunds: - ${fm(drawerSummary.cash_refunds_minor)}`);
      if (drawerSummary.paid_in_minor > 0)
        lines.push(`Paid In:      + ${fm(drawerSummary.paid_in_minor)}`);
      if (drawerSummary.paid_out_minor > 0)
        lines.push(`Paid Out:     - ${fm(drawerSummary.paid_out_minor)}`);
      if (drawerSummary.safe_drop_minor > 0)
        lines.push(`Safe Drops:   - ${fm(drawerSummary.safe_drop_minor)}`);
      lines.push("--------------------------------");
      lines.push(`Expected:       ${fm(drawerSummary.expected_minor)}`);
      if (countedCash) {
        const countedMinor = Math.round(parseFloat(countedCash) * 1000);
        const variance = countedMinor - drawerSummary.expected_minor;
        lines.push(`Counted:        ${fm(countedMinor)}`);
        lines.push(`Variance:       ${variance >= 0 ? "+" : ""}${fm(variance)}`);
      }
    }
    lines.push("================================");
    lines.push(" ");

    try {
      await printReceiptRaw(DEVICE.branch_name, lines);
    } catch {
      // Non-fatal — thermal printer may not be configured
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
                </div>
              </div>
            )}

            {/* ── Cash Drawer Reconciliation ── */}
            {drawerSummary && (
              <div className="zreport cash-recon">
                <div className="zreport-title">Cash Drawer Reconciliation</div>
                <div className="zreport-grid">
                  <div className="zreport-row">
                    <span>Opening Float</span>
                    <span>+ {DEVICE.currency} {formatMoney(drawerSummary.opening_minor, DEVICE.currency_exponent)}</span>
                  </div>
                  <div className="zreport-row">
                    <span>Cash Sales</span>
                    <span>+ {DEVICE.currency} {formatMoney(drawerSummary.cash_sales_minor, DEVICE.currency_exponent)}</span>
                  </div>
                  {drawerSummary.cash_refunds_minor > 0 && (
                    <div className="zreport-row zreport-row-refund">
                      <span>Cash Refunds</span>
                      <span>- {DEVICE.currency} {formatMoney(drawerSummary.cash_refunds_minor, DEVICE.currency_exponent)}</span>
                    </div>
                  )}
                  {drawerSummary.paid_in_minor > 0 && (
                    <div className="zreport-row">
                      <span>Paid In</span>
                      <span>+ {DEVICE.currency} {formatMoney(drawerSummary.paid_in_minor, DEVICE.currency_exponent)}</span>
                    </div>
                  )}
                  {drawerSummary.paid_out_minor > 0 && (
                    <div className="zreport-row zreport-row-refund">
                      <span>Paid Out</span>
                      <span>- {DEVICE.currency} {formatMoney(drawerSummary.paid_out_minor, DEVICE.currency_exponent)}</span>
                    </div>
                  )}
                  {drawerSummary.safe_drop_minor > 0 && (
                    <div className="zreport-row zreport-row-refund">
                      <span>Safe Drops</span>
                      <span>- {DEVICE.currency} {formatMoney(drawerSummary.safe_drop_minor, DEVICE.currency_exponent)}</span>
                    </div>
                  )}
                  <div className="zreport-divider" />
                  <div className="zreport-row zreport-row-cash">
                    <span>Expected in Drawer</span>
                    <span>{DEVICE.currency} {formatMoney(drawerSummary.expected_minor, DEVICE.currency_exponent)}</span>
                  </div>
                </div>

                {/* Paid-in / Paid-out event list */}
                {drawerSummary.events.length > 0 && (
                  <div className="cash-events-list">
                    <div className="cash-events-list-title">Cash Events</div>
                    {drawerSummary.events.map(ev => (
                      <div key={ev.cash_event_id} className="cash-event-item">
                        <span className={`cash-event-badge ${ev.event_type === "paid_in" ? "cash-event-badge-in" : "cash-event-badge-out"}`}>
                          {ev.event_type === "paid_in" ? "Paid In" : ev.event_type === "safe_drop" ? "Safe Drop" : "Paid Out"}
                        </span>
                        <span className="cash-event-amount">
                          {ev.event_type === "paid_in" ? "+" : "-"} {DEVICE.currency} {formatMoney(ev.amount_minor, DEVICE.currency_exponent)}
                        </span>
                        {ev.note && <span className="cash-event-note">{ev.note}</span>}
                        <span className="cash-event-time">{new Date(ev.created_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</span>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            )}

            {/* ── Denomination count grid ── */}
            {denomSet && (
              <div className="denom-section">
                <div className="denom-title">Count Cash by Denomination</div>
                <div className="denom-grid">
                  {denomSet.map(d => (
                    <div key={d.minor} className="denom-row">
                      <span className="denom-label">{d.label}</span>
                      <input
                        className="denom-count-input"
                        type="number"
                        min="0"
                        step="1"
                        placeholder="0"
                        value={denomCounts[d.minor] ?? ""}
                        onChange={e => setDenomCounts(prev => ({ ...prev, [d.minor]: e.target.value }))}
                      />
                      {(() => {
                        const cnt = parseInt(denomCounts[d.minor] || "0", 10);
                        const sub = isNaN(cnt) ? 0 : cnt * d.minor;
                        return sub > 0 ? (
                          <span className="denom-subtotal">
                            {DEVICE.currency} {formatMoney(sub, DEVICE.currency_exponent)}
                          </span>
                        ) : null;
                      })()}
                    </div>
                  ))}
                </div>
                {denomTotalMinor > 0 && (
                  <div className="denom-total">
                    Total: {DEVICE.currency} {formatMoney(denomTotalMinor, DEVICE.currency_exponent)}
                  </div>
                )}
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
            {countedCash && drawerSummary && (() => {
              const countedMinor = Math.round(parseFloat(countedCash) * 1000);
              const variance = countedMinor - drawerSummary.expected_minor;
              const label = variance === 0 ? "EXACT" : variance > 0 ? "OVER" : "UNDER";
              return (
                <div className={`cash-variance ${variance < 0 ? "cash-variance-under" : variance > 0 ? "cash-variance-over" : "cash-variance-exact"}`}>
                  {variance === 0
                    ? `✓ Cash balanced — ${DEVICE.currency} ${formatMoney(countedMinor, DEVICE.currency_exponent)}`
                    : `Variance: ${variance > 0 ? "+" : ""}${DEVICE.currency} ${formatMoney(Math.abs(variance), DEVICE.currency_exponent)}`}
                  {" "}
                  <span className="cash-variance-chip">{label}</span>
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
              {todaySummary && (
                <button
                  className="modal-btn-secondary"
                  onClick={handlePrintZReport}
                  disabled={loading}
                  title="Print Z-Report to thermal printer"
                >
                  🖨 Print Z-Report
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
