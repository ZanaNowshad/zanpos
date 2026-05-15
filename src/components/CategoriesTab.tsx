import { useEffect, useState } from "react";
import type { CategoryRow } from "../types";
import * as cmd from "../tauri/commands";

const EMPTY_FORM = { name: "", sort_order: 0, is_active: true };

export default function CategoriesTab() {
  const [categories, setCategories] = useState<CategoryRow[]>([]);
  const [selected, setSelected]     = useState<CategoryRow | null>(null);
  const [creating, setCreating]     = useState(false);
  const [form, setForm]             = useState(EMPTY_FORM);
  const [saving, setSaving]         = useState(false);
  const [error, setError]           = useState<string | null>(null);

  useEffect(() => {
    cmd.adminListCategories().then(setCategories);
  }, []);

  function startCreate() {
    setSelected(null); setCreating(true);
    const nextOrder = Math.max(0, ...categories.map(c => c.sort_order)) + 1;
    setForm({ name: "", sort_order: nextOrder, is_active: true });
    setError(null);
  }

  function startEdit(c: CategoryRow) {
    setCreating(false); setSelected(c);
    setForm({ name: c.name, sort_order: c.sort_order, is_active: c.is_active });
    setError(null);
  }

  function cancelEdit() { setSelected(null); setCreating(false); setError(null); }
  function set(key: string, val: unknown) { setForm(f => ({ ...f, [key]: val })); }

  async function save() {
    if (!form.name.trim()) { setError("Name is required."); return; }
    setSaving(true); setError(null);
    try {
      const saved = await cmd.adminSaveCategory({
        category_id: creating ? undefined : selected!.category_id,
        name: form.name.trim(),
        sort_order: form.sort_order,
        is_active: form.is_active,
      });
      setCategories(prev =>
        creating
          ? [saved, ...prev]
          : prev.map(c => c.category_id === saved.category_id ? saved : c)
      );
      cancelEdit();
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Save failed");
    } finally {
      setSaving(false);
    }
  }

  const showingForm = creating || selected !== null;

  return (
    <div className="bo-tab-layout">
      <div className="bo-list-pane">
        <div className="bo-list-header">
          <span className="bo-list-title">Categories</span>
          <button className="btn-primary bo-add-btn" onClick={startCreate}>+ New</button>
        </div>
        <div className="bo-list">
          {categories.map(c => (
            <button
              key={c.category_id}
              className={`bo-list-row ${selected?.category_id === c.category_id ? "bo-list-row-active" : ""} ${!c.is_active ? "bo-list-row-inactive" : ""}`}
              onClick={() => startEdit(c)}
            >
              <div className="bo-list-row-main">
                <span className="bo-list-row-name">{c.name}</span>
                <span className="bo-list-row-sub">Sort #{c.sort_order}</span>
              </div>
              {!c.is_active && <span className="bo-badge-inactive">Inactive</span>}
            </button>
          ))}
          {categories.length === 0 && <div className="bo-empty">No categories.</div>}
        </div>
      </div>

      {showingForm && (
        <div className="bo-form-pane">
          <h3 className="bo-form-title">{creating ? "New Category" : "Edit Category"}</h3>
          {error && <div className="bo-form-error">{error}</div>}

          <label className="bo-label">Name *</label>
          <input className="bo-input" value={form.name} onChange={e => set("name", e.target.value)}
            placeholder="Category name" autoFocus />

          <label className="bo-label">Sort Order</label>
          <input className="bo-input" type="number" min="0" step="1"
            value={form.sort_order} onChange={e => set("sort_order", parseInt(e.target.value) || 0)} />

          {!creating && (
            <div className="bo-checkboxes" style={{ marginTop: 12 }}>
              <label className="bo-checkbox-label">
                <input type="checkbox" checked={form.is_active}
                  onChange={e => set("is_active", e.target.checked)} />
                Active
              </label>
            </div>
          )}

          <div className="bo-form-actions">
            <button className="btn-secondary" onClick={cancelEdit}>Cancel</button>
            <button className="btn-primary" onClick={save} disabled={saving}>
              {saving ? "Saving…" : "Save"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
