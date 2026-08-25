import { useMemo, useState } from "react";
import type { CategoryRow } from "../types";
import * as cmd from "../tauri/commands";
import { useAutoFocus } from "../hooks/useAutoFocus";
import { useLanguage } from "../hooks/useLanguage";
import { modalTranslator } from "../i18n/modalStrings";
import ModalShell from "./modal/ModalShell";
import { Field, ModalActions, ModalError, ModalSection, ModalToggle } from "./modal/ModalParts";

interface Props {
  mode: "create" | "edit";
  category?: CategoryRow | null;
  nextOrder: number;
  /** The rest of the tree, so the parent field can be a choice rather than an id. */
  categories: CategoryRow[];
  sessionUserId: string;
  onClose: () => void;
  onSaved: () => void;
}

export default function CategoryFormModal({
  mode, category, nextOrder, categories, sessionUserId, onClose, onSaved,
}: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => modalTranslator(language), [language]);
  const nameRef = useAutoFocus<HTMLInputElement>();

  const [name, setName] = useState(category?.name ?? "");
  const [sortOrder, setSortOrder] = useState(category?.sort_order ?? nextOrder);
  const [isActive, setIsActive] = useState(category?.is_active ?? true);
  const [parentId, setParentId] = useState(category?.parent_category_id ?? "");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  /* A category cannot be its own parent. The field used to be a free-text box
     asking for a category *id*, so this was not enforceable and not usable —
     nobody knows a ULID by heart. */
  const parentOptions = categories.filter(row => row.category_id !== category?.category_id);

  async function save() {
    if (!name.trim()) { setError(t("nameRequired")); return; }
    setSaving(true);
    setError(null);
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
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : t("saveFailed"));
    } finally {
      setSaving(false);
    }
  }

  return (
    <ModalShell
      kicker="Catalogue"
      title={mode === "create" ? t("newCategory") : t("editCategory")}
      subtitle="Categories group products for the till's quick keys, the storefront and every stock report."
      size="md"
      onClose={onClose}
      footer={
        <ModalActions note={<ModalError message={error} />}>
          <button type="button" className="btn-secondary" onClick={onClose}>{t("cancel")}</button>
          <button type="button" className="btn-primary" onClick={save} disabled={saving}>
            {saving ? t("saving") : t("saveCategory")}
          </button>
        </ModalActions>
      }
    >
      <ModalSection title="Details" columns={2}>
        <Field label={t("name")} required wide>
          {id => (
            <input
              id={id}
              ref={nameRef}
              value={name}
              placeholder={t("categoryName")}
              onChange={event => setName(event.target.value)}
            />
          )}
        </Field>

        <Field label={t("sortOrder")} hint="Lower numbers come first on the till.">
          {id => (
            <input
              id={id}
              type="number"
              min="0"
              step="1"
              value={sortOrder}
              onChange={event => setSortOrder(parseInt(event.target.value) || 0)}
            />
          )}
        </Field>

        <Field label={t("parentCategory")} hint="Leave as None for a top-level category.">
          {id => (
            <select id={id} value={parentId} onChange={event => setParentId(event.target.value)}>
              <option value="">{t("none")}</option>
              {parentOptions.map(row => (
                <option key={row.category_id} value={row.category_id}>{row.name}</option>
              ))}
            </select>
          )}
        </Field>
      </ModalSection>

      {mode === "edit" && (
        <ModalSection title="Visibility">
          <ModalToggle
            label={t("active")}
            hint="An inactive category is hidden from the till and the storefront. Its products keep their category and are not deleted."
            checked={isActive}
            onChange={setIsActive}
          />
        </ModalSection>
      )}
    </ModalShell>
  );
}
