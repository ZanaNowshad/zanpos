import { useState } from "react";
import { DEVICE } from "../types";
import { formatMoney } from "../money";
import { cashEventCreate } from "../tauri/commands";

interface Props {
  shiftId:    string;
  userId:     string;
  onDone:     () => void;
  onCancel:   () => void;
}

export default function CashEventModal({ shiftId, userId, onDone, onCancel }: Props) {
  const [eventType, setEventType] = useState<"paid_in" | "paid_out">("paid_in");
  const [amount, setAmount]       = useState("");
  const [note, setNote]           = useState("");
  const [loading, setLoading]     = useState(false);
  const [error, setError]         = useState<string | null>(null);

  const exp = DEVICE.currency_exponent;
  const cur = DEVICE.currency;

  const handleConfirm = async () => {
    const amountFloat = parseFloat(amount);
    if (!amount || isNaN(amountFloat) || amountFloat <= 0) {
      setError("Enter a valid positive amount."); return;
    }
    if (eventType === "paid_out" && !note.trim()) {
      setError("A reason is required for Paid Out."); return;
    }
    const amountMinor = Math.round(amountFloat * Math.pow(10, exp));
    if (amountMinor <= 0) { setError("Amount must be greater than zero."); return; }

    setLoading(true);
    setError(null);
    try {
      await cashEventCreate(
        shiftId,
        eventType,
        amountMinor,
        note.trim() || undefined,
        userId,
      );
      onDone();
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to record cash event");
    } finally {
      setLoading(false);
    }
  };

  const preview = amount && !isNaN(parseFloat(amount)) && parseFloat(amount) > 0
    ? formatMoney(Math.round(parseFloat(amount) * Math.pow(10, exp)), exp)
    : null;

  return (
    <div className="modal-overlay">
      <div className="modal cash-event-modal">
        <h2 className="modal-title">Cash Drawer</h2>

        {/* Segmented toggle */}
        <div className="cash-event-toggle">
          <button
            className={`cash-event-tab ${eventType === "paid_in" ? "cash-event-tab-active" : ""}`}
            onClick={() => { setEventType("paid_in"); setError(null); }}
          >
            Paid In
          </button>
          <button
            className={`cash-event-tab ${eventType === "paid_out" ? "cash-event-tab-active" : ""}`}
            onClick={() => { setEventType("paid_out"); setError(null); }}
          >
            Paid Out
          </button>
        </div>

        <p className="cash-event-desc">
          {eventType === "paid_in"
            ? "Record cash added to the drawer (e.g. change fund, petty cash)."
            : "Record cash removed from the drawer (e.g. expense, deposit)."}
        </p>

        <label className="field-label">Amount ({cur})</label>
        <input
          className="field-input"
          type="number"
          min="0"
          step={Math.pow(10, -exp).toFixed(exp)}
          placeholder={`0.${"0".repeat(exp)}`}
          value={amount}
          onChange={e => setAmount(e.target.value)}
          autoFocus
        />
        {preview && (
          <div className="cash-event-preview">
            {eventType === "paid_in" ? "+" : "-"} {cur} {preview}
          </div>
        )}

        <label className="field-label">
          {eventType === "paid_out" ? "Reason *" : "Note (optional)"}
        </label>
        <input
          className="field-input"
          type="text"
          placeholder={eventType === "paid_out" ? "Reason for removal…" : "Optional note…"}
          value={note}
          onChange={e => setNote(e.target.value)}
          onKeyDown={e => { if (e.key === "Enter") handleConfirm(); }}
        />

        {error && <div className="modal-error">{error}</div>}

        <div className="modal-actions">
          <button className="modal-btn-secondary" onClick={onCancel} disabled={loading}>
            Cancel
          </button>
          <button
            className={`modal-btn-primary ${eventType === "paid_out" ? "modal-btn-danger" : ""}`}
            onClick={handleConfirm}
            disabled={loading}
          >
            {loading ? "Recording…" : eventType === "paid_in" ? "Record Paid In" : "Record Paid Out"}
          </button>
        </div>
      </div>
    </div>
  );
}
