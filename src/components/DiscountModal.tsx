import { useRef, useState } from "react";
import { formatMoney, parseMoney } from "../money";
import { DEVICE } from "../types";
import Dialpad, { applyDialpadKey } from "./Dialpad";

interface Props {
  grossMinor:           number;
  currentDiscountMinor: number;
  onApply:  (discount_minor: number, reason: string) => void;
  onCancel: () => void;
}

type Mode = "pct" | "flat";

export default function DiscountModal({ grossMinor, currentDiscountMinor, onApply, onCancel }: Props) {
  const [mode, setMode]     = useState<Mode>("pct");
  const [value, setValue]   = useState("");
  const [reason, setReason] = useState("");
  const confirmRef = useRef<HTMLButtonElement>(null);
  const exp = DEVICE.currency_exponent;
  const cur = DEVICE.currency;

  // ── Computed ──
  function computeMinor(): number {
    if (mode === "pct") {
      const bp = Math.round(parseFloat(value || "0") * 100);
      if (isNaN(bp) || bp <= 0) return 0;
      return Math.round(grossMinor * Math.min(bp, 10000) / 10000);
    }
    return Math.min(parseMoney(value, exp), grossMinor);
  }

  const preview    = computeMinor();
  const netAfter   = Math.max(0, grossMinor - preview);
  const pctDisplay = mode === "pct" && value ? `${value}%` : null;
  const isValid    = preview > 0 && reason.trim().length > 0;

  // ── Dialpad ──
  const handleDialpadKey = (key: string) => {
    // For percent mode: block decimal, cap at "100"
    if (mode === "pct") {
      if (key === "." || key === "00") return;
      const next = applyDialpadKey(value, key);
      const num = parseInt(next, 10);
      if (!isNaN(num) && num > 100) return;
      setValue(next);
    } else {
      setValue(prev => applyDialpadKey(prev, key));
    }
  };

  const handleApply = () => {
    if (isValid) onApply(preview, reason.trim());
  };

  return (
    <div className="modal-overlay" onClick={e => e.target === e.currentTarget && onCancel()}>
      <div className="cash-event-shell" role="dialog" aria-modal="true" aria-label="Apply Discount">

        {/* ── Left: form ── */}
        <div className="modal cash-event-left">
          <h2 className="modal-title">Apply Discount</h2>

          {/* Mode tabs */}
          <div className="ce-type-tabs">
            <button
              className={`ce-type-tab${mode === "pct" ? " ce-type-tab-active" : ""}`}
              onClick={() => { setMode("pct"); setValue(""); }}
            >
              <span className="ce-tab-icon">%</span>
              <span className="ce-tab-label">Percent</span>
            </button>
            <button
              className={`ce-type-tab${mode === "flat" ? " ce-type-tab-active" : ""}`}
              onClick={() => { setMode("flat"); setValue(""); }}
            >
              <span className="ce-tab-icon">{cur}</span>
              <span className="ce-tab-label">Flat Amount</span>
            </button>
          </div>

          {/* Bill total reference */}
          <div className="discount-bill-ref">
            <span className="discount-bill-label">Bill total</span>
            <span className="discount-bill-value">{cur} {formatMoney(grossMinor, exp)}</span>
          </div>

          {/* Amount display */}
          <div className="ce-amount-block">
            {mode === "pct" ? (
              <>
                <span className={`ce-amount-value${!value ? " ce-amount-placeholder" : ""}`}
                  style={value ? { color: "var(--accent)" } : {}}>
                  {value || "0"}
                </span>
                <span className="ce-amount-cur">%</span>
              </>
            ) : (
              <>
                <span className="ce-amount-cur">{cur}</span>
                <span className={`ce-amount-value${!value ? " ce-amount-placeholder" : ""}`}
                  style={value ? { color: "var(--accent)" } : {}}>
                  {value || `0.${"0".repeat(exp)}`}
                </span>
              </>
            )}
            <span className="touch-cursor">|</span>
          </div>

          {/* Live preview */}
          {preview > 0 && (
            <div className="discount-preview-block">
              <div className="discount-preview-row">
                <span>Discount</span>
                <span className="discount-preview-off">− {cur} {formatMoney(preview, exp)}</span>
              </div>
              <div className="discount-preview-row discount-preview-net">
                <span>New Total</span>
                <span className="discount-preview-new">{cur} {formatMoney(netAfter, exp)}</span>
              </div>
            </div>
          )}

          {currentDiscountMinor > 0 && (
            <p className="discount-current-note">
              Current discount: {cur} {formatMoney(currentDiscountMinor, exp)} — will be replaced.
            </p>
          )}

          {/* Reason */}
          <label className="ce-note-label" htmlFor="discount-reason">Reason *</label>
          <input
            id="discount-reason"
            className="ce-note-input"
            type="text"
            maxLength={120}
            placeholder="e.g. Manager approved, loyalty reward…"
            value={reason}
            onChange={e => setReason(e.target.value)}
            onKeyDown={e => e.key === "Enter" && handleApply()}
          />
        </div>

        {/* ── Right: dialpad ── */}
        <div className="payment-dialpad-panel">
          <div className="dialpad-field-indicator">
            {mode === "pct" ? "Enter Percent" : `Enter ${cur} Amount`}
          </div>

          <Dialpad onKey={handleDialpadKey} />

          <div className="dialpad-actions">
            <button
              ref={confirmRef}
              className="dialpad-confirm-btn"
              onClick={handleApply}
              disabled={!isValid}
            >
              {isValid ? (
                <>
                  <span className="dialpad-confirm-icon">✓</span>
                  <span className="dialpad-confirm-label">Apply Discount</span>
                  {pctDisplay
                    ? <span className="dialpad-confirm-total">{pctDisplay}</span>
                    : preview > 0
                    ? <span className="dialpad-confirm-total">− {cur} {formatMoney(preview, exp)}</span>
                    : null}
                </>
              ) : (
                <span className="dialpad-confirm-label">Apply Discount</span>
              )}
            </button>
            <button className="dialpad-cancel-btn" onClick={onCancel}>
              ✕ Cancel
            </button>
          </div>
        </div>

      </div>
    </div>
  );
}
