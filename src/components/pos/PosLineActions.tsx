import { Box, Clock, PauseCircle, Tag, TicketPercent } from "lucide-react";
import type { CartLine } from "../../types";
import { DEVICE } from "../../types";
import { formatMoney } from "../../money";

interface Props {
  line: CartLine | null;
  scannedAt: string | null;
  canDiscount: boolean;
  onQty: () => void;
  onPrice: () => void;
  onHold: () => void;
  onDiscount: () => void;
}

/**
 * The four actions the cashier reaches for without thinking, directly under the
 * cart.
 *
 * The till used to carry a permanent eight-button rail mixing per-line, per-sale
 * and per-shift actions at equal weight; the rest moved behind `More`. Three of
 * these four act on the selected line and are meaningless without one, so they
 * disable rather than sit there inviting a tap that cannot do anything.
 *
 * Hold is the exception, and it is here on purpose. It acts on the whole sale
 * rather than a line, but it is reached several times a shift — a customer goes
 * back for something they forgot and the queue behind them cannot wait — and
 * behind `More` that cost two taps every time. Void went the other way: it is
 * the expensive mistake at a till, and it is used far less often than holding a
 * bill, so it is worth the extra tap.
 *
 * The strip above names the line being acted on. That is the guard against
 * acting on the wrong item, rather than a confirmation dialog that would cost
 * seconds on every sale.
 */
export default function PosLineActions({
  line, scannedAt, canDiscount, onQty, onPrice, onHold, onDiscount,
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

      <div className="till-line-actions" role="group" aria-label="Sale actions">
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
        {/* Not disabled with the others: holding parks the whole sale, and
            resuming one is reached with an empty cart and nothing selected. */}
        <button type="button" className="till-line-action" onClick={onHold}>
          <PauseCircle size={18} aria-hidden="true" />
          <span>Hold</span>
          <span className="till-key">F1</span>
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
