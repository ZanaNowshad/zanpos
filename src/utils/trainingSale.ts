import type { Cart, PaymentInput, SaleResult, SessionUser } from "../types";

/**
 * Training mode: build a SaleResult from a cart entirely in the browser,
 * without calling `pos_finalize_sale`.
 *
 * The point is that a new cashier can rehearse the whole till loop — scan,
 * cart, payment, receipt, and a real print on real paper — on their first day
 * without any of it reaching the books. Nothing here writes to the database,
 * so a training sale cannot appear in reports, EOD, stock movements or the
 * receipt sequence. That is a structural guarantee rather than a filter
 * someone has to remember to add to every future report query.
 *
 * The receipt number is deliberately not sequence-shaped: it must never be
 * mistaken for a real one on a printed slip.
 */
export function buildTrainingSale(
  cart: Cart,
  payments: PaymentInput[],
  session: SessionUser,
  branchName: string,
  currency: string,
): SaleResult {
  const active = cart.lines.filter(line => !line.voided);
  const grossTotal = active.reduce((sum, line) => sum + line.line_total_minor, 0);
  const netTotal = Math.max(0, grossTotal - cart.bill_discount_minor);
  const taxTotal = active.reduce((sum, line) => sum + line.tax_amount_minor, 0);
  const discountTotal =
    cart.bill_discount_minor + active.reduce((sum, line) => sum + line.line_discount_minor, 0);

  const now = new Date();
  const tendered = payments.reduce((sum, p) => sum + (p.tendered_minor ?? p.amount_minor), 0);

  return {
    sale_id: `training-${now.getTime()}`,
    receipt_number: "TRAINING",
    net_total_minor: netTotal,
    tax_total_minor: taxTotal,
    discount_total_minor: discountTotal,
    currency,
    payments: payments.map(payment => ({
      method: payment.method,
      amount_minor: payment.amount_minor,
      change_minor:
        payment.method === "cash" ? Math.max(0, tendered - netTotal) : null,
    })),
    items: active.map(line => ({
      product_name: line.product_name,
      quantity: line.quantity,
      unit_price_minor: line.unit_price_minor,
      line_total_minor: line.line_total_minor,
      tax_amount_minor: line.tax_amount_minor,
    })),
    cashier_name: session.display_name,
    branch_name: branchName,
    sold_at: now.toISOString(),
    business_date: now.toISOString().slice(0, 10),
    created_offline: false,
    low_stock_alerts: [],
    delivery: undefined,
  };
}
