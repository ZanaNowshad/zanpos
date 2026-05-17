import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { AdminProduct, CategoryRow, TaxRuleRow } from "../types";
import { DEVICE } from "../types";
import { formatMoney, parseMoney } from "../money";
import * as cmd from "../tauri/commands";
import BarcodesPrintModal from "./BarcodesPrintModal";

interface Props {
  sessionUserId: string;
}

const EMPTY_FORM = {
  name: "", category_id: "", sku: "", barcode: "",
  tax_rule_id: "", price: "", track_inventory: true,
  allow_decimal_quantity: false, reorder_point: 0, is_active: true,
  image_path: "" as string,
};

export default function ProductsTab({ sessionUserId }: Props) {
  const [products, setProducts]       = useState<AdminProduct[]>([]);
  const [categories, setCategories]   = useState<CategoryRow[]>([]);
  const [taxRules, setTaxRules]       = useState<TaxRuleRow[]>([]);
  const [selected, setSelected]       = useState<AdminProduct | null>(null);
  const [creating, setCreating]       = useState(false);
  const [form, setForm]               = useState(EMPTY_FORM);
  const [saving, setSaving]           = useState(false);
  const [error, setError]             = useState<string | null>(null);
  const [search, setSearch]           = useState("");
  const [printProducts, setPrintProducts] = useState<AdminProduct[] | null>(null);

  const exp = DEVICE.currency_exponent;
  const cur = DEVICE.currency;

  useEffect(() => {
    Promise.all([
      cmd.adminListProducts(),
      cmd.adminListCategories(),
      cmd.adminListTaxRules(),
    ]).then(([prods, cats, taxes]) => {
      setProducts(prods);
      setCategories(cats.filter(c => c.is_active));
      setTaxRules(taxes);
    });
  }, []);

  function startCreate() {
    setSelected(null);
    setCreating(true);
    setForm({ ...EMPTY_FORM, category_id: categories[0]?.category_id ?? "" });
    setError(null);
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
  }

  async function pickImage() {
    try {
      const path = await cmd.productPickImage();
      if (path) set("image_path", path);
    } catch {
      setError("Could not open file picker");
    }
  }

  function cancelEdit() { setSelected(null); setCreating(false); setError(null); }

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
        setProducts(prev => [created, ...prev]);
      } else if (selected) {
        const updated = await cmd.adminUpdateProduct({
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
        setProducts(prev => prev.map(p => p.product_id === updated.product_id ? updated : p));
      }
      cancelEdit();
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Save failed");
    } finally {
      setSaving(false);
    }
  }

  const showingForm = creating || selected !== null;
  const filtered = products.filter(p =>
    !search || p.name.toLowerCase().includes(search.toLowerCase()) ||
    p.sku?.toLowerCase().includes(search.toLowerCase()) ||
    p.barcode?.includes(search)
  );

  return (
    <>
    {printProducts && (
      <BarcodesPrintModal products={printProducts} onClose={() => setPrintProducts(null)} />
    )}
    <div className="bo-tab-layout">
      {/* ── List pane ── */}
      <div className="bo-list-pane">
        <div className="bo-list-header">
          <input
            className="bo-search"
            placeholder="Search products…"
            value={search}
            onChange={e => setSearch(e.target.value)}
          />
          <button
            className="btn-secondary bo-print-labels-btn"
            onClick={() => setPrintProducts(filtered.length > 0 ? filtered : products)}
            title="Print barcode labels"
          >
            Print Labels
          </button>
          <button className="btn-primary bo-add-btn" onClick={startCreate}>+ New</button>
        </div>
        <div className="bo-list">
          {filtered.map(p => (
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
          {filtered.length === 0 && (
            <div className="bo-empty">No products found.</div>
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

          <label className="bo-label">Price ({cur}) *</label>
          <input className="bo-input" type="number" min="0" step={Math.pow(10, -exp).toFixed(exp)}
            value={form.price} onChange={e => set("price", e.target.value)} placeholder={`0.${"0".repeat(exp)}`} />

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
