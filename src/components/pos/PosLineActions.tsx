import { Box, Clock, Tag, Trash2, TicketPercent } from "lucide-react";
import type { CartLine } from "../../types";
import { DEVICE } from "../../types";
import { formatMoney } from "../../money";

interface Props {
  line: CartLine | null;
  scannedAt: string | null;
  canDiscount: boolean;
  onQty: () => void;
  onPrice: () => void;
  onVoid: () => void;
  onDiscount: () => void;
}

/**
 * Actions for the line the cashier has selected, directly under the cart.
 *
 * The till used to carry a permanent eight-button rail mixing per-line, per-sale
 * and per-shift actions at equal weight. These four are the per-line ones, and
 * they are meaningless with nothing selected — so they disable rather than sit
 * there inviting a tap that cannot do anything. The rest moved behind `More`.
 *
 * The strip above them names the line being acted on. Voiding the wrong item is
 * the expensive mistake at a till, and the guard against it is showing what is
 * about to be voided, not a confirmation dialog that costs seconds per sale.
 */
export default function PosLineActions({
  line, scannedAt, canDiscount, onQty, onPrice, onVoid, onDiscount,
}: Props) {
  const disabled = line === null;
  const money = (minor: number) =>
    `${DEVICE.currency} ${formatMoney(minor, DEVICE.currency_exponent)}`;

  return (
    <div className="till-line-actions-wrap">
      <div className="till-last-scanned" aria-live="polite">
        <Clock size={16} aria-hidden="true" />
        <span className="till-last-scanned-label">Last scanned</span>
        {line ? (
          <>
            <span className="till-last-scanned-name">{line.product_name}</span>
            <span className="till-last-scanned-meta">
              <span>Qty {line.quantity}</span>
              <span aria-hidden="true">·</span>
              <span>{money(line.unit_price_minor)}</span>
              {scannedAt && (<><span aria-hidden="true">·</span><span>{scannedAt}</span></>)}
            </span>
          </>
        ) : (
          <span className="till-last-scanned-empty">Nothing selected — scan an item</span>
        )}
      </div>

      <div className="till-line-actions" role="group" aria-label="Selected item actions">
        <button type="button" className="till-line-action" disabled={disabled} onClick={onQty}>
          <Box size={18} aria-hidden="true" />
          <span>Qty</span>
          <span className="till-key">F3</span>
        </button>
        <button type="button" className="till-line-action" disabled={disabled} onClick={onPrice}>
          <Tag size={18} aria-hidden="true" />
          <span>Price</span>
          <span className="till-key">F4</span>
        </button>
        <button
          type="button"
          className="till-line-action till-line-action-danger"
          disabled={disabled}
          onClick={onVoid}
        >
          <Trash2 size={18} aria-hidden="true" />
          <span>Void</span>
          <span className="till-key">Del</span>
        </button>
        <button
          type="button"
          className="till-line-action"
          disabled={disabled || !canDiscount}
          onClick={onDiscount}
          title={canDiscount ? undefined : "Discounts need a manager"}
        >
          <TicketPercent size={18} aria-hidden="true" />
          <span>Discount</span>
          <span className="till-key">F5</span>
        </button>
      </div>
    </div>
  );
}
