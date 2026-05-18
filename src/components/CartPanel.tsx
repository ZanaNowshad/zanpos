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
  // ── Quick Checkout additions ──────────────────────────────────────────────────
  recentLineId: string | null;
  onIncrementRecent: () => void;
  onDecrementRecent: () => void;
  onPayDirect: (method: "cash" | "card" | "wallet") => void;
  onPaySplit: () => void;
  onPayFast: () => void;
  payFastLoading?: boolean;
}

export default function CartPanel({
  cart, netTotal, taxTotal,
  onUpdateQty, onRemove, onApplyLineDiscount, onSetLineNote, onPay,
  recentLineId, onIncrementRecent, onDecrementRecent,
  onPayDirect, onPaySplit, onPayFast, payFastLoading,
}: Props) {
  const [editingLine, setEditingLine] = useState<CartLine | null>(null);
  const activeLines = cart.lines.filter(l => !l.voided);
  const fmt = (n: number) => `${DEVICE.currency} ${formatMoney(n, DEVICE.currency_exponent)}`;
  const grossTotal = activeLines.reduce((s, l) => s + l.line_total_minor, 0);
  const hasDiscount = cart.bill_discount_minor > 0 || activeLines.some(l => l.line_discount_minor > 0);
  const totalDiscount = cart.bill_discount_minor + activeLines.reduce((s, l) => s + l.line_discount_minor, 0);
  const canPay = activeLines.length > 0;

  return (
    <div className="cart-panel">
      {/* Header */}
      <div className="cart-panel-header">
        <span className="cart-panel-title">CART ({activeLines.length})</span>
        {recentLineId && (
          <span className="cart-recent-hint" title="Use +/− or Backspace to adjust recent item">
            +/− recent
          </span>
        )}
      </div>

      {/* Lines */}
      <div className="cart-lines">
        {activeLines.length === 0 && (
          <div className="cart-empty">
            <div className="cart-empty-icon">🛒</div>
            <div className="cart-empty-label">Your cart is empty</div>
            <div className="cart-empty-hint">Scan a barcode or tap a product to add it</div>
          </div>
        )}
        {activeLines.map(line => (
          <CartLineRow
            key={line.cart_line_id}
            line={line}
            isRecent={line.cart_line_id === recentLineId}
            onEdit={() => setEditingLine(line)}
            onIncrement={onIncrementRecent}
            onDecrement={onDecrementRecent}
            onRemove={() => onRemove(line.cart_line_id)}
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

      {/* ── Payment zone ─────────────────────────────────────────────────── */}
      <div className="pay-zone">
        {/* Method buttons — one click to payment modal pre-set to method */}
        <div className="pay-method-row">
          <button
            className="pay-method-btn"
            disabled={!canPay}
            onClick={() => onPayDirect("cash")}
            title="Cash payment"
          >
            💵 Cash
          </button>
          <button
            className="pay-method-btn"
            disabled={!canPay}
            onClick={() => onPayDirect("card")}
            title="Card / terminal payment"
          >
            💳 Card
          </button>
          <button
            className="pay-method-btn"
            disabled={!canPay}
            onClick={() => onPayDirect("wallet")}
            title="Wallet / mobile payment"
          >
            📱 Wallet
          </button>
          <button
            className="pay-method-btn"
            disabled={!canPay}
            onClick={onPaySplit}
            title="Split payment across methods"
          >
            ⊕ Split
          </button>
        </div>

        {/* Pay Fast — one keystroke checkout */}
        <button
          className="pay-fast-btn"
          disabled={!canPay || payFastLoading}
          onClick={onPayFast}
          title={!canPay ? "Add items to cart first" : "Cash exact — no receipt — instant checkout (F12)"}
        >
          {payFastLoading ? "Processing…" : <><span>⚡ Pay Fast</span><kbd>F12</kbd></>}
        </button>

        {/* Standard pay — opens full payment modal */}
        <button
          className="pay-button"
          disabled={!canPay}
          onClick={onPay}
          title={!canPay ? "Add items to cart to pay" : `Collect ${fmt(netTotal)} · F9`}
        >
          <span>🛒 PAY</span>
          <span>{fmt(netTotal)}</span>
        </button>
      </div>

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
  isRecent,
  onEdit,
  onIncrement,
  onDecrement,
  onRemove,
}: {
  line: CartLine;
  isRecent: boolean;
  onEdit: () => void;
  onIncrement: () => void;
  onDecrement: () => void;
  onRemove: () => void;
}) {
  const fmt = (n: number) => formatMoney(n, DEVICE.currency_exponent);
  const hasDiscount = line.line_discount_minor > 0;

  return (
    <div className={`cart-line-wrap ${isRecent ? "cart-line-recent" : ""}`}>
      <button
        className="cart-line-btn"
        onClick={onEdit}
        title="Tap to edit quantity, discount, or note"
      >
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

      {/* Quick controls visible on the recent item */}
      {isRecent && (
        <div className="cart-line-quick">
          <button
            className="cart-quick-btn"
            onClick={e => { e.stopPropagation(); onDecrement(); }}
            title="Decrease qty (−)"
          >−</button>
          <button
            className="cart-quick-btn"
            onClick={e => { e.stopPropagation(); onIncrement(); }}
            title="Increase qty (+)"
          >+</button>
          <button
            className="cart-quick-btn cart-quick-del"
            onClick={e => { e.stopPropagation(); onRemove(); }}
            title="Remove item (Delete)"
          >🗑</button>
        </div>
      )}
    </div>
  );
}
