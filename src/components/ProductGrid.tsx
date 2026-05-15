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
    return <span className="stock-badge stock-badge-out">OUT</span>;
  }
  if (qty <= p.reorder_point) {
    return <span className="stock-badge stock-badge-low">LOW {p.quantity_on_hand}</span>;
  }
  return null;
}

export default function ProductGrid({ products, onSelect, loading }: Props) {
  if (loading) return <div className="product-grid-msg">Loading products…</div>;
  if (!products.length) return <div className="product-grid-msg">No products found.</div>;

  return (
    <div className="product-grid">
      {products.map(p => (
        <button
          key={p.product_id}
          className="product-card"
          onClick={() => onSelect(p)}
        >
          <span className="product-card-name">{p.name}</span>
          <span className="product-card-price">
            {DEVICE.currency} {formatMoney(p.price_minor, DEVICE.currency_exponent)}
          </span>
          {p.sku && <span className="product-card-sku">{p.sku}</span>}
          {stockBadge(p)}
        </button>
      ))}
    </div>
  );
}
