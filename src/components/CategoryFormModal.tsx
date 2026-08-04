import { useMemo, useRef, useState } from "react";
import type { CategoryRow } from "../types";
import * as cmd from "../tauri/commands";
import { useAutoFocus } from "../hooks/useAutoFocus";
import { useFocusTrap } from "../hooks/useFocusTrap";
import { useLanguage } from "../hooks/useLanguage";
import { modalTranslator } from "../i18n/modalStrings";

interface Props {
  mode: "create" | "edit";
  category?: CategoryRow | null;
  nextOrder: number;
  sessionUserId: string;
  onClose: () => void;
  onSaved: () => void;
}

export default function CategoryFormModal({ mode, category, nextOrder, sessionUserId, onClose, onSaved }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => modalTranslator(language), [language]);
  const modalRef = useRef<HTMLDivElement>(null);
  useFocusTrap(modalRef, onClose);

  const nameRef = useAutoFocus<HTMLInputElement>();
  const [name, setName] = useState(category?.name ?? "");
  const [sortOrder, setSortOrder] = useState(category?.sort_order ?? nextOrder);
  const [isActive, setIsActive] = useState(category?.is_active ?? true);
  const [parentId, setParentId] = useState(category?.parent_category_id ?? "");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function save() {
    if (!name.trim()) { setError(t("nameRequired")); return; }
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
    } catch (e: unknown) { setError(typeof e === "string" ? e : t("saveFailed")); }
    finally { setSaving(false); }
  }

  return (
    <button className="modal-overlay" type="button" onClick={onClose}>
      <div ref={modalRef} className="modal bo-form-modal" role="dialog" aria-modal="true"
        aria-label={mode === "create" ? t("newCategory") : t("editCategory")}
        onClick={e => e.stopPropagation()} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}>
        <div className="bo-form-modal-header">
          <h2>{mode === "create" ? t("newCategory") : t("editCategory")}</h2>
          <button className="bo-form-modal-close" onClick={onClose}>✕</button>
        </div>
        <div className="bo-form-modal-body">
          {error && <div className="bo-form-error">{error}</div>}
          <div className="bo-form-field">
            <label htmlFor="a11y-input-1" className="bo-label">{t("name")} *</label>
            <input id="a11y-input-1" className="bo-input" value={name} onChange={e => setName(e.target.value)} placeholder={t("categoryName")} ref={nameRef} />
          </div>
          <div className="bo-row-two">
            <div className="bo-form-field">
              <label htmlFor="a11y-input-2" className="bo-label">{t("sortOrder")}</label>
              <input id="a11y-input-2" className="bo-input" type="number" min="0" step="1" value={sortOrder} onChange={e => setSortOrder(parseInt(e.target.value) || 0)} />
            </div>
            <div className="bo-form-field">
              <label htmlFor="a11y-input-3" className="bo-label">{t("parentCategory")}</label>
              <input id="a11y-input-3" className="bo-input" value={parentId} onChange={e => setParentId(e.target.value)} placeholder={t("none")} />
            </div>
          </div>
          {mode === "edit" && (
            <label htmlFor="a11y-input-4" className="bo-checkbox-label" style={{marginTop: 10}}>
              <input id="a11y-input-4" type="checkbox" checked={isActive} onChange={e => setIsActive(e.target.checked)} />{t("active")}
            </label>
          )}
        </div>
        <div className="bo-form-modal-footer">
          <button className="btn-secondary" onClick={onClose}>{t("cancel")}</button>
          <button className="btn-primary" onClick={save} disabled={saving}>{saving ? t("saving") : t("saveCategory")}</button>
        </div>
      </div>
    </button>
  );
}
