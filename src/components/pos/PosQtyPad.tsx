import { Minus, Plus, X } from "lucide-react";
import Dialpad from "../Dialpad";
import type { CartLine } from "../../types";

interface Props {
  line: CartLine | null;
  value: string;
  onKey: (key: string) => void;
  onBump: (delta: number) => void;
  onClose: () => void;
}

/**
 * The dial pad, summoned for a quantity instead of resident on the till.
 *
 * It opens over the tender column — the same place the old permanent pad
 * occupied — so the reach is unchanged and muscle memory holds. What changed is
 * that the screen is not spent on it while a scanner does the work, which is
 * most of a basket.
 *
 * Keys drive `usePosNumpad`, so the quantity applies to the line as it is typed
 * exactly as the old pad did. The plus/minus pair is the one-tap path for the
 * common correction — a second of the same item — without typing at all.
 */
export default function PosQtyPad({ line, value, onKey, onBump, onClose }: Props) {
  if (!line) return null;

  return (
    <div className="till-qtypad-backdrop" role="presentation" onClick={onClose}>
      <div
        className="till-qtypad"
        role="dialog"
        aria-modal="true"
        aria-label={`Quantity for ${line.product_name}`}
        onClick={event => event.stopPropagation()}
      >
        <header className="till-qtypad-head">
          <div className="till-qtypad-title">
            <small>Quantity</small>
            <strong>{line.product_name}</strong>
          </div>
          <button type="button" onClick={onClose} aria-label="Close quantity pad">
            <X size={20} aria-hidden="true" />
          </button>
        </header>

        <div className="till-qtypad-value" aria-live="polite" aria-atomic="true">
          <button type="button" onClick={() => onBump(-1)} aria-label="One fewer">
            <Minus size={20} aria-hidden="true" />
          </button>
          <span className="till-qtypad-number">{line.quantity}</span>
          <button type="button" onClick={() => onBump(1)} aria-label="One more">
            <Plus size={20} aria-hidden="true" />
          </button>
        </div>

        <Dialpad onKey={onKey} />

        <button type="button" className="till-qtypad-done" onClick={onClose}>
          Done <kbd>{value}</kbd>
        </button>
      </div>
    </div>
  );
}
