import { useEffect, useState } from "react";
import type { CategoryRow } from "../types";
import * as cmd from "../tauri/commands";
import BulkImportModal from "./BulkImportModal";
import CategoryFormModal from "./CategoryFormModal";
import { PageTemplate, EmptyState, LoadingSkeleton } from "./templates";
import { Tag } from "lucide-react";

interface Props { sessionUserId: string; }

export default function CategoriesTab({ sessionUserId }: Props) {
  const [categories, setCategories] = useState<CategoryRow[]>([]);
  const [loading, setLoading]       = useState(true);
  const [selected, setSelected]     = useState<CategoryRow | null>(null);
  const [creating, setCreating]     = useState(false);
  const [showBulkImport, setShowBulkImport] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    cmd.adminListCategories(sessionUserId).then(data => {
      if (!cancelled) { setCategories(data); setLoading(false); }
    });
    return () => { cancelled = true; };
  }, [sessionUserId]);

  function startCreate() { setSelected(null); setCreating(true); }
  function startEdit(c: CategoryRow) { setCreating(false); setSelected(c); }
  function cancelEdit() { setSelected(null); setCreating(false); }

  const showingForm = creating || selected !== null;

  async function refreshCategories() {
    setLoading(true);
    const cats = await cmd.adminListCategories(sessionUserId);
    setCategories(cats);
    setLoading(false);
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

    <PageTemplate
      header={{
        title: "Categories",
        icon: <Tag size={18} strokeWidth={1.7} aria-hidden="true" />,
        primaryAction: { label: "+ New Category", onClick: startCreate },
        secondaryActions: [
          { label: "Import", onClick: () => setShowBulkImport(true) },
        ],
      }}
    >
      {loading && categories.length === 0 ? (
        <LoadingSkeleton variant="table" count={4} />
      ) : categories.length === 0 ? (
        <EmptyState
          icon={<Tag size={36} strokeWidth={1.5} />}
          title="No categories"
          description="Create product categories to organise your catalogue."
          actions={[{ label: "+ New Category", onClick: startCreate, primary: true }]}
        />
      ) : (
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
        </div>
      )}
    </PageTemplate>
    </>
  );
}
