import { useRef, useState } from "react";
import type { CategoryRow } from "../types";
import * as cmd from "../tauri/commands";
import { useFocusTrap } from "../hooks/useFocusTrap";

interface Props {
  mode: "create" | "edit";
  category?: CategoryRow | null;
  nextOrder: number;
  sessionUserId: string;
  onClose: () => void;
  onSaved: () => void;
}

export default function CategoryFormModal({ mode, category, nextOrder, sessionUserId, onClose, onSaved }: Props) {
  const modalRef = useRef<HTMLDivElement>(null);
  useFocusTrap(modalRef, onClose);

  const [name, setName] = useState(category?.name ?? "");
  const [sortOrder, setSortOrder] = useState(category?.sort_order ?? nextOrder);
  const [isActive, setIsActive] = useState(category?.is_active ?? true);
  const [parentId, setParentId] = useState(category?.parent_category_id ?? "");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function save() {
    if (!name.trim()) { setError("Name is required"); return; }
    setSaving(true); setError(null);
    try {
      await cmd.adminSaveCategory({
        category_id: mode === "create" ? undefined : category!.category_id,
        name: name.trim(),
        sort_order: sortOrder,
        is_active: isActive,
        parent_category_id: parentId.trim() || undefined,
        actor_user_id: sessionUserId,
      });
      onSaved();
    } catch (e: unknown) { setError(typeof e === "string" ? e : "Save failed"); }
    finally { setSaving(false); }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div ref={modalRef} className="modal bo-form-modal" role="dialog" aria-modal="true"
        aria-label={mode === "create" ? "New Category" : "Edit Category"}
        onClick={e => e.stopPropagation()}>
        <div className="bo-form-modal-header">
          <h2>{mode === "create" ? "New Category" : "Edit Category"}</h2>
          <button className="bo-form-modal-close" onClick={onClose}>✕</button>
        </div>
        <div className="bo-form-modal-body">
          {error && <div className="bo-form-error">{error}</div>}
          <div className="bo-form-field">
            <label className="bo-label">Name *</label>
            <input className="bo-input" value={name} onChange={e => setName(e.target.value)} placeholder="Category name" autoFocus />
          </div>
          <div className="bo-row-two">
            <div className="bo-form-field">
              <label className="bo-label">Sort Order</label>
              <input className="bo-input" type="number" min="0" step="1" value={sortOrder} onChange={e => setSortOrder(parseInt(e.target.value) || 0)} />
            </div>
            <div className="bo-form-field">
              <label className="bo-label">Parent Category</label>
              <input className="bo-input" value={parentId} onChange={e => setParentId(e.target.value)} placeholder="None" />
            </div>
          </div>
          {mode === "edit" && (
            <label className="bo-checkbox-label" style={{marginTop: 10}}>
              <input type="checkbox" checked={isActive} onChange={e => setIsActive(e.target.checked)} />Active
            </label>
          )}
        </div>
        <div className="bo-form-modal-footer">
          <button className="btn-secondary" onClick={onClose}>Cancel</button>
          <button className="btn-primary" onClick={save} disabled={saving}>{saving ? "Saving…" : "Save Category"}</button>
        </div>
      </div>
    </div>
  );
}
