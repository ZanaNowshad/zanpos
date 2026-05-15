import { useState } from "react";
import { formatMoney, parseMoney } from "../money";
import { DEVICE } from "../types";

interface Props {
  /** Gross cart total in minor units — used to cap and preview percent discounts */
  grossMinor: number;
  /** Current bill discount already applied */
  currentDiscountMinor: number;
  onApply: (discount_minor: number) => void;
  onCancel: () => void;
}

type Mode = "pct" | "flat";

export default function DiscountModal({ grossMinor, currentDiscountMinor, onApply, onCancel }: Props) {
  const [mode, setMode] = useState<Mode>("pct");
  const [value, setValue] = useState("");
  const exp = DEVICE.currency_exponent;
  const cur = DEVICE.currency;

  function computeMinor(): number {
    const num = parseFloat(value);
    if (isNaN(num) || num <= 0) return 0;
    if (mode === "pct") {
      const pct = Math.min(num, 100);
      return Math.round(grossMinor * pct / 100);
    } else {
      return Math.min(parseMoney(value, exp), grossMinor);
    }
  }

  const preview = computeMinor();
  const netAfter = Math.max(0, grossMinor - preview);
  const isValid = preview > 0;

  function handleApply() {
    if (isValid) onApply(preview);
  }

  return (
    <div className="modal-overlay">
      <div className="modal discount-modal">
        <h2 className="modal-title">Apply Discount</h2>

        <div className="discount-mode-tabs">
          <button
            className={`discount-mode-tab ${mode === "pct" ? "discount-mode-tab-active" : ""}`}
            onClick={() => { setMode("pct"); setValue(""); }}
          >
            % Percent
          </button>
          <button
            className={`discount-mode-tab ${mode === "flat" ? "discount-mode-tab-active" : ""}`}
            onClick={() => { setMode("flat"); setValue(""); }}
          >
            Flat Amount
          </button>
        </div>

        <div className="discount-input-row">
          {mode === "pct" ? (
            <>
              <input
                className="discount-input"
                type="number"
                min="0"
                max="100"
                step="1"
                placeholder="e.g. 10"
                value={value}
                onChange={e => setValue(e.target.value)}
                autoFocus
              />
              <span className="discount-input-suffix">%</span>
            </>
          ) : (
            <>
              <span className="discount-input-prefix">{cur}</span>
              <input
                className="discount-input"
                type="number"
                min="0"
                step={Math.pow(10, -exp).toFixed(exp)}
                placeholder={`0.${"0".repeat(exp)}`}
                value={value}
                onChange={e => setValue(e.target.value)}
                autoFocus
              />
            </>
          )}
        </div>

        {isValid && (
          <div className="discount-preview">
            <div className="discount-preview-row">
              <span>Discount</span>
              <span>− {cur} {formatMoney(preview, exp)}</span>
            </div>
            <div className="discount-preview-row discount-preview-net">
              <span>New Total</span>
              <span>{cur} {formatMoney(netAfter, exp)}</span>
            </div>
          </div>
        )}

        {currentDiscountMinor > 0 && (
          <p className="discount-current-note">
            Current bill discount: {cur} {formatMoney(currentDiscountMinor, exp)} — applying will replace it.
          </p>
        )}

        <div className="modal-actions">
          <button className="btn-secondary" onClick={onCancel}>Cancel</button>
          <button
            className="btn-primary"
            onClick={handleApply}
            disabled={!isValid}
          >
            Apply
          </button>
        </div>
      </div>
    </div>
  );
}
