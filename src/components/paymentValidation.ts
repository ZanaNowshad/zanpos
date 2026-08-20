import type { DeliveryInput, PaymentInput } from "../types";
import { DEVICE } from "../types";
import { parseMoney } from "../money";
import type { DetailStringKey } from "../i18n/detailStrings";

export interface PaymentLineState {
  method: PaymentInput["method"];
  amountStr: string;
  tenderedStr: string;
}

export interface PaymentValidationInput {
  lines: PaymentLineState[];
  netTotal: number;
  allocatedMinor: number;
  remainingMinor: number;
  currencyExponent: number;
  requiresContact: boolean;
  isDelivery: boolean;
  deliveryData: Partial<DeliveryInput>;
  loading?: boolean;
}

/**
 * Whether the sale can be completed, and if not, why.
 *
 * Two functions rather than one because they answer different questions and the
 * button needs both: `canConfirm` gates the action, `confirmBlockReason` tells
 * the cashier what to fix. Kept pure and out of the component so the money
 * comparisons — the part where being wrong costs the shop — can be read on
 * their own.
 */
export function canConfirmPayment(input: PaymentValidationInput): boolean {
  const { lines, netTotal, allocatedMinor, currencyExponent: EXP,
          requiresContact, isDelivery, deliveryData } = input;
  if (!lines.length) return false;
  for (const l of lines) {
    if (parseMoney(l.amountStr, EXP) <= 0) return false;
    if (l.method === "cash") {
      const t = parseMoney(l.tenderedStr || l.amountStr, EXP);
      if (t < parseMoney(l.amountStr, EXP)) return false;
    }
  }
  if (Math.abs(allocatedMinor - netTotal) > 1) return false;
  if (requiresContact) {
    if (!deliveryData.contact_number) return false;
  }
  // House number only. Flat and road are optional — a villa has no flat, and
  // plenty of Bahrain addresses are given as a landmark the rider knows.
  if (isDelivery && !deliveryData.house_number?.trim()) return false;
  return true;
}

/** One thing standing between the cashier and a completed sale. */
export interface PaymentBlocker {
  message: string;
  /** CSS selector for the field to focus so the fix is one tap away. */
  focus?: string;
}

/**
 * Every reason this sale cannot complete, not just the first.
 *
 * The button used to be disabled with a single hint beside it, which is the
 * worst of both: nothing happens when you press it, and fixing the reason shown
 * reveals another. Listing them all, on demand, lets the cashier clear the lot
 * in one pass with a queue waiting.
 */
export function paymentBlockers(
  input: PaymentValidationInput,
  dt: (key: DetailStringKey) => string,
  fmt: (minor: number) => string,
): PaymentBlocker[] {
  const { lines, remainingMinor, currencyExponent: EXP,
          requiresContact, isDelivery, deliveryData } = input;
  const out: PaymentBlocker[] = [];

  if (!lines.length || lines.some(line => parseMoney(line.amountStr, EXP) <= 0)) {
    out.push({ message: dt("everyPaymentAmount") });
  }
  const shortCash = lines.find(line =>
    line.method === "cash"
    && parseMoney(line.tenderedStr || line.amountStr, EXP) < parseMoney(line.amountStr, EXP)
  );
  if (shortCash) {
    const short = parseMoney(shortCash.amountStr, EXP)
      - parseMoney(shortCash.tenderedStr || shortCash.amountStr, EXP);
    out.push({
      message: `${DEVICE.currency} ${fmt(short)} ${dt("moreCashNeeded")}`,
      focus: ".pm-tendered-box",
    });
  }
  if (remainingMinor > 1) {
    out.push({ message: `${DEVICE.currency} ${fmt(remainingMinor)} ${dt("stillDue")}` });
  }
  if (remainingMinor < -1) {
    out.push({ message: `${dt("reducePaymentsBy")} ${DEVICE.currency} ${fmt(-remainingMinor)}.` });
  }
  if (requiresContact && !deliveryData.contact_number) {
    out.push({
      message: "Enter the customer's 8-digit mobile number. Any number works — it does not have to be a saved customer.",
      focus: ".pm-contact-input",
    });
  }
  if (isDelivery && !deliveryData.house_number?.trim()) {
    out.push({
      message: "House or building number is required so the rider can find it.",
      focus: ".pm-address-form input",
    });
  }
  return out;
}

export function paymentBlockReason(
  input: PaymentValidationInput,
  dt: (key: DetailStringKey) => string,
  fmt: (minor: number) => string,
): string | null {
  const { lines, remainingMinor, currencyExponent: EXP, loading,
          requiresContact, isDelivery, deliveryData } = input;
  if (loading) return dt("recordingPayment");
  if (!lines.length || lines.some(line => parseMoney(line.amountStr, EXP) <= 0)) return dt("everyPaymentAmount");
  const shortCash = lines.find(line =>
    line.method === "cash"
    && parseMoney(line.tenderedStr || line.amountStr, EXP) < parseMoney(line.amountStr, EXP)
  );
  if (shortCash) {
    const short = parseMoney(shortCash.amountStr, EXP) - parseMoney(shortCash.tenderedStr || shortCash.amountStr, EXP);
    return `${DEVICE.currency} ${fmt(short)} ${dt("moreCashNeeded")}`;
  }
  if (remainingMinor > 1) return `${DEVICE.currency} ${fmt(remainingMinor)} ${dt("stillDue")}`;
  if (remainingMinor < -1) return `${dt("reducePaymentsBy")} ${DEVICE.currency} ${fmt(-remainingMinor)}.`;
  if (requiresContact && !deliveryData.contact_number) return "Enter a valid 8-digit Bahrain customer phone.";
  if (isDelivery && !deliveryData.house_number?.trim()) return "House number is required.";
  return null;
}

/**
 * Turn the modal's line state into the payment records the sale is committed
 * with.
 *
 * `tendered` is floored at the amount due: a cash line where the cashier typed
 * less than the price would otherwise record a tender that never happened and
 * compute negative change. Non-cash methods carry no tender at all — a card
 * terminal settles the exact amount.
 */
export function buildPaymentInputs(
  lines: PaymentLineState[],
  currencyExponent: number,
): PaymentInput[] {
  return lines.map(line => {
    const amount = parseMoney(line.amountStr, currencyExponent);
    if (line.method !== "cash") return { method: line.method, amount_minor: amount };
    const tendered = Math.max(parseMoney(line.tenderedStr || line.amountStr, currencyExponent), amount);
    return { method: line.method, amount_minor: amount, tendered_minor: tendered };
  });
}
