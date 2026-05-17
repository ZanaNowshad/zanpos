import { useEffect, useState } from "react";
import type { ProductWithPrice, SessionUser } from "../types";
import { productListAll, inventoryBulkStockTake } from "../tauri/commands";

interface Props {
  user: SessionUser;
  onClose: () => void;
}

interface Row {
  product_id: string;
  name: string;
  current_qty: string | null;
  counted: string;
}

export default function BulkStockTakeModal({ user, onClose }: Props) {
  const [rows, setRows] = useState<Row[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<{ updated: number; errors: string[] } | null>(null);
  const [search, setSearch] = useState("");

  useEffect(() => {
    productListAll()
      .then((products: ProductWithPrice[]) => {
        const tracked = products.filter(p => p.track_inventory);
        setRows(tracked.map(p => ({
          product_id: p.product_id,
          name: p.name,
          current_qty: p.quantity_on_hand,
          counted: "",
        })));
      })
      .catch(() => setError("Failed to load products"))
      .finally(() => setLoading(false));
  }, []);

  const handleSubmit = async () => {
    const entries = rows
      .filter(r => r.counted.trim() !== "")
      .map(r => ({
        product_id: r.product_id,
        new_quantity: parseFloat(r.counted),
        notes: "Bulk stock-take",
      }))
      .filter(e => !isNaN(e.new_quantity));

    if (entries.length === 0) {
      setError("Enter at least one counted quantity to submit.");
      return;
    }

    setSaving(true);
    setError(null);
    try {
      const result = await inventoryBulkStockTake(entries, user.user_id);
      setDone(result);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to save stock-take");
    } finally {
      setSaving(false);
    }
  };

  const filtered = rows.filter(r =>
    r.name.toLowerCase().includes(search.toLowerCase())
  );

  const filledCount = rows.filter(r => r.counted.trim() !== "").length;

  return (
    <div className="modal-overlay">
      <div className="modal bulk-stocktake-modal">
        <h2 className="modal-title">Bulk Stock-Take</h2>

        {done ? (
          <div className="stocktake-done">
            <div className="stocktake-done-icon">✓</div>
            <div className="stocktake-done-msg">
              Updated <strong>{done.updated}</strong> product{done.updated !== 1 ? "s" : ""}.
            </div>
            {done.errors.length > 0 && (
              <div className="stocktake-errors">
                {done.errors.map((e, i) => <div key={i} className="stocktake-error-row">{e}</div>)}
              </div>
            )}
            <button className="modal-btn-primary" onClick={onClose}>Done</button>
          </div>
        ) : loading ? (
          <div className="stocktake-loading">Loading products…</div>
        ) : (
          <>
            <div className="stocktake-toolbar">
              <input
                className="field-input stocktake-search"
                type="text"
                placeholder="Search products…"
                value={search}
                onChange={e => setSearch(e.target.value)}
              />
              <span className="stocktake-count">
                {filledCount} / {rows.length} entered
              </span>
            </div>

            <div className="stocktake-table-wrapper">
              <table className="stocktake-table">
                <thead>
                  <tr>
                    <th>Product</th>
                    <th>Current Qty</th>
                    <th>Counted Qty</th>
                  </tr>
                </thead>
                <tbody>
                  {filtered.map(r => (
                    <tr key={r.product_id} className={r.counted.trim() ? "stocktake-row-filled" : ""}>
                      <td className="stocktake-name">{r.name}</td>
                      <td className="stocktake-current">{r.current_qty ?? "—"}</td>
                      <td>
                        <input
                          className="stocktake-qty-input"
                          type="number"
                          min="0"
                          step="0.001"
                          placeholder="—"
                          value={r.counted}
                          onChange={e =>
                            setRows(prev =>
                              prev.map(row =>
                                row.product_id === r.product_id
                                  ? { ...row, counted: e.target.value }
                                  : row
                              )
                            )
                          }
                        />
                      </td>
                    </tr>
                  ))}
                  {filtered.length === 0 && (
                    <tr>
                      <td colSpan={3} className="stocktake-empty">No products match your search.</td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>

            {error && <div className="modal-error">{error}</div>}

            <div className="modal-actions">
              <button className="modal-btn-secondary" onClick={onClose} disabled={saving}>
                Cancel
              </button>
              <button
                className="modal-btn-primary"
                onClick={handleSubmit}
                disabled={saving || filledCount === 0}
              >
                {saving ? "Saving…" : `Submit ${filledCount > 0 ? `(${filledCount})` : ""}`}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
