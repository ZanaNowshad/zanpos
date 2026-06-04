import { useEffect, useRef, useState } from "react";
import { DEVICE } from "../types";
import { formatMoney, parseMoney } from "../money";
import Dialpad, { applyDialpadKey } from "./Dialpad";

interface Props {
  productName: string;
  onConfirm: (priceMajor: string) => Promise<void>;
  onCancel: () => void;
}

export default function PriceInputModal({ productName, onConfirm, onCancel }: Props) {
  const [price, setPrice]     = useState("");
  const [error, setError]     = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const confirmRef = useRef<HTMLButtonElement>(null);
  const priceRef = useRef<HTMLInputElement>(null);

  const EXP = DEVICE.currency_exponent;
  const cur = DEVICE.currency;

  // Focus the price input on open so the cashier can type immediately.
  useEffect(() => {
    const t = setTimeout(() => { priceRef.current?.focus(); priceRef.current?.select(); }, 50);
    return () => clearTimeout(t);
  }, []);

  const priceMinor = parseMoney(price, EXP);
  const canConfirm = priceMinor > 0;

  const handleDialpadKey = (key: string) => {
    setError(null);
    setPrice(prev => applyDialpadKey(prev, key));
  };

  const handleConfirm = async () => {
    if (!canConfirm) { setError("Enter a valid price"); return; }
    setLoading(true);
    try {
      await onConfirm(price);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to set price");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="modal-overlay" onClick={e => e.target === e.currentTarget && !loading && onCancel()}>
      <div className="custom-item-shell price-input-shell">

        {/* ── Left: info + price display ── */}
        <div className="modal custom-item-left">
          <h2 className="modal-title">Set Price</h2>

          <div className="pi-product-name">{productName}</div>
          <div className="pi-hint">This item has no base price. Enter the selling price:</div>

          {/* Keyboard-typeable price input (dialpad on the right still works for touch) */}
          <div className="ce-amount-block ce-amount-input-block">
            <span className="ce-amount-cur">{cur}</span>
            <input
              ref={priceRef}
              className="ce-amount-input"
              type="text"
              inputMode="decimal"
              value={price}
              placeholder={`0.${"0".repeat(EXP)}`}
              onChange={e => {
                const v = e.target.value.replace(/[^0-9.]/g, "").replace(/(\..*)\./g, "$1");
                setPrice(v); setError(null);
              }}
              onKeyDown={e => {
                if (e.key === "Enter") { e.preventDefault(); if (canConfirm) handleConfirm(); }
                else if (e.key === "Escape") { e.preventDefault(); onCancel(); }
              }}
            />
          </div>

          {/* Line total preview */}
          {priceMinor > 0 && (
            <div className="ci-line-total">
              <span className="ci-line-total-label">Price</span>
              <span className="ci-line-total-value">{cur} {formatMoney(priceMinor, EXP)}</span>
            </div>
          )}

          {error && <div className="modal-error">{error}</div>}
        </div>

        {/* ── Right: dialpad ── */}
        <div className="payment-dialpad-panel">
          <div className="dialpad-field-indicator">Enter {cur} Price</div>

          <Dialpad onKey={handleDialpadKey} />

          <div className="dialpad-actions">
            <button
              ref={confirmRef}
              className="dialpad-confirm-btn"
              onClick={handleConfirm}
              disabled={!canConfirm || loading}
            >
              {loading ? (
                <span className="dialpad-confirm-label">Saving…</span>
              ) : (
                <>
                  <span className="dialpad-confirm-icon">✓</span>
                  <span className="dialpad-confirm-label">Confirm Price</span>
                  {priceMinor > 0 && (
                    <span className="dialpad-confirm-total">{cur} {formatMoney(priceMinor, EXP)}</span>
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
