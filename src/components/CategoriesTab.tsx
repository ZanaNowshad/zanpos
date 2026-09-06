import { useEffect, useState } from "react";
import type { CategoryRow } from "../types";
import * as cmd from "../tauri/commands";
import BulkImportModal from "./BulkImportModal";
import CategoryFormModal from "./CategoryFormModal";
import { PageTemplate, EmptyState, LoadingSkeleton, DataTable } from "./templates";
import { Pencil, Tag } from "lucide-react";
import type { SessionToken } from "../types";

interface Props { sessionToken: SessionToken; }

export default function CategoriesTab({ sessionToken }: Props) {
  const [categories, setCategories] = useState<CategoryRow[]>([]);
  const [loading, setLoading]       = useState(true);
  const [selected, setSelected]     = useState<CategoryRow | null>(null);
  const [creating, setCreating]     = useState(false);
  const [showBulkImport, setShowBulkImport] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    cmd.adminListCategories(sessionToken).then(data => {
      if (!cancelled) { setCategories(data); setLoading(false); }
    });
    return () => { cancelled = true; };
  }, [sessionToken]);

  function startCreate() { setSelected(null); setCreating(true); }
  function startEdit(c: CategoryRow) { setCreating(false); setSelected(c); }
  function cancelEdit() { setSelected(null); setCreating(false); }

  const showingForm = creating || selected !== null;

  async function refreshCategories() {
    setLoading(true);
    const cats = await cmd.adminListCategories(sessionToken);
    setCategories(cats);
    setLoading(false);
  }

  return (
    <>
    {showBulkImport && <BulkImportModal mode="categories" sessionToken={sessionToken} onClose={() => setShowBulkImport(false)} onDone={refreshCategories} />}

    {showingForm && (
      <CategoryFormModal
        mode={creating ? "create" : "edit"}
        category={selected}
        nextOrder={Math.max(0, ...categories.map(c => c.sort_order)) + 1}
        categories={categories}
        sessionToken={sessionToken}
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
        /* The same table every other list in the back office uses. This page
           was a bare list of names in an otherwise empty panel: no columns, no
           product counts, no status a manager could scan, and a row so tall
           that six categories filled the screen. Reusing DataTable brings the
           column priorities, the container-query narrowing and the 40px touch
           targets with it, for free. */
        <DataTable
          caption="Product categories"
          columns={[
            {
              id: "name",
              header: "Category",
              cell: c => (
                <span className="zp-cell-primary">{c.name}</span>
              ),
            },
            {
              id: "products",
              header: "Products",
              align: "end",
              numeric: true,
              width: "110px",
              // The question actually asked of a category list: is anything in
              // it? An empty category is usually a mistake or a leftover.
              cell: c => c.product_count > 0
                ? c.product_count
                : <span className="zp-status-muted">0</span>,
            },
            {
              id: "order",
              header: "Sort",
              align: "end",
              numeric: true,
              width: "84px",
              priority: 2,
              cell: c => c.sort_order,
            },
            {
              id: "status",
              header: "Status",
              width: "120px",
              priority: 2,
              cell: c => c.is_active
                ? <span className="zp-status zp-status-ok">Active</span>
                : <span className="zp-status zp-status-muted">Inactive</span>,
            },
          ]}
          rows={categories}
          rowKey={c => c.category_id}
          onRowClick={startEdit}
          isRowActive={c => selected?.category_id === c.category_id}
          isRowMuted={c => !c.is_active}
          rowAction={c => (
            <button
              type="button"
              className="btn-secondary zp-row-action"
              onClick={() => startEdit(c)}
              aria-label={`Edit ${c.name}`}
            >
              <Pencil size={14} aria-hidden="true" />
              <span className="zp-action-label">Edit</span>
            </button>
          )}
        />
      )}
    </PageTemplate>
    </>
  );
}
