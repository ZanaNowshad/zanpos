import { memo } from "react";
import { PackageSearch } from "lucide-react";
import type { ProductWithPrice } from "../types";
import { DEVICE } from "../types";
import { formatMoney } from "../money";
import { productIsLowStock, productIsOutOfStock } from "../posProductFilters";

interface Props {
  products: ProductWithPrice[];
  onSelect: (product: ProductWithPrice) => void;
  loading?: boolean;
  cartProductIds?: Set<string>;
}

function stockBadge(p: ProductWithPrice) {
  if (!p.track_inventory || p.quantity_on_hand == null) return null;
  if (productIsOutOfStock(p)) {
    return <span className="stock-badge stock-badge-out">Out of stock</span>;
  }
  if (productIsLowStock(p)) {
    return <span className="stock-badge stock-badge-low">Low stock: {p.quantity_on_hand}</span>;
  }
  return <span className="stock-badge stock-badge-in">In stock: {p.quantity_on_hand}</span>;
}

const ProductGrid = memo(function ProductGrid({ products, onSelect, loading, cartProductIds }: Props) {
  if (loading) {
    return (
      <div className="product-grid">
        {Array.from({ length: 12 }).map((_, i) => (
          <div key={i} className="product-card-skeleton">
            <div className="product-card-skeleton-img skeleton" />
            <div className="product-card-skeleton-body">
              <div className="skeleton skeleton-text" />
              <div className="skeleton skeleton-text-sm" />
            </div>
          </div>
        ))}
      </div>
    );
  }
  if (!products.length) return (
    <div className="product-grid-msg" role="status" aria-live="polite">
      <PackageSearch size={48} />
      <p className="product-grid-empty-heading">Ready to start selling?</p>
      <p className="product-grid-empty-hint">
        Ask your manager to add products in Back Office → Products.
      </p>
    </div>
  );

  return (
    <div className="product-grid">
      {products.map(p => {
        const isOos = productIsOutOfStock(p);
        const inCart = cartProductIds?.has(p.product_id) ?? false;
        return (
          <button
            key={p.product_id}
            className={`product-card${isOos ? " product-card-oos" : ""}${inCart ? " in-cart" : ""}`}
            onClick={() => !isOos && onSelect(p)}
            disabled={isOos}
            aria-disabled={isOos}
          >
            <div className="product-card-body">
              <span className="product-card-name">{p.name}</span>
              <span className="product-card-price">
                {DEVICE.currency} {formatMoney(p.price_minor, DEVICE.currency_exponent)}
              </span>
              <span className="product-card-sku">{p.sku || p.barcode || "No SKU"}</span>
              {stockBadge(p)}
            </div>
          </button>
        );
      })}
    </div>
  );
});

export default ProductGrid;
