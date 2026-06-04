import { describe, expect, it } from "vitest";
import type { ProductWithPrice } from "../types";
import { filterProductsForSale, productIsLowStock, productIsOutOfStock } from "../posProductFilters";

function product(overrides: Partial<ProductWithPrice>): ProductWithPrice {
  return {
    product_id: "p",
    category_id: "cat",
    name: "Product",
    sku: null,
    barcode: null,
    description: null,
    track_inventory: true,
    allow_decimal_quantity: false,
    is_active: true,
    tax_rule_id: null,
    cost_minor: null,
    currency: "BHD",
    version: 1,
    reorder_point: 5,
    image_path: null,
    price_minor: 100,
    tax_rate_basis_points: 1000,
    tax_inclusive: true,
    category_name: "Category",
    quantity_on_hand: "10",
    ...overrides,
  };
}

describe("product sale filters", () => {
  it("hides unavailable products by default", () => {
    const visible = filterProductsForSale([
      product({ product_id: "stocked", name: "Cola", quantity_on_hand: "12" }),
      product({ product_id: "empty", name: "Water", quantity_on_hand: "0" }),
      product({ product_id: "disabled", name: "Disabled", is_active: false }),
    ], {
      selectedCategory: null,
      searchQuery: "",
      showUnavailable: false,
    });

    expect(visible.map(p => p.product_id)).toEqual(["stocked"]);
  });

  it("shows unavailable products after sellable products when toggled on", () => {
    const visible = filterProductsForSale([
      product({ product_id: "empty", name: "Water", quantity_on_hand: "0" }),
      product({ product_id: "stocked", name: "Cola", quantity_on_hand: "12" }),
      product({ product_id: "low", name: "Biscuits", quantity_on_hand: "2" }),
    ], {
      selectedCategory: null,
      searchQuery: "",
      showUnavailable: true,
    });

    expect(visible.map(p => p.product_id)).toEqual(["low", "stocked", "empty"]);
  });

  it("matches search and category while keeping disabled products hidden", () => {
    const visible = filterProductsForSale([
      product({ product_id: "cola", category_id: "drinks", name: "Coca-Cola", sku: "COLA-330" }),
      product({ product_id: "chips", category_id: "snacks", name: "Chips", barcode: "123456" }),
      product({ product_id: "disabled", category_id: "drinks", name: "Cola Disabled", is_active: false }),
    ], {
      selectedCategory: "drinks",
      searchQuery: "cola",
      showUnavailable: true,
    });

    expect(visible.map(p => p.product_id)).toEqual(["cola"]);
  });

  it("classifies stock states for POS badges", () => {
    expect(productIsOutOfStock(product({ quantity_on_hand: "0" }))).toBe(true);
    expect(productIsLowStock(product({ quantity_on_hand: "3", reorder_point: 5 }))).toBe(true);
    expect(productIsLowStock(product({ quantity_on_hand: "6", reorder_point: 5 }))).toBe(false);
  });
});
