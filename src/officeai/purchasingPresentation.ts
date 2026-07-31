import { formatMoney } from "../money";
import type { MarginSummary, ProductMarginRow, PurchaseOrderRow, SupplierRow } from "../types";

export interface PurchasingCommandModel {
  supplierCount: number;
  activeSupplierCount: number;
  openPoCount: number;
  receivingValueMinor: number;
  unknownCostLineCount: number;
  marginWarning: boolean;
}

export function buildPurchasingCommandModel(
  suppliers: SupplierRow[],
  orders: PurchaseOrderRow[],
  margin: MarginSummary | null,
): PurchasingCommandModel {
  const openOrders = orders.filter(po => ["draft", "ordered", "partial"].includes(po.status));
  return {
    supplierCount: suppliers.length,
    activeSupplierCount: suppliers.filter(s => s.is_active).length,
    openPoCount: openOrders.length,
    receivingValueMinor: openOrders.reduce((sum, po) => sum + Math.max(0, po.ordered_total_minor - po.received_total_minor), 0),
    unknownCostLineCount: margin?.unknown_cost_line_count ?? 0,
    marginWarning: (margin?.unknown_cost_line_count ?? 0) > 0 || (margin?.margin_basis_points ?? 10_000) < 2500,
  };
}

function isoDate(date: Date): string {
  return date.toLocaleDateString("en-CA", { timeZone: "Asia/Bahrain" });
}

export function defaultPurchasingRange() {
  const to = new Date();
  const from = new Date();
  from.setDate(from.getDate() - 29);
  return { from: isoDate(from), to: isoDate(to) };
}

export function purchasingMoney(minor: number, exponent: number): string {
  return `BHD ${formatMoney(minor, exponent)}`;
}

export function downloadPurchasingCsv(
  filename: string,
  rows: Array<Record<string, string | number | null | undefined>>,
) {
  const headers = Object.keys(rows[0] ?? {});
  const escapeValue = (value: string | number | null | undefined) =>
    `"${String(value ?? "").replace(/"/g, '""')}"`;
  const csv = [
    headers.join(","),
    ...rows.map(row => headers.map(header => escapeValue(row[header])).join(",")),
  ].join("\n");
  const blob = new Blob([csv], { type: "text/csv;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}

export type PurchasingCsvRow = ProductMarginRow | PurchaseOrderRow | SupplierRow;
