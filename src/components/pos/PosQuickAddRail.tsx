import { useRef } from "react";
import { ChevronLeft, ChevronRight, Package } from "lucide-react";
import type { QuickPosSlot } from "../../tauri/commands";
import { DEVICE } from "../../types";
import { formatMoney } from "../../money";
import { productImageSrc } from "../../productImage";

interface Props {
  slots: QuickPosSlot[];
  disabled: boolean;
  onAdd: (productId: string) => void;
}

/**
 * The one-tap row of shop-chosen products, above the cart.
 *
 * It replaced a "last item" strip that repeated what the last-scanned strip
 * under the cart already said. Two readouts of the same fact is worse than one:
 * it costs a row of screen and splits the cashier's attention. The row that
 * earns the space is the one that saves taps — loose produce, bags, bread, the
 * things with no barcode to scan.
 *
 * Which products appear is set in Admin → Catalogue → Quick POS, and the choice
 * rides config sync, so every till in the shop shows the same row.
 *
 * Empty slots are simply not rendered; a gap in the middle of the picker should
 * not become a gap in the middle of the till.
 */
export default function PosQuickAddRail({ slots, disabled, onAdd }: Props) {
  const trackRef = useRef<HTMLDivElement>(null);
  const filled = slots.filter(slot => slot.product_id !== null);

  const scrollBy = (direction: 1 | -1) => {
    const el = trackRef.current;
    if (el) el.scrollBy({ left: direction * Math.round(el.clientWidth * 0.8), behavior: "smooth" });
  };

  if (filled.length === 0) {
    return (
      <div className="till-quick-rail till-quick-rail-empty">
        <span>No quick items yet — set them in Admin → Catalogue → Quick POS</span>
      </div>
    );
  }

  return (
    <div className="till-quick-rail">
      <button
        type="button"
        className="till-quick-arrow"
        onClick={() => scrollBy(-1)}
        aria-label="Scroll quick items left"
        tabIndex={-1}
      >
        <ChevronLeft size={20} className="icon-directional" aria-hidden="true" />
      </button>

      <div className="till-quick-track" ref={trackRef} role="list">
        {filled.map(slot => (
          <button
            key={slot.slot}
            type="button"
            role="listitem"
            className="till-quick-tile"
            disabled={disabled}
            onClick={() => slot.product_id && onAdd(slot.product_id)}
            title={`${slot.name} — ${DEVICE.currency} ${formatMoney(slot.price_minor ?? 0, DEVICE.currency_exponent)}`}
          >
            <span className="till-quick-thumb">
              <Package size={18} aria-hidden="true" />
              {slot.image_path && (
                <img
                  src={productImageSrc(slot.image_path) ?? ""}
                  alt=""
                  onError={event => { event.currentTarget.style.display = "none"; }}
                />
              )}
            </span>
            {/* Name and price are the caption, not the tile: at a glance the
                photo is what identifies a bag of bananas, and the price is what
                the cashier is asked for. */}
            <span className="till-quick-name">{slot.name}</span>
            <span className="till-quick-price">
              {formatMoney(slot.price_minor ?? 0, DEVICE.currency_exponent)}
            </span>
          </button>
        ))}
      </div>

      <button
        type="button"
        className="till-quick-arrow"
        onClick={() => scrollBy(1)}
        aria-label="Scroll quick items right"
        tabIndex={-1}
      >
        <ChevronRight size={20} className="icon-directional" aria-hidden="true" />
      </button>
    </div>
  );
}
