import { useState, useRef, useEffect } from "react";
import { DEVICE } from "../types";
import { formatMoney, parseMoney } from "../money";
import { cashEventCreate, printReceiptRaw } from "../tauri/commands";
import Dialpad, { applyDialpadKey } from "./Dialpad";

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
      setError(needsNote && !note.trim() ? "A reason is required." : "Enter a valid amount.");
      return;
    }
    setLoading(true);
    setError(null);
    try {
      await cashEventCreate(shiftId, eventType, amountMinor, note.trim() || undefined, userId);
      setRecorded({ type: eventType, amountMinor, note: note.trim(), at: new Date() });
    } catch (e: unknown) {
      const msg = typeof e === "string" ? e : "Failed to record cash event";
      setError(msg.toLowerCase().includes("not permitted") || msg.toLowerCase().includes("permission")
        ? "Only managers and owners can record cash events." : msg);
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
            <div className="cash-event-success-type">{m.label}</div>
            <div className="cash-event-success-amount" style={{ color: m.color }}>
              {sign} {cur} {formatMoney(recorded.amountMinor, exp)}
            </div>
            {recorded.note && <div className="cash-event-success-note">{recorded.note}</div>}
            <div className="cash-event-success-actions">
              <button className="dialpad-cancel-btn" style={{ flex: 1 }} onClick={onDone}>Done</button>
              <button className="dialpad-confirm-btn" style={{ flex: 1, padding: "14px" }} onClick={handlePrint} disabled={printing}>
                <span className="dialpad-confirm-icon">🖨</span>
                <span className="dialpad-confirm-label">{printing ? "Printing…" : "Print Receipt"}</span>
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // ── Form ──────────────────────────────────────────────────────────────────
  return (
    <div className="modal-overlay" onClick={e => e.target === e.currentTarget && onCancel()}>
      <div className="cash-event-shell">

        {/* ── Left: form ── */}
        <div className="modal cash-event-left">

          <h2 className="modal-title">Cash Drawer</h2>

          {/* Event type tabs */}
          <div className="ce-type-tabs">
            {(["paid_in", "paid_out", "safe_drop"] as const).map(t => (
              <button
                key={t}
                className={`ce-type-tab${eventType === t ? " ce-type-tab-active" : ""}`}
                style={eventType === t ? { borderColor: TYPE_META[t].color, color: TYPE_META[t].color } : {}}
                onClick={() => { setEventType(t); setError(null); }}
              >
                <span className="ce-tab-icon">{TYPE_META[t].icon}</span>
                <span className="ce-tab-label">{TYPE_META[t].label}</span>
              </button>
            ))}
          </div>

          <p className="ce-desc">{meta.desc}</p>

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
          <label className="ce-note-label">
            {needsNote ? "Reason *" : "Note (optional)"}
          </label>
          <input
            ref={noteRef}
            className="ce-note-input"
            type="text"
            placeholder={
              eventType === "paid_in"
                ? "Optional note…"
                : eventType === "safe_drop"
                ? "Drop location / bag #…"
                : "Reason for removal…"
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
            {meta.label}
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
                <span className="dialpad-confirm-label">Recording…</span>
              ) : (
                <>
                  <span className="dialpad-confirm-icon">✓</span>
                  <span className="dialpad-confirm-label">
                    {eventType === "paid_in" ? "Record Paid In" : eventType === "safe_drop" ? "Record Safe Drop" : "Record Paid Out"}
                  </span>
                  {previewStr && (
                    <span className="dialpad-confirm-total">{cur} {previewStr}</span>
                  )}
                </>
              )}
            </button>
            <button className="dialpad-cancel-btn" onClick={onCancel} disabled={loading}>
              ✕ Cancel
            </button>
          </div>
        </div>

      </div>
    </div>
  );
}
