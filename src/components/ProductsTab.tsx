import { useCallback, useEffect, useMemo, useState } from "react";
import type { AdminProduct, CategoryRow, ProductPrefill, TaxRuleRow } from "../types";
import { DEVICE } from "../types";
import { formatMoney } from "../money";
import * as cmd from "../tauri/commands";
import BarcodesPrintModal from "./BarcodesPrintModal";
import BulkImportModal from "./BulkImportModal";
import DuplicateProductsModal from "./DuplicateProductsModal";
import ProductFormModal from "./ProductFormModal";
import { useLanguage } from "../hooks/useLanguage";
import { modalTranslator } from "../i18n/modalStrings";

interface Props {
  sessionUserId: string;
  /** Pre-fill data to open the create form with (e.g. from a POS notification). */
  prefill?: ProductPrefill | null;
  onPrefillConsumed?: () => void;
}

const PAGE_SIZE = 100;

export default function ProductsTab({
  sessionUserId,
  prefill,
  onPrefillConsumed,
}: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => modalTranslator(language), [language]);
  const [products, setProducts]       = useState<AdminProduct[]>([]);
  const [total, setTotal]             = useState(0);
  const [offset, setOffset]           = useState(0);
  const [loading, setLoading]         = useState(false);
  const [categories, setCategories]   = useState<CategoryRow[]>([]);
  const [taxRules, setTaxRules]       = useState<TaxRuleRow[]>([]);
  const [selected, setSelected]       = useState<AdminProduct | null>(null);
  const [creating, setCreating]       = useState(false);
  // Local copy of ghost prefill — captured before the parent clears productPrefill.
  const [localPrefill, setLocalPrefill] = useState<ProductPrefill | null>(null);
  const [search, setSearch]           = useState("");
  const [searchInput, setSearchInput] = useState(""); // debounced into `search`
  const [printProducts, setPrintProducts] = useState<AdminProduct[] | null>(null);
  const [showBulkImport, setShowBulkImport] = useState(false);
  const [showDupModal, setShowDupModal] = useState(false);
  const [duplicateCount, setDuplicateCount] = useState<number | null>(null);

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
  }, [sessionUserId]);

  useEffect(() => {
    fetchProducts(search, offset);
  }, [search, offset]); 

  useEffect(() => {
    Promise.all([cmd.adminListCategories(sessionUserId), cmd.adminListTaxRules(sessionUserId)])
      .then(([cats, taxes]) => {
        setCategories(cats.filter(c => c.is_active));
        setTaxRules(taxes);
      });
  }, [sessionUserId]);

  const scanDuplicateCount = useCallback(async () => {
    try {
      const groups = await cmd.adminFindDuplicateProducts(sessionUserId, false);
      setDuplicateCount(groups.reduce((n, g) => n + Math.max(0, g.products.length - 1), 0));
    } catch {
      setDuplicateCount(null);
    }
  }, [sessionUserId]);

  useEffect(() => {
    scanDuplicateCount();
  }, [scanDuplicateCount]);

  // Open the create form with prefilled data (e.g. handed off from a POS
  // notification). Capture it locally first so the form still has the data even
  // though the parent clears its prefill in the same render batch.
  useEffect(() => {
    if (!prefill) return;
    setLocalPrefill(prefill);
    setCreating(true);
    setSelected(null);
    onPrefillConsumed?.();
  }, [prefill]); 

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
    setLocalPrefill(null);
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

    {/* Product form modal — full-screen overlay */}
    {showingForm && (
      <ProductFormModal
        mode={creating ? "create" : "edit"}
        product={selected}
        prefilledName={localPrefill?.name}
        prefilledBarcode={localPrefill?.barcode}
        categories={categories}
        taxRules={taxRules}
        sessionUserId={sessionUserId}
        onClose={cancelEdit}
        onSaved={() => { cancelEdit(); refreshProducts(); }}
      />
    )}

    {/* Duplicate-products triage — scans the catalog, merges or archives dupes */}
    {showDupModal && (
      <DuplicateProductsModal
        sessionUserId={sessionUserId}
        onClose={() => setShowDupModal(false)}
        onResolved={() => { refreshProducts(); scanDuplicateCount(); }}
      />
    )}

    <div className="bo-tab-layout">
      {/* ── Full-width product list ── */}
      <div className="bo-list-pane bo-list-full">
        <div className="bo-list-header">
          <input className="bo-search" placeholder={t("searchProducts")} value={searchInput} onChange={e => setSearchInput(e.target.value)} />
          <button className="btn-secondary" onClick={() => setShowDupModal(true)} title={t("duplicates")}>{t("duplicates")}</button>
          <button className="btn-secondary" onClick={() => setPrintProducts(products)} title={t("printLabels")}>{t("labels")}</button>
          <button className="btn-secondary" onClick={() => setShowBulkImport(true)} title={t("importAction")}>{t("importAction")}</button>
          <button className="btn-primary" onClick={startCreate}>+ {t("newProduct")}</button>
        </div>
        {duplicateCount !== null && duplicateCount > 0 && (
          <div className="product-integrity-banner">
            <div>
              <strong>{duplicateCount} {t("possibleDuplicatesFound")}</strong>
              <span>{t("duplicateReviewHint")}</span>
            </div>
            <button className="btn-primary" onClick={() => setShowDupModal(true)}>{t("reviewAndMerge")}</button>
          </div>
        )}
        {total > 0 && (
          <div className="bo-pagination">
            <span className="bo-pagination-info">{loading ? t("loading") : `${offset + 1}–${Math.min(offset + products.length, total)} ${t("of")} ${total.toLocaleString()}`}</span>
            <button className="bo-pagination-btn" disabled={offset === 0 || loading} onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}><span className="icon-directional" aria-hidden="true">‹</span> {t("previous")}</button>
            <button className="bo-pagination-btn" disabled={offset + PAGE_SIZE >= total || loading} onClick={() => setOffset(offset + PAGE_SIZE)}>{t("next")} <span className="icon-directional" aria-hidden="true">›</span></button>
          </div>
        )}
        <div className="bo-list">
          {loading && products.length === 0 && <div className="bo-empty">{t("loading")}</div>}
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
                  {!p.is_active && <span className="bo-badge-inactive">{t("inactive")}</span>}
                </div>
              </button>
              <button className="btn-secondary bo-label-btn" onClick={e => { e.stopPropagation(); setPrintProducts([p]); }}>{t("label")}</button>
            </div>
          ))}
          {!loading && products.length === 0 && (
            <div className="bo-empty">
              <div className="bo-empty-icon">📦</div>
              <p className="bo-empty-title">{t("noProductsFound")}</p>
              <p className="bo-empty-hint">{t("noProductsHint")}</p>
            </div>
          )}
        </div>
      </div>
    </div>
    </>
  );
}
