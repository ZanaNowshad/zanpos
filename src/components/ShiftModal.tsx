import { useEffect, useMemo, useState } from "react";
import type { CashDrawerSummary, SessionUser, Shift, TodaySummary } from "../types";
import { DEVICE } from "../types";
import { shiftOpen, shiftClose, shiftGetActive, reportToday, cashDrawerSummary, printReceiptRaw } from "../tauri/commands";
import { formatMoney, parseMoney } from "../money";
import EodReprintQueue from "./EodReprintQueue";
import { useLanguage } from "../hooks/useLanguage";
import { modalTranslator } from "../i18n/modalStrings";
import { detailTranslator } from "../i18n/detailStrings";

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
  const { language } = useLanguage();
  const t = useMemo(() => modalTranslator(language), [language]);
  const dt = useMemo(() => detailTranslator(language), [language]);
  const [openingCash, setOpeningCash] = useState("");
  const [countedCash, setCountedCash] = useState("");
  const [notes, setNotes] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [alreadyOpen, setAlreadyOpen] = useState(false);
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
    let cancelled = false;
    const today = new Date().toLocaleDateString("en-CA", { timeZone: "Asia/Bahrain" });
    reportToday(user.user_id, DEVICE.branch_id, today)
      .then(data => { if (!cancelled) setTodaySummary(data); })
      .catch((e: unknown) => { console.warn("Failed to load today's report for Z-report:", e); });
    if (shift) {
      cashDrawerSummary(user.user_id, shift.shift_id)
        .then(data => { if (!cancelled) setDrawerSummary(data); })
        .catch((e: unknown) => { console.warn("Failed to load cash drawer summary:", e); });
    }
    return () => { cancelled = true; };
  }, [mode, shift, user.user_id]);

  const handleOpen = async () => {
    setLoading(true);
    setError(null);
    setAlreadyOpen(false);
    try {
      const cashMinor = openingCash ? parseMoney(openingCash, DEVICE.currency_exponent) : 0;
      const opened = await shiftOpen(DEVICE.branch_id, DEVICE.device_id, user.user_id, cashMinor);
      onShiftOpened(opened);
    } catch (e: unknown) {
      const msg = typeof e === "string" ? e : t("failedOpenShift");
      setError(msg);
      if (msg.toLowerCase().includes("already open")) setAlreadyOpen(true);
    } finally {
      setLoading(false);
    }
  };

  const handleResumeShift = async () => {
    setLoading(true);
    setError(null);
    try {
      const active = await shiftGetActive(DEVICE.device_id, user.user_id);
      if (active) { onShiftOpened(active); return; }
      setError(dt("activeShiftMissing"));
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : t("failedResumeShift"));
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
        const countedMinor = parseMoney(countedCash, DEVICE.currency_exponent);
        const variance = countedMinor - drawerSummary.expected_minor;
        lines.push(`Counted:        ${fm(countedMinor)}`);
        lines.push(`Variance:       ${variance >= 0 ? "+" : ""}${fm(variance)}`);
      }
    }
    lines.push("================================");
    lines.push(" ");

    try {
      await printReceiptRaw(user.user_id, DEVICE.branch_name, lines);
    } catch {
      // Non-fatal — thermal printer may not be configured
    }
  };

  const handleClose = async () => {
    if (!shift) return;
    setLoading(true);
    setError(null);
    try {
      const countedMinor = countedCash ? parseMoney(countedCash, DEVICE.currency_exponent) : undefined;
      await shiftClose(shift.shift_id, user.user_id, countedMinor, notes || undefined);
      onShiftClosed();
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : t("failedCloseShift"));
    } finally {
      setLoading(false);
    }
  };

  return (
    <button className="modal-overlay" type="button">
      <div className="modal shift-modal" role="dialog" aria-modal="true" aria-labelledby="shift-dialog-title">
        {mode === "open" ? (
          <>
            <h2 className="modal-title" id="shift-dialog-title">{t("openShift")}</h2>
            <p className="shift-info-text">
              {t("startingShiftFor")} <strong>{user.display_name}</strong> {t("on")} {DEVICE.branch_name}
            </p>
            <label className="field-label">{t("openingCash")} ({DEVICE.currency})</label>
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
                  {t("cancel")}
                </button>
              )}
              {alreadyOpen ? (
                <button className="modal-btn-primary" onClick={handleResumeShift} disabled={loading}>
                  {loading ? t("resuming") : <>{t("resumeActiveShift")} <span className="icon-directional" aria-hidden="true">→</span></>}
                </button>
              ) : (
                <button className="modal-btn-primary" onClick={handleOpen} disabled={loading}>
                  {loading ? t("opening") : t("openShift")}
                </button>
              )}
            </div>
          </>
        ) : (
          <>
            <h2 className="modal-title" id="shift-dialog-title">{t("closeShiftReport")}</h2>
            {shift && (
              <div className="shift-summary">
                <div className="shift-summary-row">
                  <span>{t("openedBy")}</span>
                  <span>{shift.cashier_name}</span>
                </div>
                <div className="shift-summary-row">
                  <span>{t("openingCash")}</span>
                  <span>{DEVICE.currency} {formatMoney(shift.opening_cash_minor, DEVICE.currency_exponent)}</span>
                </div>
              </div>
            )}

            {todaySummary && (
              <div className="zreport">
                <div className="zreport-title">{t("todaysTotals")} ({todaySummary.business_date})</div>
                <div className="zreport-grid">
                  <div className="zreport-row">
                    <span>{t("transactions")}</span>
                    <span>{todaySummary.transaction_count}</span>
                  </div>
                  <div className="zreport-row">
                    <span>{t("grossSales")}</span>
                    <span>{DEVICE.currency} {formatMoney(todaySummary.gross_total_minor, DEVICE.currency_exponent)}</span>
                  </div>
                  <div className="zreport-row">
                    <span>{t("discounts")}</span>
                    <span>- {DEVICE.currency} {formatMoney(todaySummary.discount_total_minor, DEVICE.currency_exponent)}</span>
                  </div>
                  <div className="zreport-row">
                    <span>{t("taxCollected")}</span>
                    <span>{DEVICE.currency} {formatMoney(todaySummary.tax_total_minor, DEVICE.currency_exponent)}</span>
                  </div>
                  <div className="zreport-row zreport-row-total">
                    <span>{t("netTotal")}</span>
                    <span>{DEVICE.currency} {formatMoney(todaySummary.net_total_minor, DEVICE.currency_exponent)}</span>
                  </div>
                  <div className="zreport-row">
                    <span>{t("cashSales")}</span>
                    <span>{DEVICE.currency} {formatMoney(todaySummary.cash_total_minor, DEVICE.currency_exponent)}</span>
                  </div>
                  <div className="zreport-row">
                    <span>{t("cardOtherSales")}</span>
                    <span>{DEVICE.currency} {formatMoney(todaySummary.card_total_minor, DEVICE.currency_exponent)}</span>
                  </div>
                  {todaySummary.refund_count > 0 && (
                    <div className="zreport-row zreport-row-refund">
                      <span>{t("refunds")} ({todaySummary.refund_count})</span>
                      <span>- {DEVICE.currency} {formatMoney(todaySummary.refund_total_minor, DEVICE.currency_exponent)}</span>
                    </div>
                  )}
                </div>
              </div>
            )}

            {/* ── Cash Drawer Reconciliation ── */}
            {drawerSummary && (
              <div className="zreport cash-recon">
                <div className="zreport-title">{t("cashDrawerReconciliation")}</div>
                <div className="zreport-grid">
                  <div className="zreport-row">
                    <span>{t("openingFloat")}</span>
                    <span>+ {DEVICE.currency} {formatMoney(drawerSummary.opening_minor, DEVICE.currency_exponent)}</span>
                  </div>
                  <div className="zreport-row">
                    <span>{t("cashSales")}</span>
                    <span>+ {DEVICE.currency} {formatMoney(drawerSummary.cash_sales_minor, DEVICE.currency_exponent)}</span>
                  </div>
                  {drawerSummary.cash_refunds_minor > 0 && (
                    <div className="zreport-row zreport-row-refund">
                      <span>{t("cashRefunds")}</span>
                      <span>- {DEVICE.currency} {formatMoney(drawerSummary.cash_refunds_minor, DEVICE.currency_exponent)}</span>
                    </div>
                  )}
                  {drawerSummary.paid_in_minor > 0 && (
                    <div className="zreport-row">
                      <span>{t("paidIn")}</span>
                      <span>+ {DEVICE.currency} {formatMoney(drawerSummary.paid_in_minor, DEVICE.currency_exponent)}</span>
                    </div>
                  )}
                  {drawerSummary.paid_out_minor > 0 && (
                    <div className="zreport-row zreport-row-refund">
                      <span>{t("paidOut")}</span>
                      <span>- {DEVICE.currency} {formatMoney(drawerSummary.paid_out_minor, DEVICE.currency_exponent)}</span>
                    </div>
                  )}
                  {drawerSummary.safe_drop_minor > 0 && (
                    <div className="zreport-row zreport-row-refund">
                      <span>{t("safeDrops")}</span>
                      <span>- {DEVICE.currency} {formatMoney(drawerSummary.safe_drop_minor, DEVICE.currency_exponent)}</span>
                    </div>
                  )}
                  <div className="zreport-divider" />
                  <div className="zreport-row zreport-row-cash">
                    <span>{t("expectedInDrawer")}</span>
                    <span>{DEVICE.currency} {formatMoney(drawerSummary.expected_minor, DEVICE.currency_exponent)}</span>
                  </div>
                </div>

                {/* Paid-in / Paid-out event list */}
                {drawerSummary.events.length > 0 && (
                  <div className="cash-events-list">
                    <div className="cash-events-list-title">{t("cashEvents")}</div>
                    {drawerSummary.events.map(ev => (
                      <div key={ev.cash_event_id} className="cash-event-item">
                        <span className={`cash-event-badge ${ev.event_type === "paid_in" ? "cash-event-badge-in" : "cash-event-badge-out"}`}>
                          {ev.event_type === "paid_in" ? t("paidIn") : ev.event_type === "safe_drop" ? t("safeDrop") : t("paidOut")}
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

            <EodReprintQueue actorUserId={user.user_id} />

            {/* ── Denomination count grid ── */}
            {denomSet && (
              <div className="denom-section">
                <div className="denom-title">{t("countCashByDenomination")}</div>
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
                    {t("total")}: {DEVICE.currency} {formatMoney(denomTotalMinor, DEVICE.currency_exponent)}
                  </div>
                )}
              </div>
            )}

            <label className="field-label">{t("countedCash")} ({DEVICE.currency})</label>
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
              const countedMinor = parseMoney(countedCash, DEVICE.currency_exponent);
              const variance = countedMinor - drawerSummary.expected_minor;
              const label = variance === 0 ? t("exact") : variance > 0 ? t("over") : t("under");
              return (
                <div className={`cash-variance ${variance < 0 ? "cash-variance-under" : variance > 0 ? "cash-variance-over" : "cash-variance-exact"}`}>
                  {variance === 0
                    ? `✓ ${t("cashBalanced")} — ${DEVICE.currency} ${formatMoney(countedMinor, DEVICE.currency_exponent)}`
                    : `${t("variance")}: ${variance > 0 ? "+" : ""}${DEVICE.currency} ${formatMoney(Math.abs(variance), DEVICE.currency_exponent)}`}
                  {" "}
                  <span className="cash-variance-chip">{label}</span>
                </div>
              );
            })()}
            <label className="field-label">{t("notesOptional")}</label>
            <textarea
              className="field-input"
              rows={2}
              value={notes}
              onChange={e => setNotes(e.target.value)}
              placeholder={t("endOfDayNotes")}
            />
            {error && <div className="modal-error">{error}</div>}
            <div className="modal-actions">
              {onCancel && (
                <button className="modal-btn-secondary" onClick={onCancel} disabled={loading}>
                  {t("cancel")}
                </button>
              )}
              {todaySummary && (
                <button
                  className="modal-btn-secondary"
                  onClick={handlePrintZReport}
                  disabled={loading}
                  title={t("printZReport")}
                >
                  🖨 {t("printZReport")}
                </button>
              )}
              <button className="modal-btn-danger" onClick={handleClose} disabled={loading}>
                {loading ? t("closing") : t("closeShift")}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
