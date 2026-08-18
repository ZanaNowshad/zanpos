import { useEffect, useRef, useState } from "react";
import { CheckCircle2, Eye, MessageCircle, Printer, ShoppingCart, X } from "lucide-react";
import CheckoutConfidenceStrip from "./CheckoutConfidenceStrip";
import type { SaleResult } from "../types";
import type { CheckoutConfidenceItem, ReceiptConfidenceStatus } from "../utils/posConfidence";

export type WhatsAppReceiptStatus = "not_available" | "ready" | "sending" | "sent" | "failed";

/** Kept in step with the `--receipt-banner-life` countdown in App.css. */
export const RECEIPT_BANNER_LIFE_MS = 12_000;

interface Props {
  sale: SaleResult;
  receiptStatus: ReceiptConfidenceStatus;
  whatsappStatus: WhatsAppReceiptStatus;
  confidenceItems?: CheckoutConfidenceItem[];
  onPrint: () => void;
  onSendWhatsApp?: () => void;
  onNewSale: () => void;
  onViewDetails: () => void;
  onDismiss: () => void;
}

const receiptLabel: Record<ReceiptConfidenceStatus, string> = {
  not_ready: "Receipt ready",
  ready: "Ready",
  printing: "Printing",
  printed: "Printed",
  failed: "Print failed",
  sent: "Sent",
};

const whatsappLabel: Record<WhatsAppReceiptStatus, string> = {
  not_available: "No WhatsApp target",
  ready: "Ready to send",
  sending: "Sending",
  sent: "Sent",
  failed: "Send failed",
};

export default function ReceiptActionCenter({
  sale,
  receiptStatus,
  whatsappStatus,
  confidenceItems,
  onPrint,
  onSendWhatsApp,
  onNewSale,
  onViewDetails,
  onDismiss,
}: Props) {
  const disablePrint = receiptStatus === "printing";
  const disableWhatsApp = whatsappStatus === "not_available" || whatsappStatus === "sending";

  /*
   * The banner used to stay up until the next sale cleared it, which on a busy
   * till meant it simply lived on screen. It now clears itself, but never while
   * the cashier is reaching for one of its buttons — hovering or tabbing into
   * it holds it open, and the countdown bar shows the time left. A print that
   * is still running also holds it: that is the one state where the outcome is
   * not yet known.
   */
  const [held, setHeld] = useState(false);
  const dismissRef = useRef(onDismiss);
  useEffect(() => { dismissRef.current = onDismiss; }, [onDismiss]);

  useEffect(() => {
    if (held || receiptStatus === "printing" || whatsappStatus === "sending") return;
    const timer = setTimeout(() => dismissRef.current(), RECEIPT_BANNER_LIFE_MS);
    return () => clearTimeout(timer);
  }, [held, receiptStatus, whatsappStatus]);

  return (
    <section
      className="receipt-action-center"
      role="status"
      aria-live="polite"
      onMouseEnter={() => setHeld(true)}
      onMouseLeave={() => setHeld(false)}
      onFocusCapture={() => setHeld(true)}
      onBlurCapture={event => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setHeld(false);
      }}
    >
      <div className="receipt-action-summary">
        <span className="receipt-action-icon" aria-hidden="true"><CheckCircle2 size={18} /></span>
        <div>
          <strong>Sale #{sale.receipt_number}</strong>
          <span>
            {receiptLabel[receiptStatus]}
            {onSendWhatsApp ? ` · ${whatsappLabel[whatsappStatus]}` : ""}
          </span>
        </div>
      </div>
      {confidenceItems && confidenceItems.length > 0 && (
        <CheckoutConfidenceStrip items={confidenceItems} compact />
      )}
      <div className="receipt-action-buttons">
        <button onClick={onPrint} disabled={disablePrint}>
          <Printer size={15} /> Print
        </button>
        {onSendWhatsApp && (
          <button onClick={onSendWhatsApp} disabled={disableWhatsApp}>
            <MessageCircle size={15} /> Send WhatsApp
          </button>
        )}
        <button onClick={onViewDetails}>
          <Eye size={15} /> View details
        </button>
        <button className="receipt-action-primary" onClick={onNewSale}>
          <ShoppingCart size={15} /> New sale
        </button>
        <button className="receipt-action-close" onClick={onDismiss} aria-label="Dismiss receipt actions">
          <X size={16} />
        </button>
      </div>
    </section>
  );
}
