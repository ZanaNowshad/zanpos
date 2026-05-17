import { convertFileSrc } from "@tauri-apps/api/core";
import type { ProductWithPrice } from "../types";
import { DEVICE } from "../types";
import { formatMoney } from "../money";

interface Props {
  products: ProductWithPrice[];
  onSelect: (product: ProductWithPrice) => void;
  loading?: boolean;
}

function stockBadge(p: ProductWithPrice) {
  if (!p.track_inventory || p.quantity_on_hand == null) return null;
  const qty = parseFloat(p.quantity_on_hand) || 0;
  if (qty <= 0) {
    return <span className="stock-badge stock-badge-out">Out of Stock</span>;
  }
  if (qty <= p.reorder_point) {
    return <span className="stock-badge stock-badge-low">Low · {p.quantity_on_hand}</span>;
  }
  return null;
}

function productImgSrc(imagePath: string | null): string | null {
  if (!imagePath) return null;
  try {
    return convertFileSrc(imagePath);
  } catch {
    return null;
  }
}

export default function ProductGrid({ products, onSelect, loading }: Props) {
  if (loading) return <div className="product-grid-msg">Loading products…</div>;
  if (!products.length) return <div className="product-grid-msg">No products found.</div>;

  return (
    <div className="product-grid">
      {products.map(p => {
        const imgSrc = productImgSrc(p.image_path ?? null);
        return (
          <button
            key={p.product_id}
            className="product-card"
            onClick={() => onSelect(p)}
          >
            {/* Image slot */}
            <div className="product-card-img-wrap">
              {imgSrc ? (
                <img
                  src={imgSrc}
                  alt={p.name}
                  className="product-card-img"
                  draggable={false}
                />
              ) : (
                <span className="product-card-img-placeholder">📦</span>
              )}
            </div>

            {/* Content */}
            <div className="product-card-body">
              <span className="product-card-name">{p.name}</span>
              <span className="product-card-price">
                {DEVICE.currency} {formatMoney(p.price_minor, DEVICE.currency_exponent)}
              </span>
              {p.sku && <span className="product-card-sku">{p.sku}</span>}
              {stockBadge(p)}
              <button className="product-card-star" onClick={e => e.stopPropagation()} tabIndex={-1}>☆</button>
            </div>
          </button>
        );
      })}
    </div>
  );
}
