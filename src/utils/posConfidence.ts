export type ReceiptConfidenceStatus = "not_ready" | "ready" | "printing" | "printed" | "failed" | "sent";
export type BenefitConfidenceStatus = "none" | "recorded" | "pending" | "confirmed";
export type ConfidenceTone = "ok" | "info" | "warning" | "critical";

export interface CheckoutConfidenceInput {
  stockChecked: boolean;
  saleSaved: boolean;
  syncOnline: boolean | null;
  pendingSync: number;
  receiptStatus: ReceiptConfidenceStatus;
  benefitStatus: BenefitConfidenceStatus;
}

export interface CheckoutConfidenceItem {
  key: string;
  label: string;
  detail: string;
  tone: ConfidenceTone;
}

export function buildCheckoutConfidenceItems(input: CheckoutConfidenceInput): CheckoutConfidenceItem[] {
  const syncDetail = input.syncOnline
    ? (input.pendingSync > 0 ? `${input.pendingSync} pending` : "Synced")
    : (input.pendingSync > 0 ? `${input.pendingSync} waiting` : "Offline");

  const receipt: Record<ReceiptConfidenceStatus, Omit<CheckoutConfidenceItem, "key">> = {
    not_ready: { label: "Receipt ready after payment", detail: "Not issued", tone: "info" },
    ready: { label: "Receipt ready", detail: "Print or send", tone: "info" },
    printing: { label: "Receipt printing", detail: "Printer working", tone: "info" },
    printed: { label: "Receipt printed", detail: "Hardware confirmed", tone: "ok" },
    failed: { label: "Receipt needs reprint", detail: "Printer did not respond", tone: "warning" },
    sent: { label: "Receipt sent", detail: "WhatsApp delivered", tone: "ok" },
  };

  const benefit: Record<BenefitConfidenceStatus, Omit<CheckoutConfidenceItem, "key">> = {
    none: { label: "BenefitPay not used", detail: "No wallet payment", tone: "info" },
    recorded: { label: "BenefitPay recorded", detail: "Paid at till", tone: "ok" },
    pending: { label: "BenefitPay pending", detail: "Waiting for proof", tone: "warning" },
    confirmed: { label: "BenefitPay confirmed", detail: "Payment verified", tone: "ok" },
  };

  return [
    {
      key: "stock",
      label: input.stockChecked ? "Stock checked" : "Stock not checked",
      detail: input.stockChecked ? "Current cart validated" : "Add items to validate",
      tone: input.stockChecked ? "ok" : "info",
    },
    {
      key: "sale",
      label: input.saleSaved ? "Sale saved locally" : "Sale ready to save",
      detail: input.saleSaved ? "SQLite committed" : "After payment",
      tone: input.saleSaved ? "ok" : "info",
    },
    {
      key: "sync",
      label: input.syncOnline && input.pendingSync === 0 ? "Sync synced" : "Sync pending",
      detail: syncDetail,
      tone: input.syncOnline ? "ok" : "warning",
    },
    { key: "receipt", ...receipt[input.receiptStatus] },
    { key: "benefit", ...benefit[input.benefitStatus] },
  ];
}
