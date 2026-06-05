import { useState, useRef } from "react";
import type { CartLine } from "../types";
import { formatMoney, parseMoney } from "../money";
import { DEVICE } from "../types";
import { useFocusTrap } from "../hooks/useFocusTrap";

interface Props {
  line: CartLine;
  onApply: (discount_minor: number, reason: string) => void;
  onCancel: () => void;
}

export default function LineDiscountModal({ line, onApply, onCancel }: Props) {
  const modalRef = useRef<HTMLDivElement>(null);
  useFocusTrap(modalRef, onCancel);

  const exp = DEVICE.currency_exponent;
  const cur = DEVICE.currency;
  const fmt = (n: number) => `${cur} ${formatMoney(n, exp)}`;

  const total = Math.round(line.unit_price_minor * (parseFloat(line.quantity) || 1));
  const [value, setValue] = useState("");
  const [reason, setReason] = useState("");

  const discountMinor = parseMoney(value, exp);
  const valid = discountMinor > 0 && discountMinor <= total && reason.trim().length > 0;

  return (
    <div className="modal-overlay" onClick={onCancel}>
      <div ref={modalRef} className="modal" style={{ maxWidth: 380 }} role="dialog" aria-modal="true" aria-label="Line Discount" onClick={e => e.stopPropagation()}>
        <h2 className="modal-title">Discount — {line.product_name}</h2>
        <p style={{ fontSize: "0.82rem", color: "var(--text-dim)", marginBottom: 10 }}>
          Qty: {line.quantity} · Unit: {fmt(line.unit_price_minor)} · Line: {fmt(total)}
        </p>

        <label className="line-edit-label">Discount Amount</label>
        <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 10 }}>
          <span style={{ fontWeight: 700 }}>{cur}</span>
          <input
            className="line-edit-qty-input"
            style={{ flex: 1 }}
            type="number"
            inputMode="decimal"
            min="0"
            step={Math.pow(10, -exp).toFixed(exp)}
            placeholder={`0.${"0".repeat(exp)}`}
            value={value}
            onChange={e => setValue(e.target.value)}
            autoFocus
          />
        </div>

        <label className="line-edit-label">Reason (required)</label>
        <input
          className="line-edit-note-input"
          type="text"
          maxLength={120}
          placeholder="e.g. Damaged, Expiring, Manager override…"
          value={reason}
          onChange={e => setReason(e.target.value)}
          style={{ marginBottom: 12 }}
        />

        <div className="modal-actions line-edit-actions">
          <button className="btn-secondary" onClick={onCancel}>Cancel</button>
          <button className="btn-primary" onClick={() => onApply(discountMinor, reason.trim())} disabled={!valid}>
            Apply −{fmt(discountMinor)}
          </button>
        </div>
      </div>
    </div>
  );
}
