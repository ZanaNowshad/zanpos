import type { ProductWithPrice } from "../types";
import { DEVICE } from "../types";
import { formatMoney } from "../money";

interface Props {
  products: ProductWithPrice[];
  onSelect: (product: ProductWithPrice) => void;
  loading?: boolean;
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
        </button>
      ))}
    </div>
  );
}
