import { CheckCircle2, Eye, MessageCircle, Printer, ShoppingCart, X } from "lucide-react";
import CheckoutConfidenceStrip from "./CheckoutConfidenceStrip";
import type { SaleResult } from "../types";
import type { CheckoutConfidenceItem, ReceiptConfidenceStatus } from "../utils/posConfidence";

export type WhatsAppReceiptStatus = "not_available" | "ready" | "sending" | "sent" | "failed";

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

  return (
    <section className="receipt-action-center" role="status" aria-live="polite">
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
