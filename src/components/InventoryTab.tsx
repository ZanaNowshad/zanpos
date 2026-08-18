import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { StockLevel, StockMovementRow } from "../types";
import * as cmd from "../tauri/commands";
import { useLanguage } from "../hooks/useLanguage";
import { backOfficeTranslator, inventoryMovementTypeText } from "../i18n/backOfficeStrings";
import { LoadingSkeleton, EmptyState } from "./templates";
import { Boxes } from "lucide-react";

interface Props {
  sessionUserId: string;
}

type Mode = "levels" | "receive" | "adjust" | "movements";

const PAGE_SIZE = 100;

export default function InventoryTab({ sessionUserId }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => backOfficeTranslator(language), [language]);
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
  const [recvExpiry, setRecvExpiry] = useState("");
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
  }, [sessionUserId]);

  // Reload when search or offset changes
  useEffect(() => {
    fetchPage(search, offset);
  }, [search, offset, fetchPage]);

  // ── Detail views (togglable — click again to collapse) ───────────────────

  const openReceive = (level: StockLevel) => {
    setSelected(level); setRecvQty(""); setRecvExpiry(""); setRecvNotes(""); setRecvError(null);
    setMode(mode === "receive" && selected?.product_id === level.product_id ? "levels" : "receive");
    setTimeout(() => recvRef.current?.focus(), 50);
  };
  const openAdjust = (level: StockLevel) => {
    setSelected(level); setAdjQty(level.quantity_on_hand); setAdjNotes(""); setAdjError(null);
    setMode(mode === "adjust" && selected?.product_id === level.product_id ? "levels" : "adjust");
    setTimeout(() => adjRef.current?.focus(), 50);
  };
  const openMovements = async (level: StockLevel) => {
    setSelected(level);
    setMode(mode === "movements" && selected?.product_id === level.product_id ? "levels" : "movements");
    if (mode !== "movements" || selected?.product_id !== level.product_id) {
      setMovLoading(true);
      try { setMovements(await cmd.inventoryGetMovements(sessionUserId, level.product_id)); }
      catch { setMovements([]); }
      finally { setMovLoading(false); }
    }
  };

  const handleReceive = async () => {
    if (!selected || !recvQty) return;
    setRecvLoading(true);
    setRecvError(null);
    try {
      const updated = await cmd.inventoryReceiveStock(
        selected.product_id, recvQty,
        recvExpiry || undefined,
        recvNotes || undefined, sessionUserId
      );
      setLevels(prev => prev.map(l => l.product_id === updated.product_id ? updated : l));
      setMode("levels");
    } catch (e: unknown) {
      setRecvError(typeof e === "string" ? e : t("failedReceiveStock"));
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
      setAdjError(typeof e === "string" ? e : t("failedAdjustStock"));
    } finally {
      setAdjLoading(false);
    }
  };

  // ── Inline expand panel (replaces old sub-view navigation) ──────────────
  function renderExpandPanel() {
    if (!selected || mode === "levels") return null;
    if (mode === "receive") return (
      <tr className="inv-expanded-row"><td colSpan={6}>
        <div className="inv-form-inline">
          <div className="inv-current">
            {t("currentStock")}: <strong>{parseFloat(selected.quantity_on_hand).toLocaleString()}</strong>
            {selected.is_low_stock && <span className="inv-badge-low"> ⚠ {t("lowStock")}</span>}
          </div>
          <label htmlFor="a11y-input-1" className="bo-label">{t("quantityReceived")} *</label>
          <input id="a11y-input-1" ref={recvRef} className="bo-input" type="number" step="0.001" min="0.001"
            placeholder="0.000" value={recvQty} onChange={e => setRecvQty(e.target.value)} />
          <label htmlFor="a11y-input-2" className="bo-label">{t("expiryDate")}</label>
          <input id="a11y-input-2" className="bo-input" type="date" value={recvExpiry}
            onChange={e => setRecvExpiry(e.target.value)} />
          <label htmlFor="a11y-input-3" className="bo-label">{t("notesPoSupplier")}</label>
          <input id="a11y-input-3" className="bo-input" type="text" placeholder={t("optional")}
            value={recvNotes} onChange={e => setRecvNotes(e.target.value)} />
          {recvError && <div className="modal-error">{recvError}</div>}
          <div className="inv-form-actions">
            <button className="modal-btn-secondary" onClick={() => setMode("levels")}>{t("cancel")}</button>
            <button className="modal-btn-primary" onClick={handleReceive}
              disabled={recvLoading || !recvQty || parseFloat(recvQty) <= 0}>{recvLoading ? t("saving") : t("receiveStock")}</button>
          </div>
        </div>
      </td></tr>);
    if (mode === "adjust") {
      const newQty = parseFloat(adjQty) || 0;
      const oldQty = parseFloat(selected.quantity_on_hand) || 0;
      const delta = newQty - oldQty;
      return (<tr className="inv-expanded-row"><td colSpan={6}>
        <div className="inv-form-inline">
          <div className="inv-current">{t("systemQuantity")}: <strong>{oldQty.toLocaleString()}</strong></div>
          <label htmlFor="a11y-input-4" className="bo-label">{t("actualCountedQuantity")} *</label>
          <input id="a11y-input-4" ref={adjRef} className="bo-input" type="number" step="0.001" min="0"
            value={adjQty} onChange={e => setAdjQty(e.target.value)} />
          {adjQty && !isNaN(delta) && (
            <div className={`inv-delta ${delta < 0 ? "inv-delta-neg" : delta > 0 ? "inv-delta-pos" : ""}`}>
              {delta === 0 ? t("noChange") : `${delta > 0 ? "+" : ""}${delta.toFixed(3)} ${t("variance")}`}
            </div>
          )}
          <label htmlFor="a11y-input-5" className="bo-label">{t("reasonForAdjustment")}</label>
          <input id="a11y-input-5" className="bo-input" type="text" placeholder={t("stockAdjustmentExample")}
            value={adjNotes} onChange={e => setAdjNotes(e.target.value)} />
          {adjError && <div className="modal-error">{adjError}</div>}
          <div className="inv-form-actions">
            <button className="modal-btn-secondary" onClick={() => setMode("levels")}>{t("cancel")}</button>
            <button className="modal-btn-primary" onClick={handleAdjust}
              disabled={adjLoading || !adjQty}>{adjLoading ? t("saving") : t("saveAdjustment")}</button>
          </div>
        </div>
      </td></tr>);
    }
    if (mode === "movements") return (
      <tr className="inv-expanded-row"><td colSpan={6}>
        <div className="inv-form-inline">
          <div className="inv-current">
            {t("currentStock")}: <strong>{parseFloat(selected.quantity_on_hand).toLocaleString()}</strong>
          </div>
          {movLoading ? <div className="bo-empty">{t("loading")}</div> :
           movements.length === 0 ? <div className="bo-empty">{t("noMovements")}</div> :
           <table className="rpt-table" style={{ marginTop: 12 }}><thead><tr>
             <th>{t("date")}</th><th>{t("type")}</th><th className="rpt-num">{t("delta")}</th><th className="rpt-num">{t("after")}</th><th>{t("notes")}</th>
           </tr></thead><tbody>
             {movements.map(m => (
               <tr key={m.movement_id}>
                 <td className="rpt-date">{new Date(m.created_at).toLocaleString([], {month:"short", day:"numeric", hour:"2-digit", minute:"2-digit"})}</td>
                 <td><span className={`inv-move-type inv-move-${m.movement_type}`}>{inventoryMovementTypeText(language, m.movement_type)}</span></td>
                 <td className={`rpt-num ${parseFloat(m.quantity_delta) < 0 ? "inv-neg" : "inv-pos"}`}>
                   {parseFloat(m.quantity_delta) > 0 ? "+" : ""}{parseFloat(m.quantity_delta).toLocaleString()}</td>
                 <td className="rpt-num">{parseFloat(m.quantity_after).toLocaleString()}</td>
                 <td className="rpt-dim">{m.notes ?? "—"}</td>
               </tr>
             ))}
           </tbody></table>}
          <div className="inv-form-actions">
            <button className="modal-btn-secondary" onClick={() => setMode("levels")}>{t("close")}</button>
          </div>
        </div>
      </td></tr>);
    return null;
  }

  // ── Main stock levels list ────────────────────────────────────────────────
  return (
    <div className="inv-layout">
      <div className="inv-toolbar">
        <input
          className="bo-input inv-search"
          type="text"
          placeholder={t("searchProducts")}
          value={searchInput}
          onChange={e => setSearchInput(e.target.value)}
        />
        <button className="btn-secondary" onClick={() => fetchPage(search, offset)} disabled={loading}>
          {loading ? "…" : `↺ ${t("refresh")}`}
        </button>
      </div>

      {/* Pagination bar */}
      {total > 0 && (
        <div className="bo-pagination">
          <span className="bo-pagination-info">
            {loading
              ? t("loading")
              : `${offset + 1}–${Math.min(offset + levels.length, total)} ${t("of")} ${total.toLocaleString()}`}
          </span>
          <button
            className="bo-pagination-btn"
            disabled={offset === 0 || loading}
            onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
          ><span className="icon-directional" aria-hidden="true">‹</span> {t("previous")}</button>
          <button
            className="bo-pagination-btn"
            disabled={offset + PAGE_SIZE >= total || loading}
            onClick={() => setOffset(offset + PAGE_SIZE)}
          >{t("next")} <span className="icon-directional" aria-hidden="true">›</span></button>
        </div>
      )}

      {loading && levels.length === 0 ? (
        <LoadingSkeleton variant="table" count={6} />
      ) : levels.length === 0 ? (
        <EmptyState
          icon={<Boxes size={36} strokeWidth={1.5} />}
          title={search ? t("noProductsMatch") : t("noTrackedProducts")}
          description={search ? undefined : "Add products with tracked stock to see inventory levels here."}
        />
      ) : (
        <table className="rpt-table inv-table">
          <thead>
            <tr>
              <th>{t("product")}</th>
              <th>{t("sku")}</th>
              <th className="rpt-num">{t("onHand")}</th>
              <th className="rpt-num">{t("reorderAt")}</th>
              <th>{t("status")}</th>
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
                    ? <span className="inv-badge inv-badge-oos">{t("outOfStock")}</span>
                    : l.is_low_stock
                      ? <span className="inv-badge inv-badge-low">⚠ {t("lowStock")}</span>
                      : <span className="inv-badge inv-badge-ok">{t("statusOk")}</span>
                  }
                </td>
                <td className="inv-actions">
                  <button className="inv-btn" onClick={() => openReceive(l)} title={t("receiveStock")}>+ {t("receive")}</button>
                  <button className="inv-btn inv-btn-adj" onClick={() => openAdjust(l)} title={t("countCorrection")}>⟳ {t("adjust")}</button>
                  <button className="inv-btn inv-btn-hist" onClick={() => openMovements(l)} title={t("viewHistory")}>{t("history")}</button>
                </td>
              </tr>
            ))}
            {renderExpandPanel()}
          </tbody>
        </table>
      )}
    </div>
  );
}
