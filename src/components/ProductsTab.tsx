import { useCallback, useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { AdminProduct, CategoryRow, GhostSummary, ProductBarcodeRow, ProductPrefill, TaxRuleRow } from "../types";
import { DEVICE } from "../types";
import { formatMoney, parseMoney } from "../money";
import * as cmd from "../tauri/commands";
import BarcodesPrintModal from "./BarcodesPrintModal";
import BulkImportModal from "./BulkImportModal";
import GhostBarcodesPanel from "./GhostBarcodesPanel";

interface Props {
  sessionUserId: string;
  ghostSummary?: GhostSummary;
  ghostPrefill?: ProductPrefill | null;
  onGhostPrefillConsumed?: () => void;
  onGhostCountChange?: () => void;
  onCreateProductFromGhost?: (prefill: ProductPrefill) => void;
}

const EMPTY_FORM = {
  name: "", category_id: "", sku: "", barcode: "",
  tax_rule_id: "", price: "", track_inventory: true,
  allow_decimal_quantity: false, reorder_point: 0, is_active: true,
  image_path: "" as string,
};

// Pending barcodes for new products (stored locally, inserted after create)
type PendingBarcode = { tempId: string; barcode: string };

const PAGE_SIZE = 100;

export default function ProductsTab({
  sessionUserId,
  ghostSummary,
  ghostPrefill,
  onGhostPrefillConsumed,
  onGhostCountChange,
  onCreateProductFromGhost,
}: Props) {
  const [products, setProducts]       = useState<AdminProduct[]>([]);
  const [total, setTotal]             = useState(0);
  const [offset, setOffset]           = useState(0);
  const [loading, setLoading]         = useState(false);
  const [categories, setCategories]   = useState<CategoryRow[]>([]);
  const [taxRules, setTaxRules]       = useState<TaxRuleRow[]>([]);
  const [selected, setSelected]       = useState<AdminProduct | null>(null);
  const [creating, setCreating]       = useState(false);
  const [form, setForm]               = useState(EMPTY_FORM);
  const [saving, setSaving]           = useState(false);
  const [error, setError]             = useState<string | null>(null);
  const [search, setSearch]           = useState("");
  const [searchInput, setSearchInput] = useState(""); // debounced into `search`
  const [printProducts, setPrintProducts] = useState<AdminProduct[] | null>(null);
  const [showBulkImport, setShowBulkImport] = useState(false);

  // Cost/markup helpers (UI-only — not persisted to backend)
  const [costPrice, setCostPrice]     = useState("");
  const [markupPct, setMarkupPct]     = useState("");

  // Additional barcodes
  const [extraBarcodes, setExtraBarcodes] = useState<ProductBarcodeRow[]>([]); // for editing existing
  const [pendingBarcodes, setPendingBarcodes] = useState<PendingBarcode[]>([]); // for new product
  const [newBarcodeInput, setNewBarcodeInput] = useState("");
  const [barcodeError, setBarcodeError] = useState<string | null>(null);
  const newBarcodeRef = useRef<HTMLInputElement>(null);

  const exp = DEVICE.currency_exponent;
  const cur = DEVICE.currency;

  // Debounce search input → `search` state (300 ms)
  useEffect(() => {
    const t = setTimeout(() => { setSearch(searchInput); setOffset(0); }, 300);
    return () => clearTimeout(t);
  }, [searchInput]);

  const fetchProducts = useCallback(async (q: string, off: number) => {
    setLoading(true);
    try {
      const page = await cmd.adminListProducts({ search: q, offset: off, limit: PAGE_SIZE });
      setProducts(page.items);
      setTotal(page.total);
      setOffset(off);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchProducts(search, offset);
  }, [search, offset]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    Promise.all([cmd.adminListCategories(), cmd.adminListTaxRules()])
      .then(([cats, taxes]) => {
        setCategories(cats.filter(c => c.is_active));
        setTaxRules(taxes);
      });
  }, []);

  // Pre-fill product form when a ghost barcode "Create Product" is clicked
  useEffect(() => {
    if (!ghostPrefill) return;
    setCreating(true);
    setForm(prev => ({
      ...prev,
      name:    ghostPrefill.name,
      barcode: ghostPrefill.barcode,
    }));
    onGhostPrefillConsumed?.();
  }, [ghostPrefill]); // eslint-disable-line react-hooks/exhaustive-deps

  function startCreate() {
    setSelected(null);
    setCreating(true);
    setForm({ ...EMPTY_FORM, category_id: categories[0]?.category_id ?? "" });
    setError(null);
    setCostPrice(""); setMarkupPct("");
    setPendingBarcodes([]);
    setExtraBarcodes([]);
    setNewBarcodeInput("");
    setBarcodeError(null);
  }

  function startEdit(p: AdminProduct) {
    setCreating(false);
    setSelected(p);
    setForm({
      name:                   p.name,
      category_id:            p.category_id,
      sku:                    p.sku ?? "",
      barcode:                p.barcode ?? "",
      tax_rule_id:            p.tax_rule_id ?? "",
      price:                  formatMoney(p.price_minor, exp),
      track_inventory:        p.track_inventory,
      allow_decimal_quantity: p.allow_decimal_quantity,
      reorder_point:          p.reorder_point,
      is_active:              p.is_active,
      image_path:             p.image_path ?? "",
    });
    setError(null);
    setCostPrice(""); setMarkupPct("");
    setNewBarcodeInput("");
    setBarcodeError(null);
    // Load existing extra barcodes
    cmd.productBarcodesList(p.product_id)
      .then(setExtraBarcodes)
      .catch(() => setExtraBarcodes([]));
  }

  async function pickImage() {
    try {
      const path = await cmd.productPickImage();
      if (path) set("image_path", path);
    } catch {
      setError("Could not open file picker");
    }
  }

  function cancelEdit() {
    setSelected(null); setCreating(false); setError(null);
    setCostPrice(""); setMarkupPct("");
    setExtraBarcodes([]); setPendingBarcodes([]); setNewBarcodeInput(""); setBarcodeError(null);
  }

  function handleCostChange(val: string) {
    setCostPrice(val);
    const cost = parseMoney(val, exp);
    const markup = parseFloat(markupPct);
    if (cost > 0 && !isNaN(markup) && markup >= 0) {
      const selling = cost * (1 + markup / 100);
      set("price", formatMoney(Math.round(selling), exp));
    }
  }

  function handleMarkupChange(val: string) {
    setMarkupPct(val);
    const cost = parseMoney(costPrice, exp);
    const markup = parseFloat(val);
    if (cost > 0 && !isNaN(markup) && markup >= 0) {
      const selling = cost * (1 + markup / 100);
      set("price", formatMoney(Math.round(selling), exp));
    }
  }

  async function addBarcodeForEdit() {
    const bc = newBarcodeInput.trim();
    if (!bc) return;
    if (!selected) return;
    setBarcodeError(null);
    try {
      const row = await cmd.productBarcodeAdd(sessionUserId, selected.product_id, bc);
      setExtraBarcodes(prev => [...prev, row]);
      setNewBarcodeInput("");
      newBarcodeRef.current?.focus();
    } catch (e: unknown) {
      setBarcodeError(typeof e === "string" ? e : "Failed to add barcode");
    }
  }

  async function removeBarcodeForEdit(barcodeId: string) {
    setBarcodeError(null);
    try {
      await cmd.productBarcodeRemove(sessionUserId, barcodeId);
      setExtraBarcodes(prev => prev.filter(b => b.barcode_id !== barcodeId));
    } catch (e: unknown) {
      setBarcodeError(typeof e === "string" ? e : "Failed to remove barcode");
    }
  }

  function addPendingBarcode() {
    const bc = newBarcodeInput.trim();
    if (!bc) return;
    if (pendingBarcodes.some(p => p.barcode === bc)) {
      setBarcodeError("Barcode already in list"); return;
    }
    setBarcodeError(null);
    setPendingBarcodes(prev => [...prev, { tempId: Math.random().toString(36), barcode: bc }]);
    setNewBarcodeInput("");
    newBarcodeRef.current?.focus();
  }

  function removePendingBarcode(tempId: string) {
    setPendingBarcodes(prev => prev.filter(p => p.tempId !== tempId));
  }

  function set(key: string, val: unknown) { setForm(f => ({ ...f, [key]: val })); }

  async function save() {
    if (!form.name.trim() || !form.category_id) {
      setError("Name and category are required."); return;
    }
    const price_minor = parseMoney(form.price, exp);
    if (price_minor <= 0) { setError("Price must be greater than zero."); return; }

    setSaving(true); setError(null);
    try {
      if (creating) {
        const created = await cmd.adminCreateProduct({
          category_id: form.category_id, name: form.name.trim(),
          sku: form.sku.trim() || undefined, barcode: form.barcode.trim() || undefined,
          tax_rule_id: form.tax_rule_id || undefined, price_minor,
          track_inventory: form.track_inventory,
          allow_decimal_quantity: form.allow_decimal_quantity,
          reorder_point: form.reorder_point,
          created_by_user_id: sessionUserId,
          image_path: form.image_path.trim() || undefined,
        });
        // Insert pending extra barcodes
        for (const pb of pendingBarcodes) {
          try { await cmd.productBarcodeAdd(sessionUserId, created.product_id, pb.barcode); } catch { /* skip dup */ }
        }
        await fetchProducts(search, offset);
      } else if (selected) {
        await cmd.adminUpdateProduct({
          product_id: selected.product_id,
          category_id: form.category_id, name: form.name.trim(),
          sku: form.sku.trim() || undefined, barcode: form.barcode.trim() || undefined,
          tax_rule_id: form.tax_rule_id || undefined, price_minor,
          track_inventory: form.track_inventory,
          allow_decimal_quantity: form.allow_decimal_quantity,
          reorder_point: form.reorder_point, is_active: form.is_active,
          updated_by_user_id: sessionUserId,
          image_path: form.image_path.trim() || undefined,
        });
        await fetchProducts(search, offset);
      }
      cancelEdit();
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Save failed");
    } finally {
      setSaving(false);
    }
  }

  const showingForm = creating || selected !== null;

  async function refreshProducts() {
    await fetchProducts(search, 0);
    setOffset(0);
  }

  return (
    <>
    {printProducts && (
      <BarcodesPrintModal products={printProducts} onClose={() => setPrintProducts(null)} />
    )}
    {showBulkImport && (
      <BulkImportModal
        mode="products"
        sessionUserId={sessionUserId}
        onClose={() => setShowBulkImport(false)}
        onDone={refreshProducts}
      />
    )}
    <div className="bo-tab-layout">
      {ghostSummary && (ghostSummary.pending + ghostSummary.found + ghostSummary.not_found) > 0 && (
        <GhostBarcodesPanel
          sessionUserId={sessionUserId}
          summary={ghostSummary}
          onCreateProduct={(prefill) => onCreateProductFromGhost?.(prefill)}
          onCountChange={() => onGhostCountChange?.()}
        />
      )}
      {/* ── List pane ── */}
      <div className="bo-list-pane">
        <div className="bo-list-header">
          <input
            className="bo-search"
            placeholder="Search products…"
            value={searchInput}
            onChange={e => setSearchInput(e.target.value)}
          />
          <button
            className="btn-secondary bo-print-labels-btn"
            onClick={() => setPrintProducts(products)}
            title="Print barcode labels"
          >
            Print Labels
          </button>
          <button
            className="btn-secondary bo-import-btn"
            onClick={() => setShowBulkImport(true)}
            title="Bulk import products from CSV"
          >
            Import CSV
          </button>
          <button className="btn-primary bo-add-btn" onClick={startCreate}>+ New</button>
        </div>
        {total > 0 && (
          <div className="bo-pagination">
            <span className="bo-pagination-info">
              {loading ? "Loading…" : `${offset + 1}–${Math.min(offset + products.length, total)} of ${total.toLocaleString()}`}
            </span>
            <button className="bo-pagination-btn" disabled={offset === 0 || loading}
              onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}>‹ Prev</button>
            <button className="bo-pagination-btn" disabled={offset + PAGE_SIZE >= total || loading}
              onClick={() => setOffset(offset + PAGE_SIZE)}>Next ›</button>
          </div>
        )}
        <div className="bo-list">
          {loading && products.length === 0 && <div className="bo-empty">Loading…</div>}
          {products.map(p => (
            <div key={p.product_id} className="bo-list-row-wrap">
              <button
                className={`bo-list-row ${selected?.product_id === p.product_id ? "bo-list-row-active" : ""} ${!p.is_active ? "bo-list-row-inactive" : ""}`}
                onClick={() => startEdit(p)}
              >
                <div className="bo-list-row-main">
                  <span className="bo-list-row-name">{p.name}</span>
                  <span className="bo-list-row-sub">{p.category_name}{p.sku ? ` · ${p.sku}` : ""}</span>
                </div>
                <div className="bo-list-row-right">
                  <span className="bo-list-row-price">{cur} {formatMoney(p.price_minor, exp)}</span>
                  {!p.is_active && <span className="bo-badge-inactive">Inactive</span>}
                </div>
              </button>
              <button
                className="btn-secondary bo-label-btn"
                onClick={e => { e.stopPropagation(); setPrintProducts([p]); }}
                title="Print label for this product"
              >
                Label
              </button>
            </div>
          ))}
          {!loading && products.length === 0 && (
            <div className="bo-empty">
              <div className="bo-empty-icon">📦</div>
              <p className="bo-empty-title">No products found</p>
              <p className="bo-empty-hint">Try a different search term, or add a new product.</p>
            </div>
          )}
        </div>
      </div>

      {/* ── Form pane ── */}
      {showingForm && (
        <div className="bo-form-pane">
          <h3 className="bo-form-title">{creating ? "New Product" : "Edit Product"}</h3>
          {error && <div className="bo-form-error">{error}</div>}

          <label className="bo-label">Name *</label>
          <input className="bo-input" value={form.name} onChange={e => set("name", e.target.value)} placeholder="Product name" autoFocus />

          <label className="bo-label">Category *</label>
          <select className="bo-select" value={form.category_id} onChange={e => set("category_id", e.target.value)}>
            <option value="">— select —</option>
            {categories.map(c => <option key={c.category_id} value={c.category_id}>{c.name}</option>)}
          </select>

          <label className="bo-label">Selling Price ({cur}) *</label>
          <input className="bo-input" type="number" inputMode="decimal" min="0" step={Math.pow(10, -exp).toFixed(exp)}
            value={form.price} onChange={e => set("price", e.target.value)} placeholder={`0.${"0".repeat(exp)}`} />

          <div className="bo-cost-row">
            <div>
              <label className="bo-label">Cost Price ({cur})</label>
              <input className="bo-input" type="number" inputMode="decimal" min="0" step={Math.pow(10, -exp).toFixed(exp)}
                value={costPrice} onChange={e => handleCostChange(e.target.value)}
                placeholder={`0.${"0".repeat(exp)}`} />
            </div>
            <div>
              <label className="bo-label">Markup %</label>
              <input className="bo-input" type="number" inputMode="decimal" min="0" step="0.1"
                value={markupPct} onChange={e => handleMarkupChange(e.target.value)}
                placeholder="0" />
            </div>
          </div>
          {costPrice && markupPct && (
            <p className="bo-cost-hint">
              Cost {cur} {formatMoney(parseMoney(costPrice, exp), exp)} + {markupPct}% markup = {cur} {form.price}
            </p>
          )}

          <label className="bo-label">Tax Rule</label>
          <select className="bo-select" value={form.tax_rule_id} onChange={e => set("tax_rule_id", e.target.value)}>
            <option value="">None</option>
            {taxRules.map(t => <option key={t.tax_rule_id} value={t.tax_rule_id}>{t.name}</option>)}
          </select>

          <div className="bo-row-two">
            <div>
              <label className="bo-label">SKU</label>
              <input className="bo-input" value={form.sku} onChange={e => set("sku", e.target.value)} placeholder="Optional" />
            </div>
            <div>
              <label className="bo-label">Barcode</label>
              <input className="bo-input" value={form.barcode} onChange={e => set("barcode", e.target.value)} placeholder="Optional" />
            </div>
          </div>

          {/* Additional Barcodes */}
          <label className="bo-label">Additional Barcodes (scan or type)</label>
          {barcodeError && <div className="bo-form-error bo-barcode-error">{barcodeError}</div>}
          <div className="bo-extra-barcodes">
            {/* Existing product: show saved barcodes */}
            {!creating && extraBarcodes.map(b => (
              <div key={b.barcode_id} className="bo-barcode-row">
                <span className="bo-barcode-text">{b.barcode}</span>
                <button className="bo-barcode-remove" type="button" onClick={() => removeBarcodeForEdit(b.barcode_id)} title="Remove">✕</button>
              </div>
            ))}
            {/* New product: show pending list */}
            {creating && pendingBarcodes.map(pb => (
              <div key={pb.tempId} className="bo-barcode-row">
                <span className="bo-barcode-text">{pb.barcode}</span>
                <button className="bo-barcode-remove" type="button" onClick={() => removePendingBarcode(pb.tempId)} title="Remove">✕</button>
              </div>
            ))}
            {/* Add input */}
            <div className="bo-barcode-add-row">
              <input
                ref={newBarcodeRef}
                className="bo-input bo-barcode-input"
                value={newBarcodeInput}
                onChange={e => setNewBarcodeInput(e.target.value)}
                onKeyDown={e => { if (e.key === "Enter") { e.preventDefault(); if (creating) { addPendingBarcode(); } else { void addBarcodeForEdit(); } } }}
                placeholder="Scan or type barcode…"
              />
              <button
                className="btn-secondary bo-barcode-add-btn"
                type="button"
                onClick={() => { if (creating) { addPendingBarcode(); } else { void addBarcodeForEdit(); } }}
              >
                Add
              </button>
            </div>
          </div>

          <div className="bo-checkboxes">
            <label className="bo-checkbox-label">
              <input type="checkbox" checked={form.track_inventory} onChange={e => set("track_inventory", e.target.checked)} />
              Track Inventory
            </label>
            <label className="bo-checkbox-label">
              <input type="checkbox" checked={form.allow_decimal_quantity} onChange={e => set("allow_decimal_quantity", e.target.checked)} />
              Decimal Quantity
            </label>
            {!creating && (
              <label className="bo-checkbox-label">
                <input type="checkbox" checked={form.is_active} onChange={e => set("is_active", e.target.checked)} />
                Active
              </label>
            )}
          </div>

          {form.track_inventory && (
            <>
              <label className="bo-label">Reorder Point</label>
              <input className="bo-input" type="number" min="0" step="1"
                value={form.reorder_point} onChange={e => set("reorder_point", parseInt(e.target.value) || 0)} />
            </>
          )}

          <label className="bo-label">Product Image</label>
          <div className="prod-image-row">
            {form.image_path && (
              <img
                className="prod-image-preview"
                src={convertFileSrc(form.image_path)}
                alt="Product"
                onError={e => { (e.target as HTMLImageElement).style.display = "none"; }}
              />
            )}
            {!form.image_path && (
              <div className="prod-image-placeholder">No image</div>
            )}
            <div className="prod-image-btns">
              <button className="btn-secondary" type="button" onClick={pickImage}>
                Choose Image
              </button>
              {form.image_path && (
                <button className="btn-secondary" type="button" onClick={() => set("image_path", "")}>
                  Remove
                </button>
              )}
            </div>
          </div>

          <div className="bo-form-actions">
            <button className="btn-secondary" onClick={cancelEdit}>Cancel</button>
            <button className="btn-primary" onClick={save} disabled={saving}>
              {saving ? "Saving…" : "Save"}
            </button>
          </div>
        </div>
      )}
    </div>
    </>
  );
}
