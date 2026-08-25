import {
  Banknote, CircleHelp, Clock, DoorOpen, Eraser, HandCoins,
  Printer, RotateCcw, Sparkles, SplitSquareHorizontal, Trash2, Truck, X,
} from "lucide-react";
import type { ReactNode } from "react";

interface Props {
  open: boolean;
  lineCount: number;
  canDiscount: boolean;
  canRefund: boolean;
  lastReceiptNumber: string | null;
  /** Named so the drawer can say what Void would remove, not just that it can. */
  selectedLineName: string | null;
  onClose: () => void;
  onClearCart: () => void;
  onVoidLine: () => void;
  onOpenDiscount: () => void;
  onOpenRefund: () => void;
  onOpenCashEvent: () => void;
  onOpenDeliveries: () => void;
  onOpenRecent: () => void;
  onReprintLast: () => void;
  onCustomItem: () => void;
  onNoSale: () => void;
  onPaySplit: () => void;
  onHelp: () => void;
}

interface ItemProps {
  icon: ReactNode;
  label: string;
  hint?: string;
  shortcut?: string;
  danger?: boolean;
  disabled?: boolean;
  onClick: () => void;
}

function Item({ icon, label, hint, shortcut, danger, disabled, onClick }: ItemProps) {
  return (
    <button
      type="button"
      className={`till-more-item${danger ? " till-more-item-danger" : ""}`}
      disabled={disabled}
      onClick={onClick}
    >
      <span className="till-more-icon">{icon}</span>
      <span className="till-more-text">
        <strong>{label}</strong>
        {hint && <small>{hint}</small>}
      </span>
      {shortcut && <span className="till-key">{shortcut}</span>}
    </button>
  );
}

/**
 * Everything the sale loop does not need in front of it.
 *
 * These actions were previously a permanent rail across the bottom of the till,
 * all eight at equal weight despite being used anywhere from once a sale to
 * twice a shift. None of them was removed — they are one tap away instead of
 * competing with the cart for attention.
 */
export default function PosMoreDrawer({
  open, lineCount, canDiscount, canRefund, lastReceiptNumber, selectedLineName, onClose,
  onClearCart, onVoidLine, onOpenDiscount, onOpenRefund, onOpenCashEvent,
  onOpenDeliveries, onOpenRecent, onReprintLast, onCustomItem, onNoSale,
  onPaySplit, onHelp,
}: Props) {
  if (!open) return null;
  const run = (fn: () => void) => () => { onClose(); fn(); };

  return (
    <div className="till-more-backdrop" role="presentation" onClick={onClose}>
      <div
        className="till-more-drawer"
        role="dialog"
        aria-modal="true"
        aria-label="More options"
        onClick={event => event.stopPropagation()}
      >
        <header className="till-more-head">
          <h2>More Options</h2>
          <button type="button" className="till-more-close" onClick={onClose} aria-label="Close">
            <X size={20} aria-hidden="true" />
          </button>
        </header>

        <div className="till-more-grid">
          {/* Void moved in from the rail under the cart, trading places with
              Hold. Removing a line is the expensive mistake at a till and is
              reached far less often than parking a bill, so it is worth the
              extra tap; the keyboard shortcut is unchanged for anyone who
              knows it. */}
          <Item
            icon={<Trash2 size={20} aria-hidden="true" />}
            label="Void Line" hint={selectedLineName ?? "Nothing selected"}
            shortcut="Del" danger
            disabled={!selectedLineName} onClick={run(onVoidLine)}
          />
          <Item
            icon={<SplitSquareHorizontal size={20} aria-hidden="true" />}
            label="Split Payment" hint="Across several methods"
            disabled={lineCount === 0} onClick={run(onPaySplit)}
          />
          <Item
            icon={<Sparkles size={20} aria-hidden="true" />}
            label="Custom Item" hint="Not in the catalogue"
            onClick={run(onCustomItem)}
          />
          <Item
            icon={<HandCoins size={20} aria-hidden="true" />}
            label="Bill Discount" hint="Whole sale"
            /* No keycap: this said F8, which opens the Digital journey. F5 and
               Ctrl+D discount the selected line, not the bill, so there is no
               key for this one and printing a wrong one is worse than none. */
            disabled={lineCount === 0 || !canDiscount}
            onClick={run(onOpenDiscount)}
          />
          <Item
            icon={<Banknote size={20} aria-hidden="true" />}
            label="Cash Event" hint="Cash in, out or safe drop"
            onClick={run(onOpenCashEvent)}
          />
          <Item
            icon={<DoorOpen size={20} aria-hidden="true" />}
            label="No Sale" hint="Open the drawer"
            shortcut="F11" onClick={run(onNoSale)}
          />
          <Item
            icon={<Truck size={20} aria-hidden="true" />}
            label="Deliveries" hint="Open orders out for delivery"
            onClick={run(onOpenDeliveries)}
          />
          <Item
            icon={<Clock size={20} aria-hidden="true" />}
            label="Recent Sales" hint="Reprint or void"
            onClick={run(onOpenRecent)}
          />
          <Item
            icon={<Printer size={20} aria-hidden="true" />}
            label="Reprint Receipt"
            hint={lastReceiptNumber ? `#${lastReceiptNumber}` : "Nothing printed yet"}
            shortcut="Ctrl+P"
            disabled={!lastReceiptNumber}
            onClick={run(onReprintLast)}
          />
          <Item
            icon={<CircleHelp size={20} aria-hidden="true" />}
            label="Shortcuts" hint="Keyboard help"
            shortcut="Ctrl+H" onClick={run(onHelp)}
          />
          {canRefund && (
            <Item
              icon={<RotateCcw size={20} aria-hidden="true" />}
              label="Refund" hint="Return against a receipt"
              shortcut="F10" danger onClick={run(onOpenRefund)}
            />
          )}
          <Item
            icon={<Eraser size={20} aria-hidden="true" />}
            label="Clear Sale" hint="Empty the basket"
            shortcut="Ctrl+⌫" danger
            disabled={lineCount === 0} onClick={run(onClearCart)}
          />
        </div>
      </div>
    </div>
  );
}
