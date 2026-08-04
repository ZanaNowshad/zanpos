import { useState, useRef, useEffect, useMemo } from "react";
import { DEVICE } from "../types";
import { formatMoney, parseMoney } from "../money";
import { cashEventCreate, printReceiptRaw } from "../tauri/commands";
import Dialpad, { applyDialpadKey } from "./Dialpad";
import { useLanguage } from "../hooks/useLanguage";
import { modalTranslator } from "../i18n/modalStrings";
import { detailTranslator } from "../i18n/detailStrings";

interface Props {
  shiftId:     string;
  userId:      string;
  cashierName: string;
  onDone:      () => void;
  onCancel:    () => void;
}

interface RecordedEvent {
  type: "paid_in" | "paid_out" | "safe_drop";
  amountMinor: number;
  note: string;
  at: Date;
}

const TYPE_META = {
  paid_in:   { label: "Paid In",   icon: "＋", color: "var(--success)", desc: "Record cash added to the drawer (e.g. change fund, petty cash)." },
  paid_out:  { label: "Paid Out",  icon: "－", color: "var(--error)",   desc: "Record cash removed from the drawer (e.g. expense, deposit)." },
  safe_drop: { label: "Safe Drop", icon: "⬇", color: "var(--warning)",  desc: "Transfer cash from drawer to safe. Reduces expected drawer total." },
} as const;

