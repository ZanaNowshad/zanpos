import type { AdminProduct } from "../types";
import type { BulkCategoryRow, BulkProductRow } from "../tauri/commands";
import { formatMoney } from "../money";

/**
 * The products CSV contract, declared once.
 *
 * The import template and the catalogue export are the same shape on purpose:
 * a shopkeeper exports the catalogue, edits it in a spreadsheet, and imports it
 * back. Previously the template was a string literal inside BulkImportModal, so
 * an export written separately could drift from what the importer accepts. Both
 * are now built from `PRODUCT_CSV_COLUMNS`.
 *
 * Conventions are fixed by `rowsToProducts` in BulkImportModal:
 *   • `price`           major units, e.g. 0.400 — `formatMoney` produces this.
 *   • `barcodes`        pipe-separated; the importer prefers it over `barcode`.
 *   • `track_inventory` "false" / "0" / "no" are false, anything else is true.
 *   • `tax_rule_name`   matched by name, blank means no rule.
 */
export const PRODUCT_CSV_COLUMNS = [
  "name", "category_name", "price", "sku", "barcodes", "track_inventory", "tax_rule_name",
] as const;

export const PRODUCT_TEMPLATE_CSV = [
  PRODUCT_CSV_COLUMNS.join(","),
  "Coca-Cola 330ml,Drinks,0.400,COLA-330,5449000000996,true,VAT 10%",
  "Pepsi 330ml,Drinks,0.350,PEPS-330,1234567890|9876543210,true,VAT 10%",
  "Water 500ml,Drinks,0.250,WATR-500,,true,Zero Rate",
  "Sandwich,Food,0.800,,,true,",
].join("\n");

export const CATEGORY_TEMPLATE_CSV = [
  "name,sort_order", "Drinks,1", "Food,2", "Misc,3",
].join("\n");

/**
 * Quote a field for the importer's parser.
 *
 * That parser splits the file into lines before parsing quotes, so a newline
 * inside a quoted field would silently truncate the row. Any newline or tab is
 * collapsed to a space rather than escaped — losing a line break in a product
 * name is preferable to losing the rest of the product.
 */
export function csvCell(value: string | null | undefined): string {
  const flat = (value ?? "").replace(/[\r\n\t]+/g, " ").trim();
  return /[",]/.test(flat) ? `"${flat.replace(/"/g, '""')}"` : flat;
}

/** Every barcode the product carries, in the importer's pipe-separated form. */
function barcodeField(p: AdminProduct): string {
  const all = p.barcodes?.length ? p.barcodes : p.barcode ? [p.barcode] : [];
  return all.filter(Boolean).join("|");
}

/**
 * Serialise products into a file the importer will accept unchanged.
 *
 * `is_active` is deliberately absent: the template has no such column, so
 * writing one would produce a file that round-trips inconsistently — the
 * importer would ignore it and every re-imported product would come back
 * active. Filter before exporting instead.
 */
export function productsToCsv(products: AdminProduct[], currencyExponent: number): string {
  const rows = products.map(p => [
    csvCell(p.name),
    csvCell(p.category_name),
    formatMoney(p.price_minor, currencyExponent),
    csvCell(p.sku),
    csvCell(barcodeField(p)),
    p.track_inventory ? "true" : "false",
    csvCell(p.tax_rule_name),
  ].join(","));
  return [PRODUCT_CSV_COLUMNS.join(","), ...rows].join("\n");
}

// ─── Reading ──────────────────────────────────────────────────────────────────
// Parsing lives beside serialising so the two halves of the contract are read
// together and the round-trip can be tested against the real importer, not a
// second copy of it.

export function parseCSV(text: string): string[][] {
  const lines = text.replace(/\r\n/g, "\n").replace(/\r/g, "\n").split("\n");
  return lines
    .filter(l => l.trim() !== "")
    .map(line => {
      const cols: string[] = [];
      let cur = "";
      let inQuote = false;
      for (let i = 0; i < line.length; i++) {
        const ch = line[i];
        if (ch === '"') {
          if (inQuote && line[i + 1] === '"') { cur += '"'; i++; }
          else { inQuote = !inQuote; }
        } else if (ch === "," && !inQuote) {
          cols.push(cur); cur = "";
        } else {
          cur += ch;
        }
      }
      cols.push(cur);
      return cols.map(c => c.trim());
    });
}

export function rowsToCategories(rows: string[][]): BulkCategoryRow[] {
  if (rows.length < 2) return [];
  const header = rows[0].map(h => h.toLowerCase());
  const nameIdx = header.findIndex(h => h === "name");
  const orderIdx = header.findIndex(h => h.includes("sort") || h.includes("order"));
  if (nameIdx < 0) return [];
  return rows.slice(1).map(r => ({
    name: r[nameIdx] ?? "",
    sort_order: orderIdx >= 0 && r[orderIdx] ? parseInt(r[orderIdx]) || undefined : undefined,
  }));
}

export function rowsToProducts(rows: string[][]): BulkProductRow[] {
  if (rows.length < 2) return [];
  const header = rows[0].map(h => h.toLowerCase().replace(/[^a-z_]/g, "_"));
  const col = (names: string[]) =>
    names.reduce((acc, n) => acc >= 0 ? acc : header.findIndex(h => h.includes(n)), -1);

  const nameIdx     = col(["name"]);
  const catIdx      = col(["category"]);
  const priceIdx    = col(["price"]);
  const skuIdx      = col(["sku"]);
  // `barcodes` (plural, pipe-separated) takes priority over single `barcode`
  const barcodesIdx = header.findIndex(h => h === "barcodes");
  const barcodeIdx  = barcodesIdx >= 0 ? -1 : col(["barcode"]);
  const trackIdx    = col(["track", "inventory"]);
  const taxIdx      = col(["tax"]);

  if (nameIdx < 0 || catIdx < 0 || priceIdx < 0) return [];

  return rows.slice(1).map(r => ({
    name:            r[nameIdx] ?? "",
    category_name:   catIdx >= 0 ? (r[catIdx] ?? "") : "",
    price:           priceIdx >= 0 ? (r[priceIdx] ?? "0") : "0",
    sku:             skuIdx >= 0 ? (r[skuIdx] || undefined) : undefined,
    barcode:         barcodeIdx >= 0 ? (r[barcodeIdx] || undefined) : undefined,
    barcodes:        barcodesIdx >= 0 ? (r[barcodesIdx] || undefined) : undefined,
    track_inventory: trackIdx >= 0
      ? !["false", "0", "no"].includes((r[trackIdx] ?? "").toLowerCase())
      : undefined,
    tax_rule_name:   taxIdx >= 0 ? (r[taxIdx] || undefined) : undefined,
  }));
}

/** Trigger a browser download. Kept here so callers do not repeat the dance. */
export function downloadCsv(filename: string, content: string): void {
  // BOM so Excel opens Arabic product names as UTF-8 rather than mojibake.
  const blob = new Blob(["﻿", content], { type: "text/csv;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}
