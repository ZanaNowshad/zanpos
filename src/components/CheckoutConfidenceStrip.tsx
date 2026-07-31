import { CheckCircle2, CircleDashed, ReceiptText, RefreshCw, ShieldCheck, Smartphone } from "lucide-react";
import type { CheckoutConfidenceItem } from "../utils/posConfidence";

interface Props {
  items: CheckoutConfidenceItem[];
  compact?: boolean;
}

const ICONS: Record<string, typeof CheckCircle2> = {
  stock: ShieldCheck,
  sale: CheckCircle2,
  sync: RefreshCw,
  receipt: ReceiptText,
  benefit: Smartphone,
};

export default function CheckoutConfidenceStrip({ items, compact = false }: Props) {
  return (
    <div className={`checkout-confidence${compact ? " checkout-confidence-compact" : ""}`} aria-label="Checkout confidence">
      {items.map(item => {
        const Icon = ICONS[item.key] ?? CircleDashed;
        return (
          <div key={item.key} className={`checkout-confidence-item checkout-confidence-${item.tone}`}>
            <Icon size={14} strokeWidth={1.8} aria-hidden="true" />
            <span className="checkout-confidence-copy">
              <span className="checkout-confidence-label">{item.label}</span>
              {!compact && <span className="checkout-confidence-detail">{item.detail}</span>}
            </span>
          </div>
        );
      })}
    </div>
  );
}
