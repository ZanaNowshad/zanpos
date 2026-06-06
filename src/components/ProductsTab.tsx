import { useCallback, useEffect, useState } from "react";
import type { AdminProduct, CategoryRow, GhostSummary, ProductPrefill, TaxRuleRow } from "../types";
import { DEVICE } from "../types";
import { formatMoney } from "../money";
import * as cmd from "../tauri/commands";
import BarcodesPrintModal from "./BarcodesPrintModal";
import BulkImportModal from "./BulkImportModal";
import GhostBarcodesPanel from "./GhostBarcodesPanel";
import ProductFormModal from "./ProductFormModal";

interface Props {
  sessionUserId: string;
  ghostSummary?: GhostSummary;
  ghostPrefill?: ProductPrefill | null;
  onGhostPrefillConsumed?: () => void;
  onGhostCountChange?: () => void;
  onCreateProductFromGhost?: (prefill: ProductPrefill) => void;
}

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
  const [search, setSearch]           = useState("");
  const [searchInput, setSearchInput] = useState(""); // debounced into `search`
  const [printProducts, setPrintProducts] = useState<AdminProduct[] | null>(null);
  const [showBulkImport, setShowBulkImport] = useState(false);

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
      const page = await cmd.adminListProducts(sessionUserId, { search: q, offset: off, limit: PAGE_SIZE });
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
    Promise.all([cmd.adminListCategories(sessionUserId), cmd.adminListTaxRules(sessionUserId)])
      .then(([cats, taxes]) => {
        setCategories(cats.filter(c => c.is_active));
        setTaxRules(taxes);
      });
  }, []);

  // Open create form with ghost barcode prefill data
  useEffect(() => {
    if (!ghostPrefill) return;
    setCreating(true);
    setSelected(null);
    onGhostPrefillConsumed?.();
  }, [ghostPrefill]); // eslint-disable-line react-hooks/exhaustive-deps

  function startCreate() {
    setSelected(null);
    setCreating(true);
  }

  function startEdit(p: AdminProduct) {
    setCreating(false);
    setSelected(p);
  }

  function cancelEdit() {
    setSelected(null);
    setCreating(false);
  }

  const showingForm = creating || selected !== null;

  async function refreshProducts() {
    await fetchProducts(search, 0);
    setOffset(0);
  }

  return (
    <>
    {printProducts && <BarcodesPrintModal products={printProducts} onClose={() => setPrintProducts(null)} />}
    {showBulkImport && <BulkImportModal mode="products" sessionUserId={sessionUserId} onClose={() => setShowBulkImport(false)} onDone={refreshProducts} />}

    {/* Product form modal — widget style */}
    {showingForm && (
      <ProductFormModal
        mode={creating ? "create" : "edit"}
        product={selected}
        prefilledName={ghostPrefill?.name}
        prefilledBarcode={ghostPrefill?.barcode}
        categories={categories}
        taxRules={taxRules}
        sessionUserId={sessionUserId}
        onClose={cancelEdit}
        onSaved={() => { cancelEdit(); refreshProducts(); }}
      />
    )}

    <div className="bo-tab-layout">
      {ghostSummary && (ghostSummary.pending + ghostSummary.found + ghostSummary.not_found) > 0 && (
        <GhostBarcodesPanel
          sessionUserId={sessionUserId} summary={ghostSummary}
          onCreateProduct={(prefill) => onCreateProductFromGhost?.(prefill)}
          onCountChange={() => onGhostCountChange?.()}
        />
      )}
      {/* ── Full-width product list ── */}
      <div className="bo-list-pane bo-list-full">
        <div className="bo-list-header">
          <input className="bo-search" placeholder="Search products…" value={searchInput} onChange={e => setSearchInput(e.target.value)} />
          <button className="btn-secondary" onClick={() => setPrintProducts(products)} title="Print barcode labels">Labels</button>
          <button className="btn-secondary" onClick={() => setShowBulkImport(true)} title="Bulk import CSV">Import</button>
          <button className="btn-primary" onClick={startCreate}>+ New Product</button>
        </div>
        {total > 0 && (
          <div className="bo-pagination">
            <span className="bo-pagination-info">{loading ? "Loading…" : `${offset + 1}–${Math.min(offset + products.length, total)} of ${total.toLocaleString()}`}</span>
            <button className="bo-pagination-btn" disabled={offset === 0 || loading} onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}>‹ Prev</button>
            <button className="bo-pagination-btn" disabled={offset + PAGE_SIZE >= total || loading} onClick={() => setOffset(offset + PAGE_SIZE)}>Next ›</button>
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
              <button className="btn-secondary bo-label-btn" onClick={e => { e.stopPropagation(); setPrintProducts([p]); }}>Label</button>
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
    </div>
    </>
  );
}
