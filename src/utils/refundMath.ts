import type { SaleItemForRefund } from "../types";

/** Quantity map: sale_item_id → how many units to refund (0 = skip). */
export type QtyMap = Map<string, number>;

export function initQtyMap(items: SaleItemForRefund[]): QtyMap {
  const m = new Map<string, number>();
  for (const item of items) {
    m.set(item.sale_item_id, parseFloat(item.quantity) || 0);
  }
  return m;
}

/**
 * What returning `qty` units of a line is worth.
 *
 * The share of `line_total_minor`, not `qty * unit_price_minor`. A line the
 * manager discounted was never sold at its unit price, so refunding at that
 * price hands back more than the customer paid — and the backend's per-line
 * ceiling would then refuse the last unit of a line the customer had fully
 * returned. The two have to agree, and the line total is the one that says what
 * was actually collected.
 */
export function lineRefundAmount(item: SaleItemForRefund, qty: number): number {
  const origQty = parseFloat(item.quantity);
  // `!(x > 0)` rather than `x <= 0`: an unparseable quantity gives NaN, and every
  // comparison against NaN is false, so `<= 0` let it through and the function
  // returned NaN — a refund amount that is not a number at all.
  if (!(origQty > 0)) return 0;
  return Math.round((qty * item.line_total_minor) / origQty);
}
