import type { ProductWithPrice } from "./types";

export type ProductGridFilters = {
  selectedCategory: string | null;
  searchQuery: string;
  showUnavailable: boolean;
};

export function productIsOutOfStock(product: ProductWithPrice): boolean {
  if (!product.track_inventory || product.quantity_on_hand == null) return false;
  return (parseFloat(product.quantity_on_hand) || 0) <= 0;
}

export function productIsLowStock(product: ProductWithPrice): boolean {
  if (!product.track_inventory || product.quantity_on_hand == null) return false;
  const qty = parseFloat(product.quantity_on_hand) || 0;
  return qty > 0 && qty <= product.reorder_point;
}

export function filterProductsForSale(
  products: ProductWithPrice[],
  filters: ProductGridFilters,
): ProductWithPrice[] {
  const q = filters.searchQuery.trim().toLowerCase();

  return products
    .filter(product => {
      if (!product.is_active) return false;
      if (!filters.showUnavailable && productIsOutOfStock(product)) return false;
      if (filters.selectedCategory && product.category_id !== filters.selectedCategory) return false;
      if (!q) return true;

      return product.name.toLowerCase().includes(q) ||
        product.sku?.toLowerCase().includes(q) ||
        product.barcode?.includes(q);
    })
    .sort((a, b) => {
      const aOut = productIsOutOfStock(a);
      const bOut = productIsOutOfStock(b);
      if (aOut !== bOut) return aOut ? 1 : -1;
      return a.name.localeCompare(b.name);
    });
}
