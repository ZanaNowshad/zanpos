import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { AdminProduct, CategoryRow, ProductBarcodeRow, TaxRuleRow } from "../types";
import { DEVICE } from "../types";
import { formatMoney, parseMoney } from "../money";
import * as cmd from "../tauri/commands";
import { useFocusTrap } from "../hooks/useFocusTrap";

interface Props {
  mode: "create" | "edit";
  product?: AdminProduct | null;
  prefilledName?: string;
  prefilledBarcode?: string;
  categories: CategoryRow[];
  taxRules: TaxRuleRow[];
  sessionUserId: string;
  onClose: () => void;
  onSaved: () => void;
}

type PendingBarcode = { tempId: string; barcode: string };

export default function ProductFormModal({
  mode, product, prefilledName, prefilledBarcode,
  categories, taxRules, sessionUserId, onClose, onSaved,
}: Props) {
  const modalRef = useRef<HTMLDivElement>(null);
  useFocusTrap(modalRef, onClose);
  const exp = DEVICE.currency_exponent;
  const cur = DEVICE.currency;

  const [name, setName] = useState(product?.name ?? prefilledName ?? "");
  const [categoryId, setCategoryId] = useState(product?.category_id ?? categories[0]?.category_id ?? "");
  const [price, setPrice] = useState(product ? formatMoney(product.price_minor, exp) : "");
  const [sku, setSku] = useState(product?.sku ?? "");
  const [barcode, setBarcode] = useState(product?.barcode ?? prefilledBarcode ?? "");
  const [taxRuleId, setTaxRuleId] = useState(product?.tax_rule_id ?? "");
  const [trackInventory, setTrackInventory] = useState(product?.track_inventory ?? true);
  const [allowDecimal, setAllowDecimal] = useState(product?.allow_decimal_quantity ?? false);
  const [reorderPoint, setReorderPoint] = useState(product?.reorder_point ?? 0);
  const [isActive, setIsActive] = useState(product?.is_active ?? true);
  const [imagePath, setImagePath] = useState(product?.image_path ?? "");
  const [costPrice, setCostPrice] = useState("");
  const [markupPct, setMarkupPct] = useState("");
  const [extraBarcodes, setExtraBarcodes] = useState<ProductBarcodeRow[]>([]);
  const [pendingBarcodes, setPendingBarcodes] = useState<PendingBarcode[]>([]);
  const [newBarcodeInput, setNewBarcodeInput] = useState("");
  const [barcodeErr, setBarcodeErr] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const newBarcodeRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (mode === "edit" && product) {
      cmd.productBarcodesList(sessionUserId, product.product_id).then(setExtraBarcodes).catch(() => {});
    }
  }, [mode, product]);

  async function pickImage() {
    try { const p = await cmd.productPickImage(); if (p) setImagePath(p); }
    catch { setError("Could not open file picker"); }
  }

  function computeSelling(cost: number, pct: string): number {
    const bp = Math.round(parseFloat(pct || "0") * 100);
    if (cost <= 0 || isNaN(bp) || bp < 0) return 0;
    return Math.round(cost * (10000 + bp) / 10000);
  }

  function addPending() {
    const bc = newBarcodeInput.trim();
    if (!bc) return;
    if (pendingBarcodes.some(p => p.barcode === bc)) { setBarcodeErr("Already in list"); return; }
    setBarcodeErr(null);
    setPendingBarcodes(p => [...p, { tempId: Math.random().toString(36), barcode: bc }]);
    setNewBarcodeInput("");
    newBarcodeRef.current?.focus();
  }

  async function addBarcodeForEdit() {
    const bc = newBarcodeInput.trim();
    if (!bc || !product) return;
    setBarcodeErr(null);
    try {
      const row = await cmd.productBarcodeAdd(sessionUserId, product.product_id, bc);
      setExtraBarcodes(p => [...p, row]);
      setNewBarcodeInput("");
      newBarcodeRef.current?.focus();
    } catch (e: unknown) { setBarcodeErr(typeof e === "string" ? e : "Failed"); }
  }

  async function removeBarcodeForEdit(id: string) {
    try {
      await cmd.productBarcodeRemove(sessionUserId, id);
      setExtraBarcodes(p => p.filter(b => b.barcode_id !== id));
    } catch (e: unknown) { setBarcodeErr(typeof e === "string" ? e : "Failed"); }
  }

  async function save() {
    if (!name.trim() || !categoryId) { setError("Name and category required"); return; }
    const priceMinor = parseMoney(price, exp);
    if (priceMinor <= 0) { setError("Price must be > 0"); return; }
    setSaving(true); setError(null);
    try {
      if (mode === "create") {
        const created = await cmd.adminCreateProduct({
          category_id: categoryId, name: name.trim(),
          sku: sku.trim() || undefined, barcode: barcode.trim() || undefined,
          tax_rule_id: taxRuleId || undefined, price_minor: priceMinor,
          track_inventory: trackInventory, allow_decimal_quantity: allowDecimal,
          reorder_point: reorderPoint, created_by_user_id: sessionUserId,
          image_path: imagePath.trim() || undefined,
        });
        for (const pb of pendingBarcodes) {
          try { await cmd.productBarcodeAdd(sessionUserId, created.product_id, pb.barcode); } catch {}
        }
      } else if (product) {
        await cmd.adminUpdateProduct({
          product_id: product.product_id, category_id: categoryId, name: name.trim(),
          sku: sku.trim() || undefined, barcode: barcode.trim() || undefined,
          tax_rule_id: taxRuleId || undefined, price_minor: priceMinor,
          track_inventory: trackInventory, allow_decimal_quantity: allowDecimal,
          reorder_point: reorderPoint, is_active: isActive,
          updated_by_user_id: sessionUserId, image_path: imagePath.trim() || undefined,
        });
      }
      onSaved();
    } catch (e: unknown) { setError(typeof e === "string" ? e : "Save failed"); }
    finally { setSaving(false); }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div ref={modalRef} className="modal bo-form-modal" role="dialog" aria-modal="true"
        aria-label={mode === "create" ? "New Product" : "Edit Product"}
        onClick={e => e.stopPropagation()}>
        <div className="bo-form-modal-header">
          <h2>{mode === "create" ? "New Product" : "Edit Product"}</h2>
          <button className="bo-form-modal-close" onClick={onClose}>✕</button>
        </div>
        <div className="bo-form-modal-body">
          {error && <div className="bo-form-error">{error}</div>}

          <div className="bo-form-grid">
            <div className="bo-form-field bo-form-field-full">
              <label className="bo-label">Name *</label>
              <input className="bo-input" value={name} onChange={e => setName(e.target.value)} placeholder="Product name" autoFocus />
            </div>
            <div className="bo-form-field">
              <label className="bo-label">Category *</label>
              <select className="bo-select" value={categoryId} onChange={e => setCategoryId(e.target.value)}>
                <option value="">— select —</option>
                {categories.map(c => <option key={c.category_id} value={c.category_id}>{c.name}</option>)}
              </select>
            </div>
            <div className="bo-form-field">
              <label className="bo-label">Price ({cur}) *</label>
              <input className="bo-input" type="number" inputMode="decimal" min="0" step={Math.pow(10, -exp).toFixed(exp)}
                value={price} onChange={e => setPrice(e.target.value)} placeholder={`0.${"0".repeat(exp)}`} />
            </div>
            <div className="bo-form-field">
              <label className="bo-label">Cost ({cur})</label>
              <input className="bo-input" type="number" inputMode="decimal" min="0" step={Math.pow(10, -exp).toFixed(exp)}
                value={costPrice} onChange={e => { setCostPrice(e.target.value); const s = computeSelling(parseMoney(e.target.value, exp), markupPct); if (s > 0) setPrice(formatMoney(s, exp)); }}
                placeholder={`0.${"0".repeat(exp)}`} />
            </div>
            <div className="bo-form-field">
              <label className="bo-label">Markup %</label>
              <input className="bo-input" type="number" inputMode="decimal" min="0" step="0.1"
                value={markupPct} onChange={e => { setMarkupPct(e.target.value); const s = computeSelling(parseMoney(costPrice, exp), e.target.value); if (s > 0) setPrice(formatMoney(s, exp)); }}
                placeholder="0" />
            </div>
            <div className="bo-form-field">
              <label className="bo-label">Tax Rule</label>
              <select className="bo-select" value={taxRuleId} onChange={e => setTaxRuleId(e.target.value)}>
                <option value="">None</option>
                {taxRules.map(t => <option key={t.tax_rule_id} value={t.tax_rule_id}>{t.name}</option>)}
              </select>
            </div>
            <div className="bo-form-field">
              <label className="bo-label">SKU</label>
              <input className="bo-input" value={sku} onChange={e => setSku(e.target.value)} placeholder="Optional" />
            </div>
            <div className="bo-form-field bo-form-field-full">
              <label className="bo-label">Barcode</label>
              <input className="bo-input" value={barcode} onChange={e => setBarcode(e.target.value)} placeholder="Optional" />
            </div>
          </div>

          {/* Extra barcodes */}
          <label className="bo-label">Additional Barcodes</label>
          {barcodeErr && <div className="bo-form-error" style={{marginTop: 4}}>{barcodeErr}</div>}
          <div style={{marginBottom: 8}}>
            {(mode === "edit" ? extraBarcodes : pendingBarcodes).map((b: any) => (
              <div key={b.barcode_id ?? b.tempId} className="bo-barcode-row">
                <span className="bo-barcode-text">{b.barcode}</span>
                <button className="bo-barcode-remove"
                  onClick={() => mode === "edit" ? removeBarcodeForEdit(b.barcode_id) : setPendingBarcodes(p => p.filter(x => x.tempId !== b.tempId))}
                >✕</button>
              </div>
            ))}
            <div className="bo-barcode-add-row">
              <input ref={newBarcodeRef} className="bo-input" style={{flex: 1}} value={newBarcodeInput}
                onChange={e => setNewBarcodeInput(e.target.value)}
                onKeyDown={e => { if (e.key === "Enter") { e.preventDefault(); mode === "create" ? addPending() : addBarcodeForEdit(); } }}
                placeholder="Scan or type barcode…" />
              <button className="btn-secondary" style={{whiteSpace: "nowrap"}}
                onClick={() => mode === "create" ? addPending() : addBarcodeForEdit()}>Add</button>
            </div>
          </div>

          <div className="bo-checkboxes" style={{margin: "10px 0"}}>
            <label className="bo-checkbox-label"><input type="checkbox" checked={trackInventory} onChange={e => setTrackInventory(e.target.checked)} />Track Inventory</label>
            <label className="bo-checkbox-label"><input type="checkbox" checked={allowDecimal} onChange={e => setAllowDecimal(e.target.checked)} />Decimal Qty</label>
            {mode === "edit" && <label className="bo-checkbox-label"><input type="checkbox" checked={isActive} onChange={e => setIsActive(e.target.checked)} />Active</label>}
          </div>

          {trackInventory && (
            <div style={{marginBottom: 10}}>
              <label className="bo-label">Reorder Point</label>
              <input className="bo-input" type="number" min="0" step="1" value={reorderPoint} onChange={e => setReorderPoint(parseInt(e.target.value) || 0)} />
            </div>
          )}

          <label className="bo-label">Image</label>
          <div className="prod-image-row">
            {imagePath ? (
              <img className="prod-image-preview" src={convertFileSrc(imagePath)} alt="" onError={e => { (e.target as HTMLImageElement).style.display = "none"; }} />
            ) : <div className="prod-image-placeholder">No image</div>}
            <div className="prod-image-btns">
              <button className="btn-secondary" type="button" onClick={pickImage}>Choose</button>
              {imagePath && <button className="btn-secondary" type="button" onClick={() => setImagePath("")}>Remove</button>}
            </div>
          </div>
        </div>
        <div className="bo-form-modal-footer">
          <button className="btn-secondary" onClick={onClose}>Cancel</button>
          <button className="btn-primary" onClick={save} disabled={saving}>{saving ? "Saving…" : "Save Product"}</button>
        </div>
      </div>
    </div>
  );
}
