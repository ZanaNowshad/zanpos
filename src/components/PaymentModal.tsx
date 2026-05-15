import { useState } from "react";
import type { PaymentInput } from "../types";
import { formatMoney, parseMoney } from "../money";
import { DEVICE } from "../types";

interface Props {
  netTotal: number;
  onConfirm: (payments: PaymentInput[]) => void;
  onCancel: () => void;
  loading?: boolean;
}

export default function PaymentModal({ netTotal, onConfirm, onCancel, loading }: Props) {
  const [method, setMethod] = useState<PaymentInput["method"]>("cash");
  const [tenderedStr, setTenderedStr] = useState("");
  const fmt = (n: number) => `${DEVICE.currency} ${formatMoney(n, DEVICE.currency_exponent)}`;

  const tenderedMinor = parseMoney(tenderedStr, DEVICE.currency_exponent);
  const changeMinor = method === "cash" ? Math.max(0, tenderedMinor - netTotal) : 0;
  const canConfirm = method !== "cash" || tenderedMinor >= netTotal;

  const handleConfirm = () => {
    const payment: PaymentInput = {
      method,
      amount_minor: netTotal,
      ...(method === "cash" && { tendered_minor: tenderedMinor }),
    };
    onConfirm([payment]);
  };

  return (
    <div className="modal-overlay" onClick={e => e.target === e.currentTarget && onCancel()}>
      <div className="modal payment-modal">
        <h2>Payment</h2>
        <div className="payment-total">
          <span>Total due</span>
          <span className="payment-total-amount">{fmt(netTotal)}</span>
        </div>

        <div className="payment-methods">
          {(["cash", "card", "wallet"] as const).map(m => (
            <button
              key={m}
              className={`method-btn ${method === m ? "active" : ""}`}
              onClick={() => setMethod(m)}
            >
              {m.charAt(0).toUpperCase() + m.slice(1)}
            </button>
          ))}
        </div>

        {method === "cash" && (
          <div className="cash-section">
            <label>
              Cash tendered
              <input
                type="number"
                min="0"
                step="0.001"
                placeholder="0.000"
                value={tenderedStr}
                onChange={e => setTenderedStr(e.target.value)}
                autoFocus
              />
            </label>
            {tenderedMinor > 0 && (
              <div className="change-due">
                Change: {fmt(changeMinor)}
              </div>
            )}
          </div>
        )}

        {method !== "cash" && (
          <div className="card-note">
            Record {method} payment of {fmt(netTotal)} — confirm when terminal approves.
          </div>
        )}

        <div className="modal-actions">
          <button className="btn-secondary" onClick={onCancel} disabled={loading}>Cancel</button>
          <button
            className="btn-primary"
            onClick={handleConfirm}
            disabled={!canConfirm || loading}
          >
            {loading ? "Processing…" : "Confirm Payment"}
          </button>
        </div>
      </div>
    </div>
  );
}
