import { useState } from "react";
import type { CartLine } from "../types";
import { formatMoney, parseMoney } from "../money";
import { DEVICE } from "../types";

interface Props {
  line: CartLine;
  onUpdateQty: (id: string, qty: string) => void;
  onApplyLineDiscount: (id: string, discount_minor: number) => void;
  onSetLineNote: (id: string, note: string | null) => void;
  onRemove: (id: string) => void;
  onClose: () => void;
}

type DiscountMode = "pct" | "flat";

export default function LineEditModal({
  line,
  onUpdateQty,
  onApplyLineDiscount,
  onSetLineNote,
  onRemove,
  onClose,
}: Props) {
  const exp = DEVICE.currency_exponent;
  const cur = DEVICE.currency;
  const fmt = (n: number) => `${cur} ${formatMoney(n, exp)}`;

  const [qty, setQty] = useState(line.quantity);
  const [discountMode, setDiscountMode] = useState<DiscountMode>("pct");
  const [discountValue, setDiscountValue] = useState(
    line.line_discount_minor > 0 ? "" : ""
  );
  const [note, setNote] = useState(line.note ?? "");

  const parsedQty = parseFloat(qty);
  const qtyValid = !isNaN(parsedQty) && parsedQty > 0;
  const lineSubtotal = qtyValid
    ? Math.round(line.unit_price_minor * parsedQty)
    : line.unit_price_minor;

  function computeDiscountMinor(): number {
    const num = parseFloat(discountValue);
    if (isNaN(num) || num <= 0) return 0;
    if (discountMode === "pct") {
      return Math.round(lineSubtotal * Math.min(num, 100) / 100);
    } else {
      return Math.min(parseMoney(discountValue, exp), lineSubtotal);
    }
  }

  const discountPreview = computeDiscountMinor();

  function handleApply() {
    if (!qtyValid) return;
    if (qty !== line.quantity) {
      onUpdateQty(line.cart_line_id, qty);
    }
    if (discountPreview !== line.line_discount_minor) {
      onApplyLineDiscount(line.cart_line_id, discountPreview);
    }
    const noteVal = note.trim() || null;
    if (noteVal !== (line.note ?? null)) {
      onSetLineNote(line.cart_line_id, noteVal);
    }
    onClose();
  }

  function handleRemove() {
    onRemove(line.cart_line_id);
    onClose();
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal line-edit-modal" onClick={e => e.stopPropagation()}>
        <h2 className="modal-title">{line.product_name}</h2>
        <p className="line-edit-unit-price">Unit price: {fmt(line.unit_price_minor)}</p>

        {/* ── Quantity ── */}
        <label className="line-edit-label">Quantity</label>
        <div className="line-edit-qty-row">
          <button
            className="qty-btn"
            onClick={() => {
              const q = Math.max(1, parseFloat(qty) - 1);
              setQty(String(q));
            }}
          >−</button>
          <input
            className="line-edit-qty-input"
            type="number"
            min="0.001"
            step="1"
            value={qty}
            onChange={e => setQty(e.target.value)}
          />
          <button
            className="qty-btn"
            onClick={() => setQty(String(parseFloat(qty) + 1))}
          >+</button>
        </div>

        {/* ── Line discount ── */}
        <label className="line-edit-label">Line Discount</label>
        <div className="discount-mode-tabs">
          <button
            className={`discount-mode-tab ${discountMode === "pct" ? "discount-mode-tab-active" : ""}`}
            onClick={() => { setDiscountMode("pct"); setDiscountValue(""); }}
          >
            %
          </button>
          <button
            className={`discount-mode-tab ${discountMode === "flat" ? "discount-mode-tab-active" : ""}`}
            onClick={() => { setDiscountMode("flat"); setDiscountValue(""); }}
          >
            Flat
          </button>
        </div>
        <div className="discount-input-row">
          {discountMode === "pct" ? (
            <>
              <input
                className="discount-input"
                type="number"
                min="0"
                max="100"
                step="1"
                placeholder="0"
                value={discountValue}
                onChange={e => setDiscountValue(e.target.value)}
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
                value={discountValue}
                onChange={e => setDiscountValue(e.target.value)}
              />
            </>
          )}
        </div>
        {discountPreview > 0 && (
          <p className="discount-current-note">
            Discount: − {fmt(discountPreview)}
            &nbsp;→ Line total: {fmt(Math.max(0, lineSubtotal - discountPreview))}
          </p>
        )}

        {/* ── Note ── */}
        <label className="line-edit-label">Note (optional)</label>
        <input
          className="line-edit-note-input"
          type="text"
          maxLength={120}
          placeholder="e.g. No ice"
          value={note}
          onChange={e => setNote(e.target.value)}
        />

        <div className="modal-actions line-edit-actions">
          <button className="btn-danger" onClick={handleRemove}>Remove</button>
          <button className="btn-secondary" onClick={onClose}>Cancel</button>
          <button className="btn-primary" onClick={handleApply} disabled={!qtyValid}>
            Apply
          </button>
        </div>
      </div>
    </div>
  );
}
