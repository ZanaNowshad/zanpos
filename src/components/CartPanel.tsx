import type { Cart, CartLine } from "../types";
import { formatMoney } from "../money";
import { DEVICE } from "../types";

interface Props {
  cart: Cart;
  netTotal: number;
  taxTotal: number;
  onUpdateQty: (line_id: string, qty: string) => void;
  onRemove: (line_id: string) => void;
  onPay: () => void;
}

export default function CartPanel({ cart, netTotal, taxTotal, onUpdateQty, onRemove, onPay }: Props) {
  const activeLines = cart.lines.filter(l => !l.voided);
  const fmt = (n: number) => `${DEVICE.currency} ${formatMoney(n, DEVICE.currency_exponent)}`;

  return (
    <div className="cart-panel">
      <div className="cart-lines">
        {activeLines.length === 0 && (
          <div className="cart-empty">Cart is empty. Scan or select a product.</div>
        )}
        {activeLines.map(line => (
          <CartLineRow
            key={line.cart_line_id}
            line={line}
            onUpdateQty={onUpdateQty}
            onRemove={onRemove}
          />
        ))}
      </div>

      <div className="cart-totals">
        {taxTotal > 0 && (
          <div className="cart-total-row">
            <span>Tax</span>
            <span>{fmt(taxTotal)}</span>
          </div>
        )}
        <div className="cart-total-row cart-net-total">
          <span>Total</span>
          <span>{fmt(netTotal)}</span>
        </div>
      </div>

      <button
        className="pay-button"
        disabled={activeLines.length === 0}
        onClick={onPay}
      >
        Pay — {fmt(netTotal)}
      </button>
    </div>
  );
}

function CartLineRow({
  line,
  onUpdateQty,
  onRemove,
}: {
  line: CartLine;
  onUpdateQty: (id: string, qty: string) => void;
  onRemove: (id: string) => void;
}) {
  const fmt = (n: number) => formatMoney(n, DEVICE.currency_exponent);

  return (
    <div className="cart-line">
      <div className="cart-line-name">{line.product_name}</div>
      <div className="cart-line-controls">
        <button
          className="qty-btn"
          onClick={() => {
            const q = Math.max(1, parseFloat(line.quantity) - 1);
            onUpdateQty(line.cart_line_id, String(q));
          }}
        >−</button>
        <span className="qty-value">{line.quantity}</span>
        <button
          className="qty-btn"
          onClick={() => {
            const q = parseFloat(line.quantity) + 1;
            onUpdateQty(line.cart_line_id, String(q));
          }}
        >+</button>
      </div>
      <div className="cart-line-price">{fmt(line.unit_price_minor)}</div>
      <div className="cart-line-total">{fmt(line.line_total_minor)}</div>
      <button className="remove-btn" onClick={() => onRemove(line.cart_line_id)}>✕</button>
    </div>
  );
}
