import { useState } from "react";
import type { Cart, CartLine } from "../types";
import { formatMoney } from "../money";
import { DEVICE } from "../types";
import LineEditModal from "./LineEditModal";

interface Props {
  cart: Cart;
  netTotal: number;
  taxTotal: number;
  onUpdateQty: (line_id: string, qty: string) => void;
  onRemove: (line_id: string) => void;
  onApplyLineDiscount: (line_id: string, discount_minor: number) => void;
  onSetLineNote: (line_id: string, note: string | null) => void;
  onPay: () => void;
}

export default function CartPanel({
  cart, netTotal, taxTotal,
  onUpdateQty, onRemove, onApplyLineDiscount, onSetLineNote, onPay,
}: Props) {
  const [editingLine, setEditingLine] = useState<CartLine | null>(null);
  const activeLines = cart.lines.filter(l => !l.voided);
  const fmt = (n: number) => `${DEVICE.currency} ${formatMoney(n, DEVICE.currency_exponent)}`;
  const grossTotal = activeLines.reduce((s, l) => s + l.line_total_minor, 0);
  const hasDiscount = cart.bill_discount_minor > 0 || activeLines.some(l => l.line_discount_minor > 0);
  const totalDiscount = cart.bill_discount_minor + activeLines.reduce((s, l) => s + l.line_discount_minor, 0);

  return (
    <div className="cart-panel">
      {/* Header */}
      <div className="cart-panel-header">
        <span className="cart-panel-title">🛒 Cart</span>
        {activeLines.length > 0 && (
          <span className="cart-item-count">
            {activeLines.length} {activeLines.length === 1 ? "item" : "items"}
          </span>
        )}
      </div>

      {/* Lines */}
      <div className="cart-lines">
        {activeLines.length === 0 && (
          <div className="cart-empty">
            <div className="cart-empty-icon">🛒</div>
            <div className="cart-empty-label">Cart is empty</div>
            <div className="cart-empty-hint">Scan a barcode or tap a product to add it</div>
          </div>
        )}
        {activeLines.map(line => (
          <CartLineRow
            key={line.cart_line_id}
            line={line}
            onEdit={() => setEditingLine(line)}
          />
        ))}
      </div>

      {/* Totals */}
      <div className="cart-totals">
        {taxTotal > 0 && (
          <div className="cart-total-row">
            <span>Tax</span>
            <span>{fmt(taxTotal)}</span>
          </div>
        )}
        {hasDiscount && (
          <>
            <div className="cart-total-row cart-subtotal-row">
              <span>Subtotal</span>
              <span>{fmt(grossTotal)}</span>
            </div>
            <div className="cart-total-row cart-discount-row">
              <span>Discount</span>
              <span>− {fmt(totalDiscount)}</span>
            </div>
          </>
        )}
        <div className="cart-total-row cart-net-total">
          <span>Total</span>
          <span>{fmt(netTotal)}</span>
        </div>
      </div>

      {/* Pay button */}
      <button
        className="pay-button"
        disabled={activeLines.length === 0}
        onClick={onPay}
        title={activeLines.length === 0 ? "Add items to cart to pay" : `Collect ${fmt(netTotal)}`}
      >
        <span>🛒 PAY</span>
        <span>{fmt(netTotal)}</span>
      </button>

      {editingLine && (
        <LineEditModal
          line={editingLine}
          onUpdateQty={onUpdateQty}
          onApplyLineDiscount={onApplyLineDiscount}
          onSetLineNote={onSetLineNote}
          onRemove={onRemove}
          onClose={() => setEditingLine(null)}
        />
      )}
    </div>
  );
}

function CartLineRow({
  line,
  onEdit,
}: {
  line: CartLine;
  onEdit: () => void;
}) {
  const fmt = (n: number) => formatMoney(n, DEVICE.currency_exponent);
  const hasDiscount = line.line_discount_minor > 0;

  return (
    <button className="cart-line-btn" onClick={onEdit} title="Tap to edit quantity, discount, or note">
      <div className="cart-line-main">
        <div className="cart-line-name">{line.product_name}</div>
        {line.note && <span className="cart-line-note">{line.note}</span>}
        <div className="cart-line-meta">
          <span className="cart-line-qty">×{line.quantity}</span>
          <span className="cart-line-unit">@ {fmt(line.unit_price_minor)}</span>
        </div>
      </div>
      <div className="cart-line-right">
        {hasDiscount && (
          <div className="cart-line-discount">−{fmt(line.line_discount_minor)}</div>
        )}
        <div className="cart-line-total">{fmt(line.line_total_minor)}</div>
      </div>
    </button>
  );
}
