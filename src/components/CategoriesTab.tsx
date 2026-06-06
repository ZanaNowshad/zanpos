import { useEffect, useState } from "react";
import type { CategoryRow } from "../types";
import * as cmd from "../tauri/commands";
import BulkImportModal from "./BulkImportModal";
import CategoryFormModal from "./CategoryFormModal";

interface Props { sessionUserId: string; }

export default function CategoriesTab({ sessionUserId }: Props) {
  const [categories, setCategories] = useState<CategoryRow[]>([]);
  const [selected, setSelected]     = useState<CategoryRow | null>(null);
  const [creating, setCreating]     = useState(false);
  const [showBulkImport, setShowBulkImport] = useState(false);

  useEffect(() => {
    let cancelled = false;
    cmd.adminListCategories(sessionUserId).then(data => { if (!cancelled) setCategories(data); });
    return () => { cancelled = true; };
  }, []);

  function startCreate() { setSelected(null); setCreating(true); }
  function startEdit(c: CategoryRow) { setCreating(false); setSelected(c); }
  function cancelEdit() { setSelected(null); setCreating(false); }

  const showingForm = creating || selected !== null;

  async function refreshCategories() {
    const cats = await cmd.adminListCategories(sessionUserId);
    setCategories(cats);
  }

  return (
    <>
    {showBulkImport && <BulkImportModal mode="categories" sessionUserId={sessionUserId} onClose={() => setShowBulkImport(false)} onDone={refreshCategories} />}

    {showingForm && (
      <CategoryFormModal
        mode={creating ? "create" : "edit"}
        category={selected}
        nextOrder={Math.max(0, ...categories.map(c => c.sort_order)) + 1}
        sessionUserId={sessionUserId}
        onClose={cancelEdit}
        onSaved={() => { cancelEdit(); refreshCategories(); }}
      />
    )}

    <div className="bo-tab-layout">
      <div className="bo-list-pane bo-list-full">
        <div className="bo-list-header">
          <span className="bo-list-title">Categories</span>
          <button className="btn-secondary" onClick={() => setShowBulkImport(true)} title="Bulk import CSV">Import</button>
          <button className="btn-primary" onClick={startCreate}>+ New Category</button>
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
    </div>
    </>
  );
}
