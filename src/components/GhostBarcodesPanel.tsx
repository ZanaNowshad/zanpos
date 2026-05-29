import { useState, useEffect, useCallback } from "react";
import type { GhostBarcode, GhostSummary, ProductPrefill } from "../types";
import {
  ghostList,
  ghostResolve,
  ghostDismiss,
  ghostPrefill,
} from "../tauri/commands";

interface Props {
  sessionUserId: string;
  summary: GhostSummary;
  /** Called after "Create Product" to pre-fill the product form */
  onCreateProduct: (prefill: ProductPrefill) => void;
  /** Called after any action that changes the count (dismiss/resolve) */
  onCountChange: () => void;
}

export default function GhostBarcodesPanel({
  sessionUserId,
  summary,
  onCreateProduct,
  onCountChange,
}: Props) {
  const [expanded, setExpanded]   = useState(false);
  const [items, setItems]         = useState<GhostBarcode[]>([]);
  const [loading, setLoading]     = useState(false);
  const [resolving, setResolving] = useState(false);
  const [error, setError]         = useState<string | null>(null);

  const total = summary.pending + summary.found + summary.not_found;

  // Load items whenever the panel is expanded
  const loadItems = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const rows = await ghostList(sessionUserId);
      setItems(rows);
    } catch (e) {
      setError(typeof e === "string" ? e : "Failed to load ghost barcodes");
    } finally {
      setLoading(false);
    }
  }, [sessionUserId]);

  useEffect(() => {
    if (expanded) loadItems();
  }, [expanded, loadItems]);

  const handleResolve = async () => {
    setResolving(true);
    setError(null);
    try {
      await ghostResolve(sessionUserId);
      await loadItems();
      onCountChange();
    } catch (e) {
      setError(typeof e === "string" ? e : "Lookup failed");
    } finally {
      setResolving(false);
    }
  };

  const handleDismiss = async (id: string) => {
    try {
      await ghostDismiss(id, sessionUserId);
      setItems(prev => prev.filter(i => i.id !== id));
      onCountChange();
    } catch (e) {
      setError(typeof e === "string" ? e : "Dismiss failed");
    }
  };

  const handleCreateProduct = async (id: string) => {
    try {
      const prefill = await ghostPrefill(id, sessionUserId);
      await ghostDismiss(id, sessionUserId);
      setItems(prev => prev.filter(i => i.id !== id));
      onCountChange();
      onCreateProduct(prefill);
    } catch (e) {
      setError(typeof e === "string" ? e : "Failed to get product data");
    }
  };

  if (total === 0) return null;

  return (
    <div className="ghost-panel">
      <button
        className="ghost-panel-header"
        onClick={() => setExpanded(e => !e)}
      >
        <span className="ghost-panel-badge">{total}</span>
        <span className="ghost-panel-title">
          Unrecognized barcodes
          {summary.pending > 0 && ` · ${summary.pending} pending lookup`}
          {summary.found   > 0 && ` · ${summary.found} ready to create`}
        </span>
        <span className="ghost-panel-chevron">{expanded ? "▲" : "▼"}</span>
      </button>

      {expanded && (
        <div className="ghost-panel-body">
          {error && <div className="ghost-error">{error}</div>}

          {summary.pending > 0 && (
            <div className="ghost-actions-row">
              <button
                className="ghost-lookup-btn"
                onClick={handleResolve}
                disabled={resolving}
              >
                {resolving ? "Looking up…" : `Look up ${summary.pending} barcode${summary.pending !== 1 ? "s" : ""} now`}
              </button>
            </div>
          )}

          {loading && <div className="ghost-loading">Loading…</div>}

          {!loading && items.length > 0 && (
            <div className="ghost-list">
              {items.map(item => (
                <div
                  key={item.id}
                  className={`ghost-item ghost-item-${item.status}`}
                >
                  <div className="ghost-item-left">
                    {item.image_url && (
                      <img
                        src={item.image_url}
                        alt={item.product_name ?? ""}
                        className="ghost-item-img"
                        onError={e => { (e.target as HTMLImageElement).style.display = "none"; }}
                      />
                    )}
                    <div className="ghost-item-info">
                      {item.status === "found" ? (
                        <>
                          <div className="ghost-item-name">{item.product_name}</div>
                          {item.brand    && <div className="ghost-item-meta">{item.brand}</div>}
                          {item.category && <div className="ghost-item-meta">{item.category}</div>}
                        </>
                      ) : item.status === "not_found" ? (
                        <div className="ghost-item-name ghost-not-found">
                          Not found in any database
                        </div>
                      ) : (
                        <div className="ghost-item-name ghost-pending">
                          Pending lookup…
                        </div>
                      )}
                      <div className="ghost-item-barcode">
                        #{item.barcode}
                        <span className="ghost-item-count">
                          · scanned {item.scan_count}×
                        </span>
                      </div>
                    </div>
                  </div>

                  <div className="ghost-item-right">
                    {item.status === "found" && (
                      <button
                        className="ghost-btn ghost-btn-create"
                        onClick={() => handleCreateProduct(item.id)}
                      >
                        + Create Product
                      </button>
                    )}
                    <button
                      className="ghost-btn ghost-btn-dismiss"
                      onClick={() => handleDismiss(item.id)}
                    >
                      Dismiss
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}

          {!loading && items.length === 0 && (
            <div className="ghost-empty">No unrecognized barcodes to show.</div>
          )}
        </div>
      )}
    </div>
  );
}
