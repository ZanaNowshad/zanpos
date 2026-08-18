import { describe, expect, it } from "vitest";
import type { AdminProduct } from "../types";
import {
  PRODUCT_CSV_COLUMNS,
  PRODUCT_TEMPLATE_CSV,
  csvCell,
  parseCSV,
  productsToCsv,
  rowsToProducts,
} from "../csv/productsCsv";

/**
 * The export exists so a shopkeeper can edit the catalogue in a spreadsheet and
 * import it back. That only holds if the file the exporter writes is one the
 * importer accepts, so these tests round-trip through the real `parseCSV` and
 * `rowsToProducts` rather than asserting against a hand-written string.
 */
const product = (over: Partial<AdminProduct>): AdminProduct => ({
  product_id: "prd_1",
  category_id: "cat_1",
  category_name: "Drinks",
  name: "Coca-Cola 330ml",
  sku: "COLA-330",
  barcode: "5449000000996",
  track_inventory: true,
  allow_decimal_quantity: false,
  is_active: true,
  tax_rule_id: "tax_1",
  tax_rule_name: "VAT 10%",
  price_minor: 400,
  reorder_point: 10,
  image_path: null,
  ...over,
});

const roundTrip = (rows: AdminProduct[], exp = 3) =>
  rowsToProducts(parseCSV(productsToCsv(rows, exp)));

describe("products CSV export", () => {
  it("writes the import template's header, in order", () => {
    const [header] = parseCSV(productsToCsv([product({})], 3));
    expect(header).toEqual([...PRODUCT_CSV_COLUMNS]);
    expect(header).toEqual(parseCSV(PRODUCT_TEMPLATE_CSV)[0]);
  });

  it("round-trips a product through the real importer", () => {
    expect(roundTrip([product({})])[0]).toEqual({
      name: "Coca-Cola 330ml",
      category_name: "Drinks",
      price: "0.400",
      sku: "COLA-330",
      barcode: undefined,          // the importer reads `barcodes` instead
      barcodes: "5449000000996",
      track_inventory: true,
      tax_rule_name: "VAT 10%",
    });
  });

  it("writes price in major units at the store's exponent", () => {
    expect(roundTrip([product({ price_minor: 400 })])[0].price).toBe("0.400");
    expect(roundTrip([product({ price_minor: 12_345 })])[0].price).toBe("12.345");
    // A 2-exponent currency must not be written with 3 decimals.
    expect(roundTrip([product({ price_minor: 400 })], 2)[0].price).toBe("4.00");
  });

  it("joins multiple barcodes the way the importer splits them", () => {
    const p = product({ barcodes: ["1234567890", "9876543210"], barcode: "1234567890" });
    expect(roundTrip([p])[0].barcodes).toBe("1234567890|9876543210");
  });

  it("falls back to the single barcode when there is no list", () => {
    expect(roundTrip([product({ barcodes: undefined })])[0].barcodes).toBe("5449000000996");
  });

  it("survives a name containing a comma", () => {
    // The bug this prevents: an unquoted comma shifts every later column, so
    // the price lands in `sku` and the product re-imports at zero.
    const p = product({ name: "Rice, Basmati 5kg" });
    const back = roundTrip([p])[0];
    expect(back.name).toBe("Rice, Basmati 5kg");
    expect(back.price).toBe("0.400");
  });

  it("survives a name containing quotes", () => {
    const p = product({ name: 'Al Ain 1.5L "Value" Pack' });
    expect(roundTrip([p])[0].name).toBe('Al Ain 1.5L "Value" Pack');
  });

  it("flattens newlines so a row cannot be truncated", () => {
    // parseCSV splits on lines before parsing quotes, so an embedded newline
    // would silently drop the rest of the product.
    const p = product({ name: "Two\nLines" });
    const csv = productsToCsv([p], 3);
    expect(csv.split("\n")).toHaveLength(2);
    expect(roundTrip([p])[0].name).toBe("Two Lines");
  });

  it("writes empty cells, not the word null, for absent optional fields", () => {
    const csv = productsToCsv([product({ sku: null, tax_rule_name: null, barcode: null, barcodes: [] })], 3);
    expect(csv).not.toMatch(/null|undefined/);
    const back = rowsToProducts(parseCSV(csv))[0];
    expect(back.sku).toBeUndefined();
    expect(back.tax_rule_name).toBeUndefined();
    expect(back.barcodes).toBeUndefined();
  });

  it("preserves track_inventory in both directions", () => {
    expect(roundTrip([product({ track_inventory: false })])[0].track_inventory).toBe(false);
    expect(roundTrip([product({ track_inventory: true })])[0].track_inventory).toBe(true);
  });

  it("omits is_active, which the template cannot carry", () => {
    // Writing a column the importer ignores would make the round trip lie:
    // every inactive product would come back active.
    expect(PRODUCT_CSV_COLUMNS).not.toContain("is_active");
    expect(productsToCsv([product({ is_active: false })], 3)).not.toMatch(/is_active|false,\s*$/);
  });

  it("emits a header-only file for an empty catalogue", () => {
    expect(productsToCsv([], 3)).toBe(PRODUCT_CSV_COLUMNS.join(","));
    expect(rowsToProducts(parseCSV(productsToCsv([], 3)))).toEqual([]);
  });

  it("quotes only what needs quoting", () => {
    expect(csvCell("plain")).toBe("plain");
    expect(csvCell("a,b")).toBe('"a,b"');
    expect(csvCell('say "hi"')).toBe('"say ""hi"""');
    expect(csvCell(null)).toBe("");
  });

  it("keeps the shipped template importable", () => {
    const rows = rowsToProducts(parseCSV(PRODUCT_TEMPLATE_CSV));
    expect(rows).toHaveLength(4);
    expect(rows[1].barcodes).toBe("1234567890|9876543210");
    expect(rows[3].tax_rule_name).toBeUndefined();
  });
});
