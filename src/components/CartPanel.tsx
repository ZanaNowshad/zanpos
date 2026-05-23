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
  onApplyLineDiscount: (line_id: string, discount_minor: number, reason: string) => void;
  onSetLineNote: (line_id: string, note: string | null) => void;
  onPaySplit: () => void;
  onPayFast: () => void;
  onPayDirect: (method: "cash" | "card" | "wallet") => void;
  payFastLoading?: boolean;
  paymentStarted?: boolean;
  recentLineId: string | null;
  onBumpLine: (cart_line_id: string, delta: number) => void;
}

export default function CartPanel({
  cart, netTotal, taxTotal,
  onUpdateQty, onRemove, onApplyLineDiscount, onSetLineNote,
  onPaySplit, onPayFast, onPayDirect, payFastLoading,
  paymentStarted, recentLineId, onBumpLine,
}: Props) {
  const [editingLine, setEditingLine] = useState<CartLine | null>(null);
  const activeLines = cart.lines.filter(l => !l.voided);
  const fmt = (n: number) => `${DEVICE.currency} ${formatMoney(n, DEVICE.currency_exponent)}`;
  const grossTotal = activeLines.reduce((s, l) => s + l.line_total_minor, 0);
  const totalDiscount = cart.bill_discount_minor + activeLines.reduce((s, l) => s + l.line_discount_minor, 0);
  const isPaymentLocked = Boolean(payFastLoading || paymentStarted);
  const canPay = activeLines.length > 0 && netTotal > 0 && !isPaymentLocked;


  return (
    <div className={`cart-panel ${activeLines.length > 0 ? "cart-panel-active" : "cart-panel-idle"} ${isPaymentLocked ? "cart-panel-locked" : ""}`}>
      {/* Header */}
      <div className="cart-panel-header">
        <div>
          <span className="cart-panel-title">Cart</span>
          <span className="cart-item-count">{activeLines.length} item{activeLines.length === 1 ? "" : "s"}</span>
        </div>
        {recentLineId && (
          <span className="cart-recent-hint" title="Use +/− or Backspace to adjust recent item">
            +/− recent
          </span>
        )}
      </div>

      {/* Lines */}
      <div className="cart-lines">
        {isPaymentLocked && (
          <div className="cart-processing-banner">
            Processing payment. Do not edit this sale.
          </div>
        )}
        {activeLines.length === 0 && (
          <div className="cart-empty">
            <div className="cart-empty-icon">🛒</div>
            <div className="cart-empty-label">Cart is empty</div>
            <div className="cart-empty-hint">Scan an item, tap a product, or press <kbd>F2</kbd>.</div>
          </div>
        )}
        {activeLines.map(line => (
          <CartLineRow
            key={line.cart_line_id}
            line={line}
            isRecent={line.cart_line_id === recentLineId}
            disabled={isPaymentLocked}
            onEdit={() => setEditingLine(line)}
            onIncrement={() => onBumpLine(line.cart_line_id, 1)}
            onDecrement={() => onBumpLine(line.cart_line_id, -1)}
            onRemove={() => onRemove(line.cart_line_id)}
          />
        ))}
      </div>

      {/* Totals */}
      <div className="cart-totals">
        <div className="cart-total-row cart-subtotal-row">
          <span>Subtotal</span>
          <span className="num">{fmt(grossTotal)}</span>
        </div>
        {totalDiscount > 0 && (
          <div className="cart-total-row cart-discount-row cart-discount-active">
            <span>Discount</span>
            <span className="num">−{fmt(totalDiscount)}</span>
          </div>
        )}
        {totalDiscount <= 0 && (
          <div className="cart-total-row cart-discount-row">
            <span>Discount</span>
            <span className="num cart-total-neutral">{fmt(0)}</span>
          </div>
        )}
        <div className="cart-total-row">
          <span>Tax</span>
          <span className="num">{fmt(taxTotal)}</span>
        </div>
        <div className="cart-total-row cart-net-total">
          <span>TOTAL</span>
          <span className="num cart-grand-total">{fmt(netTotal)}</span>
        </div>
      </div>

      <div className="cart-method-row">
        <button
          className="cart-method-btn"
          disabled={!canPay}
          onClick={() => onPayDirect("cash")}
          title="Cash payment"
        >Cash</button>
        <button
          className="cart-method-btn"
          disabled={!canPay}
          onClick={() => onPayDirect("card")}
          title="Card payment"
        >Card</button>
        <button
          className="cart-method-btn"
          disabled={!canPay}
          onClick={() => onPayDirect("wallet")}
          title="Wallet / mobile payment"
        >Wallet</button>
      </div>

      <div className="cart-pay-row">
        <button
          className="cart-pay-fast-btn"
          disabled={!canPay || payFastLoading}
          onClick={onPayFast}
          title={!canPay ? "Add items to pay" : "Fast Cash — exact amount, no receipt · F12"}
        >
          {payFastLoading ? "…" : <><span>Fast Cash <kbd>F12</kbd></span><span className="cart-pay-total">{fmt(netTotal)}</span></>}
        </button>
        <button
          className="cart-pay-split-btn"
          disabled={!canPay}
          onClick={onPaySplit}
          title="Split across multiple payment methods"
        >
          Split
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
  disabled,
  onEdit,
  onIncrement,
  onDecrement,
  onRemove,
}: {
  line: CartLine;
  isRecent: boolean;
  disabled: boolean;
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
        className="cart-line-name-btn"
        onClick={onEdit}
        disabled={disabled}
        title="Tap to edit quantity, discount, or note"
      >
        <span className="cart-line-name">{line.product_name}</span>
        {line.note && <span className="cart-line-note">{line.note}</span>}
        <span className="cart-line-sku">{line.sku || line.barcode || "Custom item"}</span>
        <span className="cart-line-unit">{DEVICE.currency} {fmt(line.unit_price_minor)}</span>
      </button>

      <div className="cart-line-controls">
        <button
          className="cart-qty-btn"
          disabled={disabled}
          aria-label={`Decrease quantity of ${line.product_name}`}
          onClick={e => { e.stopPropagation(); onDecrement(); }}
          title="Decrease qty"
        >−</button>
        <span className="cart-qty-val">{line.quantity}</span>
        <button
          className="cart-qty-btn"
          disabled={disabled}
          aria-label={`Increase quantity of ${line.product_name}`}
          onClick={e => { e.stopPropagation(); onIncrement(); }}
          title="Increase qty"
        >+</button>

        <div className="cart-line-total-col">
          {hasDiscount && (
            <span className="cart-line-discount">−{DEVICE.currency} {fmt(line.line_discount_minor)}</span>
          )}
          <span className="cart-line-total">{DEVICE.currency} {fmt(line.line_total_minor)}</span>
        </div>

        <button
          className="cart-line-del-btn"
          disabled={disabled}
          aria-label={`Remove ${line.product_name} from cart`}
          onClick={e => { e.stopPropagation(); onRemove(); }}
          title={`Remove ${line.product_name}`}
        >×</button>
      </div>
    </div>
  );
}
