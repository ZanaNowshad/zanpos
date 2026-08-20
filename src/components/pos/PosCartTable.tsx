import { useEffect, useRef } from "react";
import { Minus, Package, Plus } from "lucide-react";
import type { Cart, CartLine } from "../../types";
import { DEVICE } from "../../types";
import { formatMoney } from "../../money";
import { productImageSrc } from "../../productImage";

interface Props {
  cart: Cart;
  selectedLineId: string | null;
  disabled: boolean;
  onSelectLine: (lineId: string) => void;
  onBumpQty: (lineId: string, delta: number) => void;
  onEditPrice: (line: CartLine) => void;
}

/**
 * The basket, and the largest thing on the till by design.
 *
 * Newest line sits at the bottom so the list reads in the order the receipt
 * prints and the customer can follow along, and the view auto-scrolls to it.
 * The pinned "last scanned" strip below the table keeps the just-added item
 * visible when the cashier has scrolled up — that pair removes the usual fight
 * between auto-scroll and manual scroll.
 *
 * Selecting a line reveals its two corrections in place: minus/plus on the
 * quantity, and a tappable unit price. Both are the mistakes that actually
 * happen at a till — two of something instead of one, or a shelf price that
 * disagrees — and putting them on the line means the cashier fixes what they
 * are looking at rather than aiming at a strip somewhere else. The controls
 * appear only on the selected row, so an unselected basket stays a clean list.
 *
 * The row is a grid of cells rather than one big button because those controls
 * are buttons themselves, and a button cannot contain a button.
 *
 * Row numbers exist so a line can be named out loud: "void line seven".
 */
export default function PosCartTable({
  cart, selectedLineId, disabled, onSelectLine, onBumpQty, onEditPrice,
}: Props) {
  const lines = cart.lines.filter(line => !line.voided);
  const bodyRef = useRef<HTMLDivElement>(null);
  const lineCount = lines.length;

  useEffect(() => {
    const el = bodyRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [lineCount]);

  const money = (minor: number) => formatMoney(minor, DEVICE.currency_exponent);

  if (lineCount === 0) {
    return (
      <div className="till-cart till-cart-empty">
        <Package size={34} aria-hidden="true" />
        <p>Scan an item to start the sale</p>
        <small>The barcode field is always ready — press F2 if focus is lost</small>
      </div>
    );
  }

  return (
    <div className="till-cart">
      <div className="till-cart-head" aria-hidden="true">
        <span />
        <span className="till-c-qty">Qty</span>
        <span className="till-c-unit">Unit price</span>
        <span className="till-c-total">Line total</span>
      </div>

      <div className="till-cart-body" ref={bodyRef} role="list">
        {lines.map((line, index) => {
          const selected = line.cart_line_id === selectedLineId;
          const image = line.image_path ? productImageSrc(line.image_path) : null;
          return (
            <div
              key={line.cart_line_id}
              role="listitem"
              className={`till-row${selected ? " is-selected" : ""}`}
            >
              <div className="till-row-grid">
                <button
                  type="button"
                  className="till-row-select"
                  onClick={() => onSelectLine(line.cart_line_id)}
                  aria-current={selected ? "true" : undefined}
                  aria-label={`Line ${index + 1}: ${line.product_name}, quantity ${line.quantity}`}
                >
                  <span className="till-c-num">{index + 1}</span>
                  <span className="till-c-thumb">
                    <Package size={16} aria-hidden="true" />
                    {image && (
                      <img
                        src={image}
                        alt=""
                        onError={event => { event.currentTarget.style.display = "none"; }}
                      />
                    )}
                  </span>
                  <span className="till-c-item">
                    <span className="till-item-name">{line.product_name}</span>
                    {line.sku && <span className="till-item-sub">{line.sku}</span>}
                  </span>
                </button>

                <span className="till-c-qty">
                  {selected ? (
                    <span className="till-qty-step">
                      <button
                        type="button"
                        disabled={disabled}
                        onClick={() => onBumpQty(line.cart_line_id, -1)}
                        aria-label={`One fewer ${line.product_name}`}
                      >
                        <Minus size={15} aria-hidden="true" />
                      </button>
                      <span className="till-qty-value">{line.quantity}</span>
                      <button
                        type="button"
                        disabled={disabled}
                        onClick={() => onBumpQty(line.cart_line_id, 1)}
                        aria-label={`One more ${line.product_name}`}
                      >
                        <Plus size={15} aria-hidden="true" />
                      </button>
                    </span>
                  ) : line.quantity}
                </span>

                <span className="till-c-unit">
                  {selected ? (
                    <button
                      type="button"
                      className="till-price-btn"
                      disabled={disabled}
                      onClick={() => onEditPrice(line)}
                      aria-label={`Change the price of ${line.product_name}`}
                    >
                      {money(line.unit_price_minor)}
                    </button>
                  ) : money(line.unit_price_minor)}
                </span>

                <span className="till-c-total">
                  {money(line.line_total_minor)}
                  {line.line_discount_minor > 0 && (
                    <small className="till-line-disc">−{money(line.line_discount_minor)}</small>
                  )}
                </span>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
