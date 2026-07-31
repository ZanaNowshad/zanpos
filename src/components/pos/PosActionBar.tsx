import { Clock, LockKeyhole, Printer, Truck } from "lucide-react";

interface Props {
  lineCount: number;
  canDiscount: boolean;
  canRefund: boolean;
  lastReceiptNumber: string | null;
  onClearCart: () => void;
  onOpenHold: () => void;
  onOpenDiscount: () => void;
  onOpenRefund: () => void;
  onOpenCashEvent: () => void;
  onOpenDeliveries: () => void;
  onOpenRecent: () => void;
  onReprintLast: () => void;
}

/**
 * The till's action bar. Purely presentational — every action arrives as a
 * prop, and the only logic is which buttons a role may see and when a button
 * is disabled.
 *
 * Keyboard hints are rendered inline because the shortcuts are the fast path
 * for an experienced cashier; the mouse targets exist for everyone else.
 */
export default function PosActionBar({
  lineCount, canDiscount, canRefund, lastReceiptNumber,
  onClearCart, onOpenHold, onOpenDiscount, onOpenRefund,
  onOpenCashEvent, onOpenDeliveries, onOpenRecent, onReprintLast,
}: Props) {
  return (
    <div className="action-bar">
      <div className="action-group action-group-transaction">
        <button
          className="action-btn action-btn-danger"
          onClick={onClearCart}
          disabled={lineCount === 0}
          title="Clear cart — Ctrl+Delete"
        >
          Clear <kbd>Ctrl+⌫</kbd>
        </button>
        <button
          className="action-btn"
          onClick={onOpenHold}
          title="Hold current order or resume a held order — F6"
        >
          Hold / Resume <kbd>F6</kbd>
        </button>
      </div>
      <div className="action-group action-group-modifiers">
        {canDiscount && (
          <button
            className="action-btn"
            onClick={onOpenDiscount}
            disabled={lineCount === 0}
            title="Apply bill discount — F8"
          >
            Discount <LockKeyhole size={13} aria-hidden="true" /> <kbd>F8</kbd>
          </button>
        )}
        {canRefund && (
          <button
            className="action-btn action-btn-danger"
            onClick={onOpenRefund}
            title="Process a refund — Ctrl+R"
          >
            Refund <LockKeyhole size={13} aria-hidden="true" /> <kbd>F10</kbd>
          </button>
        )}
      </div>
      <div className="action-group action-group-operational">
        <button className="action-btn" onClick={onOpenCashEvent} title="Cash In / Out / Safe Drop">
          Cash Event <LockKeyhole size={13} aria-hidden="true" />
        </button>
        <button className="action-btn" onClick={onOpenDeliveries} title="View deliveries">
          <Truck size={15} strokeWidth={1.75} aria-hidden="true" /> Deliveries
        </button>
        <button className="action-btn" onClick={onOpenRecent} title="Recent sales — reprint or void">
          <Clock size={15} strokeWidth={1.75} aria-hidden="true" /> Recent
        </button>
        <button
          className="action-btn"
          onClick={onReprintLast}
          disabled={!lastReceiptNumber}
          title={lastReceiptNumber ? `Reprint receipt #${lastReceiptNumber}` : "No receipt to reprint yet"}
        >
          <Printer size={15} strokeWidth={1.75} aria-hidden="true" /> Reprint
        </button>
      </div>
    </div>
  );
}
