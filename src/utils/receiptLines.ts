/**
 * Shared receipt line builder — produces the 48-char plain-text lines fed to
 * the ESC/POS printer. Used by POS sale, reprint, and auto-print paths.
 */
import type { BranchSettings, SaleResult } from "../types";
import { loadReceiptDesign, type ReceiptDesign } from "../components/ReceiptDesignEditor";
import { formatMoney } from "../money";
import { DEVICE } from "../types";

const W = 48;

/** Right-pad a two-column row to exactly W characters. */
function pad(l: string, r: string): string {
  const space = W - l.length - r.length;
  return l + " ".repeat(Math.max(1, space)) + r;
}

const DIVIDER = "-".repeat(W);

export type ReceiptPrintTrigger = "auto" | "manual";
export type ReceiptPrintOutcome = "printed" | "skipped" | "unavailable";

interface PerformReceiptPrintInput {
  sale: SaleResult;
  settings: BranchSettings | null;
  trigger: ReceiptPrintTrigger;
  autoPrintEnabled: boolean;
  thermalEnabled: boolean;
  isReprint?: boolean;
  print: (storeName: string, lines: string[]) => Promise<string>;
}

/**
 * One policy for every sale-time receipt path.
 * Automatic prints honor the business toggle; explicit prints never do.
 */
export async function performReceiptPrint({
  sale,
  settings,
  trigger,
  autoPrintEnabled,
  thermalEnabled,
  isReprint = false,
  print,
}: PerformReceiptPrintInput): Promise<ReceiptPrintOutcome> {
  if (!thermalEnabled) return "unavailable";
  if (trigger === "auto" && !autoPrintEnabled) return "skipped";

  await print(
    sale.branch_name || settings?.name || "ZANPOS",
    buildReceiptLines(sale, settings, isReprint),
  );
  return "printed";
}

/** Build the full plain-text receipt body for ESC/POS printing. */
export function buildReceiptLines(
  sale: SaleResult,
  settings: BranchSettings | null,
  isReprint = false,
  /**
   * What the owner chose to show on the receipt.
   *
   * Read fresh at print time rather than captured once, so a change in Settings
   * takes effect on the next receipt without restarting the till — the same
   * discipline `usePosReceipt` already applies to the branch settings.
   *
   * These toggles were saved to localStorage and never read: the editor showed
   * a live preview that honoured every one of them, and the printer ignored all
   * of them. Unchecking "Show Phone" appeared to work and changed nothing.
   */
  design: ReceiptDesign = loadReceiptDesign(),
): string[] {
  const fmt = (n: number) => formatMoney(n, DEVICE.currency_exponent);
  const lines: string[] = [];

  if (isReprint) {
    lines.push("*".repeat(W));
    lines.push("*" + " DUPLICATE — NOT ORIGINAL ".padStart(25 + 13).padEnd(W - 1) + "*");
    lines.push("*".repeat(W));
  }

  if (design.show_store_name && settings?.name)     lines.push(settings.name);
  if (design.show_address    && settings?.address)  lines.push(settings.address);
  if (design.show_phone      && settings?.phone)    lines.push(settings.phone);
  if (design.show_tax_number && settings?.tax_number) lines.push(`TRN: ${settings.tax_number}`);
  if (design.show_tax_number && settings?.cr_number)  lines.push(`CR No: ${settings.cr_number}`);
  if (design.show_header     && settings?.receipt_header) lines.push(settings.receipt_header);

  lines.push(DIVIDER);
  lines.push(`Receipt: #${sale.receipt_number}`);
  lines.push(
    new Date(sale.sold_at).toLocaleString("en-BH", {
      timeZone: "Asia/Bahrain",
      dateStyle: "short",
      timeStyle: "short",
    }),
  );
  lines.push(`Cashier: ${sale.cashier_name}`);
  lines.push(DIVIDER);

  for (const item of sale.items) {
    lines.push(
      pad(
        `${item.product_name} x${item.quantity}`,
        `${DEVICE.currency} ${fmt(item.line_total_minor)}`,
      ),
    );
  }

  lines.push(DIVIDER);
  if (sale.discount_total_minor > 0)
    lines.push(pad("Discount", `- ${DEVICE.currency} ${fmt(sale.discount_total_minor)}`));
  if (sale.tax_total_minor > 0) {
    lines.push(
      pad("Subtotal (excl. VAT)", `${DEVICE.currency} ${fmt(sale.net_total_minor - sale.tax_total_minor)}`),
    );
    lines.push(pad("VAT", `${DEVICE.currency} ${fmt(sale.tax_total_minor)}`));
  }
  lines.push(pad("TOTAL", `${DEVICE.currency} ${fmt(sale.net_total_minor)}`));
  lines.push(DIVIDER);

  for (const p of sale.payments) {
    const method = p.method.charAt(0).toUpperCase() + p.method.slice(1);
    lines.push(pad(method, `${DEVICE.currency} ${fmt(p.amount_minor)}`));
    if (p.change_minor && p.change_minor > 0)
      lines.push(pad("Change", `${DEVICE.currency} ${fmt(p.change_minor)}`));
  }

  // Delivery section
  if (sale.delivery) {
    const d = sale.delivery;
    lines.push(DIVIDER);
    const isPaid = d.payment_status === "paid";
    lines.push(
      isPaid ? "** DELIVERY ORDER — PAID **" : "** DELIVERY ORDER — PAYMENT PENDING **",
    );
    if (d.customer_name) lines.push(`Customer: ${d.customer_name}`);
    lines.push(`Contact:  ${d.contact_number}`);
    if (d.house_number || d.area)
      lines.push(`Address:  ${[d.house_number, d.area].filter(Boolean).join(", ")}`);
    if (d.address_text) lines.push(`          ${d.address_text}`);
    if (d.delivery_staff_name) lines.push(`Rider:    ${d.delivery_staff_name}`);
    const method =
      d.expected_payment_method === "wallet"
        ? "BenefitPay"
        : d.expected_payment_method.charAt(0).toUpperCase() + d.expected_payment_method.slice(1);
    lines.push(`Expected: ${method}`);
    if (isPaid && d.paid_confirmed_at)
      lines.push(`Paid at:  ${new Date(d.paid_confirmed_at).toLocaleString()}`);
  }

  if (design.show_footer && settings?.receipt_footer) {
    lines.push(DIVIDER);
    lines.push(settings.receipt_footer);
  }

  return lines;
}
