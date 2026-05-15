import { useEffect, useRef, useState } from "react";
import { DEVICE } from "../types";
import { formatMoney } from "../money";

interface Props {
  onAdd: (name: string, priceMajor: string, quantity: string) => Promise<void>;
  onCancel: () => void;
}

export default function CustomItemModal({ onAdd, onCancel }: Props) {
  const [name, setName] = useState("");
  const [price, setPrice] = useState("");
  const [qty, setQty] = useState("1");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const nameRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    nameRef.current?.focus();
  }, []);

  const priceNum = parseFloat(price);
  const qtyNum = parseFloat(qty);
  const previewMinor = !isNaN(priceNum) && priceNum > 0 ? Math.round(priceNum * Math.pow(10, DEVICE.currency_exponent)) : 0;
  const lineTotal = !isNaN(qtyNum) && qtyNum > 0 ? previewMinor * qtyNum : 0;

  const handleAdd = async () => {
    setError(null);
    if (!name.trim()) { setError("Item description is required"); return; }
    if (isNaN(priceNum) || priceNum <= 0) { setError("Enter a valid price"); return; }
    if (isNaN(qtyNum) || qtyNum <= 0) { setError("Enter a valid quantity"); return; }
    setLoading(true);
    try {
      await onAdd(name.trim(), price, qty);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to add item");
    } finally {
      setLoading(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") handleAdd();
    if (e.key === "Escape") onCancel();
  };

  return (
    <div className="modal-overlay" onKeyDown={handleKeyDown}>
      <div className="modal custom-item-modal">
        <h2 className="modal-title">Custom Item</h2>
        <p className="modal-subtitle">Add a one-off item that isn't in the product list.</p>

        <label className="field-label">Description *</label>
        <input
          ref={nameRef}
          className="field-input"
          type="text"
          placeholder="e.g. Gift wrap, Labour charge…"
          value={name}
          onChange={e => setName(e.target.value)}
          maxLength={80}
        />

        <div className="custom-item-row">
          <div className="custom-item-col">
            <label className="field-label">Price ({DEVICE.currency})</label>
            <input
              className="field-input"
              type="number"
              step="0.001"
              min="0"
              placeholder="0.000"
              value={price}
              onChange={e => setPrice(e.target.value)}
            />
          </div>
          <div className="custom-item-col">
            <label className="field-label">Quantity</label>
            <input
              className="field-input"
              type="number"
              step="0.001"
              min="0.001"
              placeholder="1"
              value={qty}
              onChange={e => setQty(e.target.value)}
            />
          </div>
        </div>

        {previewMinor > 0 && (
          <div className="custom-item-preview">
            {!isNaN(qtyNum) && qtyNum !== 1
              ? `${formatMoney(previewMinor, DEVICE.currency_exponent)} × ${qty} = ${DEVICE.currency} ${formatMoney(lineTotal, DEVICE.currency_exponent)}`
              : `${DEVICE.currency} ${formatMoney(previewMinor, DEVICE.currency_exponent)}`
            }
          </div>
        )}

        {error && <div className="modal-error">{error}</div>}

        <div className="modal-actions">
          <button className="modal-btn-secondary" onClick={onCancel} disabled={loading}>
            Cancel
          </button>
          <button className="modal-btn-primary" onClick={handleAdd} disabled={loading || !name.trim() || priceNum <= 0}>
            {loading ? "Adding…" : "Add to Cart"}
          </button>
        </div>
      </div>
    </div>
  );
}