export default function CashEventModal({ shiftId, userId, cashierName, onDone, onCancel }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => modalTranslator(language), [language]);
  const dt = useMemo(() => detailTranslator(language), [language]);
  const [eventType, setEventType] = useState<"paid_in" | "paid_out" | "safe_drop">("paid_in");
  const [amount, setAmount]       = useState("");
  const [note, setNote]           = useState("");
  const [loading, setLoading]     = useState(false);
  const [error, setError]         = useState<string | null>(null);
  const [recorded, setRecorded]   = useState<RecordedEvent | null>(null);
  const [printing, setPrinting]   = useState(false);
  const confirmRef = useRef<HTMLButtonElement>(null);
  const noteRef    = useRef<HTMLInputElement>(null);

  const exp = DEVICE.currency_exponent;
  const cur = DEVICE.currency;
  const meta = TYPE_META[eventType];
  const displayLabel = eventType === "paid_in" ? t("paidIn") : eventType === "safe_drop" ? t("safeDrop") : t("paidOut");
  const description = eventType === "paid_in" ? dt("paidInDescription") : eventType === "safe_drop" ? dt("safeDropDescription") : dt("paidOutDescription");

  // Parse amount using integer arithmetic
  const amountMinor = amount ? parseMoney(amount, exp) : 0;
  const previewStr  = amountMinor > 0 ? formatMoney(amountMinor, exp) : null;
  const needsNote   = eventType === "paid_out" || eventType === "safe_drop";
  const canConfirm  = amountMinor > 0 && (!needsNote || note.trim().length > 0);

  // Auto-focus confirm when amount is set
  useEffect(() => {
    if (canConfirm) {
      const t = setTimeout(() => confirmRef.current?.focus(), 60);
      return () => clearTimeout(t);
    }
  }, [canConfirm]);

  const handleDialpadKey = (key: string) => {
    setAmount(prev => applyDialpadKey(prev, key));
    setError(null);
  };

  const handleConfirm = async () => {
    if (!canConfirm) {
      setError(needsNote && !note.trim() ? dt("reasonRequired") : dt("validAmountRequired"));
      return;
    }
    setLoading(true);
    setError(null);
    try {
      await cashEventCreate(shiftId, eventType, amountMinor, note.trim() || undefined, userId);
      setRecorded({ type: eventType, amountMinor, note: note.trim(), at: new Date() });
    } catch (e: unknown) {
      const msg = typeof e === "string" ? e : dt("failedCashEvent");
      setError(msg.toLowerCase().includes("not permitted") || msg.toLowerCase().includes("permission")
        ? dt("cashEventPermission") : msg);
    } finally {
      setLoading(false);
    }
  };

  const handlePrint = async () => {
    if (!recorded) return;
    setPrinting(true);
    const sep = "--------------------------------";
    const sign = recorded.type === "paid_in" ? "+" : "-";
    const amtStr = `${sign} ${cur} ${formatMoney(recorded.amountMinor, exp)}`;
    const dateStr = recorded.at.toLocaleString([], {
      day: "2-digit", month: "2-digit", year: "numeric",
      hour: "2-digit", minute: "2-digit",
    });
    const lines = [
      sep, "     CASH DRAWER EVENT", sep,
      `Type    : ${TYPE_META[recorded.type].label}`,
      `Amount  : ${amtStr}`,
      ...(recorded.note ? [`Note    : ${recorded.note}`] : []),
      "", `Date    : ${dateStr}`,
      `Cashier : ${cashierName}`,
      `Branch  : ${DEVICE.branch_name}`, sep,
    ];
    try { await printReceiptRaw(userId, DEVICE.branch_name, lines); }
    catch (e) { console.error("Print failed", e); }
    finally { setPrinting(false); }
  };

  // ── Success state ─────────────────────────────────────────────────────────
  if (recorded) {
    const sign = recorded.type === "paid_in" ? "+" : "−";
    const m = TYPE_META[recorded.type];
    return (
      <div className="modal-overlay">
        <div className="cash-event-shell cash-event-success-shell">
          <div className="cash-event-success">
            <div className="cash-event-success-icon" style={{ color: m.color }}>✓</div>
            <div className="cash-event-success-type">{recorded.type === "paid_in" ? t("paidIn") : recorded.type === "safe_drop" ? t("safeDrop") : t("paidOut")}</div>
            <div className="cash-event-success-amount" style={{ color: m.color }}>
              {sign} {cur} {formatMoney(recorded.amountMinor, exp)}
            </div>
            {recorded.note && <div className="cash-event-success-note">{recorded.note}</div>}
            <div className="cash-event-success-actions">
              <button className="dialpad-cancel-btn" style={{ flex: 1 }} onClick={onDone}>{t("done")}</button>
              <button className="dialpad-confirm-btn" style={{ flex: 1, padding: "14px" }} onClick={handlePrint} disabled={printing}>
                <span className="dialpad-confirm-icon">🖨</span>
                <span className="dialpad-confirm-label">{printing ? dt("printing") : dt("printReceipt")}</span>
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // ── Form ──────────────────────────────────────────────────────────────────
  return (
    <button className="modal-overlay" type="button" onClick={e => e.target === e.currentTarget && onCancel()}>
      <div className="cash-event-shell">

        {/* ── Left: form ── */}
        <div className="modal cash-event-left">

          <h2 className="modal-title">{t("cashDrawerReconciliation")}</h2>

          {/* Event type tabs */}
          <div className="ce-type-tabs">
            {(["paid_in", "paid_out", "safe_drop"] as const).map(type => (
              <button
                key={type}
                className={`ce-type-tab${eventType === type ? " ce-type-tab-active" : ""}`}
                style={eventType === type ? { borderColor: TYPE_META[type].color, color: TYPE_META[type].color } : {}}
                onClick={() => { setEventType(type); setError(null); }}
              >
                <span className="ce-tab-icon">{TYPE_META[type].icon}</span>
                <span className="ce-tab-label">{type === "paid_in" ? t("paidIn") : type === "safe_drop" ? t("safeDrop") : t("paidOut")}</span>
              </button>
            ))}
          </div>

          <p className="ce-desc">{description}</p>

          {/* Amount display (touch-input style) */}
          <div className="ce-amount-block">
            <span className="ce-amount-cur">{cur}</span>
            <span className={`ce-amount-value${!previewStr ? " ce-amount-placeholder" : ""}`}
              style={previewStr ? { color: meta.color } : {}}>
              {previewStr ?? `0.${"0".repeat(exp)}`}
            </span>
            <span className="touch-cursor">|</span>
          </div>

          {/* Note */}
          <label htmlFor="a11y-wrap-CashEventModal" className="ce-note-label">
            {needsNote ? t("reasonRequired") : t("noteOptional")}
          </label>
          <input id="a11y-wrap-CashEventModal"
            ref={noteRef}
            className="ce-note-input"
            type="text"
            placeholder={
              eventType === "paid_in"
                ? dt("optionalNote")
                : eventType === "safe_drop"
                ? dt("dropLocation")
                : dt("removalReason")
            }
            value={note}
            onChange={e => { setNote(e.target.value); setError(null); }}
            onKeyDown={e => e.key === "Enter" && handleConfirm()}
          />

          {error && <div className="modal-error">{error}</div>}
        </div>

        {/* ── Right: dialpad ── */}
        <div className="payment-dialpad-panel">
          <div className="dialpad-field-indicator" style={{ borderColor: `${meta.color}55`, color: meta.color }}>
            {displayLabel}
          </div>

          <Dialpad onKey={handleDialpadKey} />

          <div className="dialpad-actions">
            <button
              ref={confirmRef}
              className="dialpad-confirm-btn"
              style={canConfirm ? { background: meta.color } : {}}
              onClick={handleConfirm}
              disabled={!canConfirm || loading}
            >
              {loading ? (
                <span className="dialpad-confirm-label">{t("saving")}</span>
              ) : (
                <>
                  <span className="dialpad-confirm-icon">✓</span>
                  <span className="dialpad-confirm-label">
                    {eventType === "paid_in" ? dt("recordPaidIn") : eventType === "safe_drop" ? dt("recordSafeDrop") : dt("recordPaidOut")}
                  </span>
                  {previewStr && (
                    <span className="dialpad-confirm-total">{cur} {previewStr}</span>
                  )}
                </>
              )}
            </button>
            <button className="dialpad-cancel-btn" onClick={onCancel} disabled={loading}>
              ✕ {t("cancel")}
            </button>
          </div>
        </div>

      </div>
    </button>
  );
}
