import { useCallback, useEffect, useRef, useState } from "react";
import type { StockLevel, StockMovementRow } from "../types";
import * as cmd from "../tauri/commands";

interface Props {
  sessionUserId: string;
}

type Mode = "levels" | "receive" | "adjust" | "movements";

const PAGE_SIZE = 100;

export default function InventoryTab({ sessionUserId }: Props) {
  // ── Paginated list state ──────────────────────────────────────────────────
  const [levels, setLevels]           = useState<StockLevel[]>([]);
  const [total, setTotal]             = useState(0);
  const [offset, setOffset]           = useState(0);
  const [loading, setLoading]         = useState(true);
  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch]           = useState("");

  const [mode, setMode]               = useState<Mode>("levels");
  const [selected, setSelected]       = useState<StockLevel | null>(null);
  const [movements, setMovements]     = useState<StockMovementRow[]>([]);
  const [movLoading, setMovLoading]   = useState(false);

  // Receive stock form
  const [recvQty, setRecvQty]       = useState("");
  const [recvNotes, setRecvNotes]   = useState("");
  const [recvLoading, setRecvLoading] = useState(false);
  const [recvError, setRecvError]   = useState<string | null>(null);

  // Adjust stock form
  const [adjQty, setAdjQty]         = useState("");
  const [adjNotes, setAdjNotes]     = useState("");
  const [adjLoading, setAdjLoading] = useState(false);
  const [adjError, setAdjError]     = useState<string | null>(null);

  const recvRef = useRef<HTMLInputElement>(null);
  const adjRef  = useRef<HTMLInputElement>(null);

  // Debounce search input → `search` (300 ms), reset to page 0
  useEffect(() => {
    const t = setTimeout(() => { setSearch(searchInput); setOffset(0); }, 300);
    return () => clearTimeout(t);
  }, [searchInput]);

  const fetchPage = useCallback(async (q: string, off: number) => {
    setLoading(true);
    try {
      const page = await cmd.inventoryGetLevelsPaged(sessionUserId, q, off, PAGE_SIZE);
      setLevels(page.items);
      setTotal(page.total);
      setOffset(off);
    } finally {
      setLoading(false);
    }
  }, []);

  // Reload when search or offset changes
  useEffect(() => {
    fetchPage(search, offset);
  }, [search, offset]); // eslint-disable-line react-hooks/exhaustive-deps

  // ── Detail views ─────────────────────────────────────────────────────────

  const openMovements = async (level: StockLevel) => {
    setSelected(level);
    setMode("movements");
    setMovLoading(true);
    try {
      const m = await cmd.inventoryGetMovements(sessionUserId, level.product_id);
      setMovements(m);
    } finally {
      setMovLoading(false);
    }
  };

  const openReceive = (level: StockLevel) => {
    setSelected(level);
    setRecvQty("");
    setRecvNotes("");
    setRecvError(null);
    setMode("receive");
    setTimeout(() => recvRef.current?.focus(), 50);
  };

  const openAdjust = (level: StockLevel) => {
    setSelected(level);
    setAdjQty(parseFloat(level.quantity_on_hand).toFixed(3));
    setAdjNotes("");
    setAdjError(null);
    setMode("adjust");
    setTimeout(() => adjRef.current?.focus(), 50);
  };

  const handleReceive = async () => {
    if (!selected || !recvQty) return;
    setRecvLoading(true);
    setRecvError(null);
    try {
      const updated = await cmd.inventoryReceiveStock(
        selected.product_id, recvQty,
        recvNotes || undefined, sessionUserId
      );
      setLevels(prev => prev.map(l => l.product_id === updated.product_id ? updated : l));
      setMode("levels");
    } catch (e: unknown) {
      setRecvError(typeof e === "string" ? e : "Failed to receive stock");
    } finally {
      setRecvLoading(false);
    }
  };

  const handleAdjust = async () => {
    if (!selected || !adjQty) return;
    setAdjLoading(true);
    setAdjError(null);
    try {
      const updated = await cmd.inventoryAdjustStock(
        selected.product_id, adjQty,
        adjNotes || undefined, sessionUserId
      );
      setLevels(prev => prev.map(l => l.product_id === updated.product_id ? updated : l));
      setMode("levels");
    } catch (e: unknown) {
      setAdjError(typeof e === "string" ? e : "Failed to adjust stock");
    } finally {
      setAdjLoading(false);
    }
  };

  // ── Sub-views ─────────────────────────────────────────────────────────────

  if (mode === "receive" && selected) {
    return (
      <div className="inv-form-pane">
        <button className="bo-back-btn" onClick={() => setMode("levels")}>← Back to Inventory</button>
        <h3 className="inv-form-title">Receive Stock — {selected.product_name}</h3>
        <div className="inv-current">
          Current stock: <strong>{parseFloat(selected.quantity_on_hand).toLocaleString()}</strong>
          {selected.is_low_stock && <span className="inv-badge-low"> ⚠ Low</span>}
        </div>
        <label className="bo-label">Quantity received *</label>
        <input
          ref={recvRef}
          className="bo-input"
          type="number"
          step="0.001"
          min="0.001"
          placeholder="0.000"
          value={recvQty}
          onChange={e => setRecvQty(e.target.value)}
        />
        <label className="bo-label">Notes (PO number, supplier, etc.)</label>
        <input
          className="bo-input"
          type="text"
          placeholder="Optional"
          value={recvNotes}
          onChange={e => setRecvNotes(e.target.value)}
        />
        {recvError && <div className="modal-error">{recvError}</div>}
        <div className="modal-actions" style={{ marginTop: 16 }}>
          <button className="modal-btn-secondary" onClick={() => setMode("levels")}>Cancel</button>
          <button
            className="modal-btn-primary"
            onClick={handleReceive}
            disabled={recvLoading || !recvQty || parseFloat(recvQty) <= 0}
          >
            {recvLoading ? "Saving…" : "Receive Stock"}
          </button>
        </div>
      </div>
    );
  }

  if (mode === "adjust" && selected) {
    const newQty = parseFloat(adjQty) || 0;
    const oldQty = parseFloat(selected.quantity_on_hand) || 0;
    const delta  = newQty - oldQty;
    return (
      <div className="inv-form-pane">
        <button className="bo-back-btn" onClick={() => setMode("levels")}>← Back to Inventory</button>
        <h3 className="inv-form-title">Count Correction — {selected.product_name}</h3>
        <div className="inv-current">
          System quantity: <strong>{oldQty.toLocaleString()}</strong>
        </div>
        <label className="bo-label">Actual counted quantity *</label>
        <input
          ref={adjRef}
          className="bo-input"
          type="number"
          step="0.001"
          min="0"
          value={adjQty}
          onChange={e => setAdjQty(e.target.value)}
        />
        {adjQty && !isNaN(delta) && (
          <div className={`inv-delta ${delta < 0 ? "inv-delta-neg" : delta > 0 ? "inv-delta-pos" : ""}`}>
            {delta === 0 ? "No change" : `${delta > 0 ? "+" : ""}${delta.toFixed(3)} variance`}
          </div>
        )}
        <label className="bo-label">Reason for adjustment</label>
        <input
          className="bo-input"
          type="text"
          placeholder="e.g. Stocktake, damaged goods…"
          value={adjNotes}
          onChange={e => setAdjNotes(e.target.value)}
        />
        {adjError && <div className="modal-error">{adjError}</div>}
        <div className="modal-actions" style={{ marginTop: 16 }}>
          <button className="modal-btn-secondary" onClick={() => setMode("levels")}>Cancel</button>
          <button
            className="modal-btn-primary"
            onClick={handleAdjust}
            disabled={adjLoading || !adjQty}
          >
            {adjLoading ? "Saving…" : "Save Adjustment"}
          </button>
        </div>
      </div>
    );
  }

  if (mode === "movements" && selected) {
    return (
      <div className="inv-form-pane">
        <button className="bo-back-btn" onClick={() => setMode("levels")}>← Back to Inventory</button>
        <h3 className="inv-form-title">Movement History — {selected.product_name}</h3>
        <div className="inv-current">
          Current stock: <strong>{parseFloat(selected.quantity_on_hand).toLocaleString()}</strong>
        </div>
        {movLoading ? (
          <div className="bo-empty">Loading…</div>
        ) : movements.length === 0 ? (
          <div className="bo-empty">No movements recorded yet.</div>
        ) : (
          <table className="rpt-table" style={{ marginTop: 12 }}>
            <thead>
              <tr>
                <th>Date</th>
                <th>Type</th>
                <th className="rpt-num">Delta</th>
                <th className="rpt-num">After</th>
                <th>Notes</th>
              </tr>
            </thead>
            <tbody>
              {movements.map(m => (
                <tr key={m.movement_id}>
                  <td className="rpt-date">{new Date(m.created_at).toLocaleString([], {
                    month: "short", day: "numeric", hour: "2-digit", minute: "2-digit"
                  })}</td>
                  <td><span className={`inv-move-type inv-move-${m.movement_type}`}>{m.movement_type}</span></td>
                  <td className={`rpt-num ${parseFloat(m.quantity_delta) < 0 ? "inv-neg" : "inv-pos"}`}>
                    {parseFloat(m.quantity_delta) > 0 ? "+" : ""}{parseFloat(m.quantity_delta).toLocaleString()}
                  </td>
                  <td className="rpt-num">{parseFloat(m.quantity_after).toLocaleString()}</td>
                  <td className="rpt-dim">{m.notes ?? "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    );
  }

  // ── Main stock levels list ────────────────────────────────────────────────
  return (
    <div className="inv-layout">
      <div className="inv-toolbar">
        <input
          className="bo-input inv-search"
          type="text"
          placeholder="Search products…"
          value={searchInput}
          onChange={e => setSearchInput(e.target.value)}
        />
        <button className="btn-secondary" onClick={() => fetchPage(search, offset)} disabled={loading}>
          {loading ? "…" : "↺ Refresh"}
        </button>
      </div>

      {/* Pagination bar */}
      {total > 0 && (
        <div className="bo-pagination">
          <span className="bo-pagination-info">
            {loading
              ? "Loading…"
              : `${offset + 1}–${Math.min(offset + levels.length, total)} of ${total.toLocaleString()}`}
          </span>
          <button
            className="bo-pagination-btn"
            disabled={offset === 0 || loading}
            onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
          >‹ Prev</button>
          <button
            className="bo-pagination-btn"
            disabled={offset + PAGE_SIZE >= total || loading}
            onClick={() => setOffset(offset + PAGE_SIZE)}
          >Next ›</button>
        </div>
      )}

      {loading && levels.length === 0 ? (
        <div className="bo-empty">Loading inventory…</div>
      ) : levels.length === 0 ? (
        <div className="bo-empty">
          {search ? "No products match your search." : "No tracked products found."}
        </div>
      ) : (
        <table className="rpt-table inv-table">
          <thead>
            <tr>
              <th>Product</th>
              <th>SKU</th>
              <th className="rpt-num">On Hand</th>
              <th className="rpt-num">Reorder At</th>
              <th>Status</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {levels.map(l => (
              <tr key={l.product_id} className={l.is_out_of_stock ? "inv-row-oos" : l.is_low_stock ? "inv-row-low" : ""}>
                <td className="inv-name">{l.product_name}</td>
                <td className="rpt-dim">{l.sku ?? "—"}</td>
                <td className="rpt-num inv-qty">{parseFloat(l.quantity_on_hand).toLocaleString()}</td>
                <td className="rpt-num rpt-dim">{l.reorder_point}</td>
                <td>
                  {l.is_out_of_stock
                    ? <span className="inv-badge inv-badge-oos">Out of Stock</span>
                    : l.is_low_stock
                      ? <span className="inv-badge inv-badge-low">⚠ Low</span>
                      : <span className="inv-badge inv-badge-ok">OK</span>
                  }
                </td>
                <td className="inv-actions">
                  <button className="inv-btn" onClick={() => openReceive(l)} title="Receive stock">+ Receive</button>
                  <button className="inv-btn inv-btn-adj" onClick={() => openAdjust(l)} title="Count correction">⟳ Adjust</button>
                  <button className="inv-btn inv-btn-hist" onClick={() => openMovements(l)} title="View history">History</button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
