import { useCallback, useEffect, useMemo, useState } from "react";
import type { AdminProduct, CategoryRow, ProductPrefill, StockLevel, TaxRuleRow, SessionToken } from "../types";
import { DEVICE } from "../types";
import * as cmd from "../tauri/commands";
import type { ProductView } from "../tauri/commands";
import BarcodesPrintModal from "./BarcodesPrintModal";
import BulkImportModal from "./BulkImportModal";
import DuplicateProductsModal from "./DuplicateProductsModal";
import ProductFormModal from "./ProductFormModal";
import { useLanguage } from "../hooks/useLanguage";
import { modalTranslator } from "../i18n/modalStrings";
import { PageTemplate, EmptyState, LoadingSkeleton, DataTable, Toolbar } from "./templates";
import { exportProductCatalogue } from "../csv/exportProductCatalogue";
import { Package, Tag } from "lucide-react";
import { ProductImageSearchControl } from "./ProductImageSearchControl";
import { productCatalogueColumns } from "./productCatalogueColumns";
import { useProductImageFetch } from "./useProductImageFetch";

interface Props {
  sessionUserId: string;
  /** For inventory reads, which authenticate by session. */
  sessionToken: SessionToken;
  /** Pre-fill data to open the create form with (e.g. from a POS notification). */
  prefill?: ProductPrefill | null;
  onPrefillConsumed?: () => void;
}

const PAGE_SIZE = 100;

export default function ProductsTab({
  sessionUserId,
  sessionToken,
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
  const [categoryFilter, setCategoryFilter] = useState("");
  /* Saved views replace the old client-side status filter. That filter ran
     over the loaded page, so "Inactive" answered "which of these hundred are
     inactive" while the count beside it said 28,032 — two different questions
     on one line. The view goes to the server with the query. */
  const [view, setView] = useState<ProductView>("all");
  /** Stock is a separate command; joined by product_id. Null = unavailable. */
  const [stockByProduct, setStockByProduct] = useState<Map<string, StockLevel> | null>(null);
  const [loadError, setLoadError]           = useState<string | null>(null);
  const [exporting, setExporting]           = useState(false);
  const [exportError, setExportError]       = useState<string | null>(null);

  const { imageSearchState, bulkImages, fetchProductImage, fetchMissingImages, stopBulk } =
    useProductImageFetch({
      sessionUserId,
      products,
      failureLabel: t("imageSearchFailed"),
      onImageSaved: (productId, imageUrl) => setProducts(current =>
        current.map(row => (row.product_id === productId ? { ...row, image_path: imageUrl } : row))),
    });

  const exp = DEVICE.currency_exponent;
  const cur = DEVICE.currency;

  // Debounce search input → `search` state (300 ms)
  useEffect(() => {
    const t = setTimeout(() => { setSearch(searchInput); setOffset(0); }, 300);
    return () => clearTimeout(t);
  }, [searchInput]);

  const fetchProducts = useCallback(async (q: string, off: number, categoryId: string, v: ProductView) => {
    setLoading(true);
    setLoadError(null);
    try {
      const page = await cmd.adminListProducts(sessionUserId, {
        search: q,
        categoryId: categoryId || undefined,
        view: v === "all" ? undefined : v,
        offset: off,
        limit: PAGE_SIZE,
      });
      setProducts(page.items);
      setTotal(page.total);
      setOffset(off);
    } catch (e) {
      setLoadError(e instanceof Error ? e.message : String(e));
      setProducts([]);
      setTotal(0);
    } finally {
      setLoading(false);
    }
  }, [sessionUserId]);

  // Stock lives behind its own command. If it is unavailable the catalogue is
  // still fully usable — the column degrades to "—" rather than failing the page.
  useEffect(() => {
    let cancelled = false;
    cmd.inventoryGetLevels(sessionToken)
      .then(levels => {
        if (cancelled) return;
        setStockByProduct(new Map(levels.map(l => [l.product_id, l])));
      })
      .catch(() => { if (!cancelled) setStockByProduct(null); });
    return () => { cancelled = true; };
  }, [sessionUserId, sessionToken]);

  useEffect(() => {
    fetchProducts(search, offset, categoryFilter, view);
  }, [search, offset, categoryFilter, view, fetchProducts]);

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
    // Deliberately keyed on `prefill` alone: this effect calls back into the
    // parent to clear the prefill, so depending on that callback would re-run
    // it on every parent render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
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
    await fetchProducts(search, 0, categoryFilter, view);
    setOffset(0);
  }

  /** Rows on this page that could take an image but have none. */
  const missingImageCount = products.filter(
    p => !p.image_path?.trim() && p.barcode?.trim(),
  ).length;

  const hasQuery = search.trim() !== "" || categoryFilter !== "" || view !== "all";

  function clearQuery() {
    setSearchInput("");
    setSearch("");
    setCategoryFilter("");
    setView("all");
    setOffset(0);
  }


  const exportProducts = useCallback(async () => {
    setExporting(true);
    setExportError(null);
    try {
      await exportProductCatalogue({
        sessionUserId, search, categoryFilter, view,
        currencyExponent: exp, pageSize: PAGE_SIZE,
      });
    } catch (e) {
      setExportError(e instanceof Error ? e.message : String(e));
    } finally {
      setExporting(false);
    }
  }, [sessionUserId, search, categoryFilter, view, exp]);

  /**
   * Catalogue counts.
   *
   * `total` is the server's count for the whole filtered catalogue. The low and
   * out-of-stock figures are counted over the loaded page only, because stock
   * arrives from a separate command joined by product_id and there is no
   * server-side aggregate for it — so they are labelled "on this page" rather
   * than presented as store-wide totals. The reference's "Catalogue Health"
   * score is deliberately absent: no backend computes it.
   */
  const stockCounts = useMemo(() => {
    if (!stockByProduct) return null;
    let low = 0, out = 0;
    for (const p of products) {
      const level = stockByProduct.get(p.product_id);
      if (!level) continue;
      const qty = Number(level.quantity_on_hand);
      if (!Number.isFinite(qty)) continue;
      if (qty <= 0) out += 1;
      else if (qty <= p.reorder_point) low += 1;
    }
    return { low, out };
  }, [products, stockByProduct]);

  /* No client-side filter step any more: the view is part of the query, so the
     rows returned are the rows to show and `total` counts the same set. The
     old pair disagreed — the list was filtered, the total was not. */
  const visibleProducts = products;

  const columns = useMemo(
    () => productCatalogueColumns(t, cur, exp, stockByProduct),
    [t, cur, exp, stockByProduct],
  );

  const rangeLabel = total > 0
    ? `${offset + 1}–${Math.min(offset + visibleProducts.length, total)} ${t("of")} ${total.toLocaleString()}`
    : "";

  return (
    <>
    {printProducts && <BarcodesPrintModal products={printProducts} onClose={() => setPrintProducts(null)} />}
    {showBulkImport && <BulkImportModal mode="products" sessionUserId={sessionUserId} onClose={() => setShowBulkImport(false)} onDone={refreshProducts} />}

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

    {showDupModal && (
      <DuplicateProductsModal
        sessionUserId={sessionUserId}
        onClose={() => setShowDupModal(false)}
        onResolved={() => { refreshProducts(); scanDuplicateCount(); }}
      />
    )}

    <PageTemplate
      contentFlat
      header={{
        /* No subtitle: "Manage your products, prices, stock levels and
           availability" describes the Products tab to someone who just
           pressed the Products tab. Dropping it lets the header band merge
           into the toolbar and gives the table back ~85px of a 768px screen. */
        title: t("products"),
        primaryAction: { label: `+ ${t("newProduct")}`, onClick: startCreate },
        secondaryActions: [
          {
            label: bulkImages
              ? `${t("fetchingImages")} ${bulkImages.done}/${bulkImages.total}`
              : t("fetchMissingImages"),
            onClick: () => {
              if (bulkImages) { stopBulk(); return; }
              void fetchMissingImages();
            },
            disabled: missingImageCount === 0 && !bulkImages,
          },
          { label: t("duplicates"), onClick: () => setShowDupModal(true) },
          { label: t("labels"), onClick: () => setPrintProducts(products) },
          { label: t("importAction"), onClick: () => setShowBulkImport(true) },
          {
            label: exporting ? t("exporting") : t("exportAction"),
            onClick: () => { void exportProducts(); },
            disabled: exporting || total === 0,
          },
        ],
      }}
      degraded={exportError ? {
        severity: "warning",
        message: `${t("exportFailed")} ${exportError}`,
        onDismiss: () => setExportError(null),
      } : undefined}
      toolbar={
        <Toolbar
          search={{
            value: searchInput,
            onChange: setSearchInput,
            placeholder: t("searchProducts"),
            label: t("searchProducts"),
          }}
          count={rangeLabel}
          onClear={hasQuery ? clearQuery : undefined}
          clearLabel={t("clearFilters")}
          filters={
            <>
              <select
                className="zp-filter"
                value={categoryFilter}
                aria-label={t("category")}
                onChange={e => { setCategoryFilter(e.target.value); setOffset(0); }}
              >
                <option value="">{t("allCategories")}</option>
                {categories.map(c => (
                  <option key={c.category_id} value={c.category_id}>{c.name}</option>
                ))}
              </select>
              {/* Saved views. The recurring questions a manager opens this
                  page to ask, as one control instead of a status dropdown that
                  could only answer one of them — and only for the loaded page. */}
              <select
                className="zp-filter"
                value={view}
                aria-label={t("savedView")}
                onChange={e => { setView(e.target.value as ProductView); setOffset(0); }}
              >
                <option value="all">{t("allProducts")}</option>
                <option value="out_of_stock">{t("viewOutOfStock")}</option>
                <option value="low_stock">{t("viewLowStock")}</option>
                <option value="no_barcode">{t("viewNoBarcode")}</option>
                <option value="no_image">{t("viewNoImage")}</option>
                <option value="active">{t("active")}</option>
                <option value="inactive">{t("inactive")}</option>
              </select>
            </>
          }
        />
      }
    >
      {bulkImages && bulkImages.done === bulkImages.total && bulkImages.failed > 0 && (
        <div className="product-integrity-banner">
          <div>
            <strong>{bulkImages.total - bulkImages.failed} {t("bulkImagesDone")}</strong>
            <span>{bulkImages.failed} {t("bulkImagesFailed")}</span>
          </div>
        </div>
      )}

      {duplicateCount !== null && duplicateCount > 0 && (
        <div className="product-integrity-banner">
          <div>
            <strong>{duplicateCount} {t("possibleDuplicatesFound")}</strong>
            <span>{t("duplicateReviewHint")}</span>
          </div>
          <button className="btn-primary" onClick={() => setShowDupModal(true)}>{t("reviewAndMerge")}</button>
        </div>
      )}

      {/* Which empty state to show is decided from application state, never
          from how the page happens to look. */}
      {/* Catalogue counts. Total is the server's figure for the whole filtered
          catalogue; stock counts are page-scoped and say so, because stock has
          no server-side aggregate. No composite "health score" — nothing
          computes one. */}
      <div className="zp-cat-counts">
        <div>
          <span>{t("totalProducts")}</span>
          <strong className="zp-numeric">{total.toLocaleString()}</strong>
        </div>
        <div>
          <span>{t("lowStockLabel")}</span>
          {stockCounts
            ? <strong className="zp-numeric zp-cat-warn">{stockCounts.low}<small>{t("onThisPage")}</small></strong>
            : <strong className="zp-status-muted">{t("stockUnavailable")}</strong>}
        </div>
        <div>
          <span>{t("outOfStockLabel")}</span>
          {stockCounts
            ? <strong className="zp-numeric zp-cat-danger">{stockCounts.out}<small>{t("onThisPage")}</small></strong>
            : <strong className="zp-status-muted">{t("stockUnavailable")}</strong>}
        </div>
      </div>

      {loading && products.length === 0 ? (
        <LoadingSkeleton variant="table" count={8} />
      ) : loadError ? (
        <EmptyState
          variant="degraded"
          title={t("couldNotLoadProducts")}
          description={loadError}
          stillWorks={t("sellingUnaffected")}
          actions={[{ label: t("retry"), onClick: () => refreshProducts(), primary: true }]}
        />
      ) : visibleProducts.length === 0 && hasQuery ? (
        <EmptyState
          variant="no-results"
          title={t("noMatchingProducts")}
          description={t("noMatchingProductsHint")}
          actions={[{ label: t("clearFilters"), onClick: clearQuery, primary: true }]}
        />
      ) : visibleProducts.length === 0 ? (
        <EmptyState
          variant="first-use"
          icon={<Package size={32} strokeWidth={1.5} />}
          title={t("addFirstProduct")}
          description={t("addFirstProductHint")}
          actions={[
            { label: `+ ${t("newProduct")}`, onClick: startCreate, primary: true },
            { label: t("importAction"), onClick: () => setShowBulkImport(true) },
          ]}
        />
      ) : (
        <>
          <DataTable
            caption={t("products")}
            columns={columns}
            rows={visibleProducts}
            rowKey={p => p.product_id}
            onRowClick={startEdit}
            isRowActive={p => selected?.product_id === p.product_id}
            isRowMuted={p => !p.is_active}
            rowAction={p => (
              <div className="product-catalogue-actions">
                <ProductImageSearchControl
                  compact
                  showPreview={false}
                  productName={p.name}
                  barcode={p.barcode}
                  sku={p.sku}
                  categoryName={p.category_name}
                  imagePath={p.image_path}
                  fetchLabel={t("fetchImage")}
                  changeLabel={t("changeImage")}
                  searchingLabel={t("findingImage")}
                  evidenceLabel={t("imageSearchEvidence")}
                  noImageLabel={t("noImage")}
                  loading={imageSearchState[p.product_id]?.loading}
                  error={imageSearchState[p.product_id]?.error}
                  onSearch={() => { void fetchProductImage(p); }}
                />
                <button
                  type="button"
                  className="btn-secondary zp-row-action"
                  onClick={() => setPrintProducts([p])}
                  aria-label={`${t("label")}: ${p.name}`}
                >
                  <Tag size={14} aria-hidden="true" />
                  <span className="zp-action-label">{t("label")}</span>
                </button>
              </div>
            )}
          />
          {total > PAGE_SIZE && (
            <div className="bo-pagination">
              <span className="bo-pagination-info">{loading ? t("loading") : rangeLabel}</span>
              <button className="bo-pagination-btn" disabled={offset === 0 || loading} onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}><span className="icon-directional" aria-hidden="true">‹</span> {t("previous")}</button>
              <button className="bo-pagination-btn" disabled={offset + PAGE_SIZE >= total || loading} onClick={() => setOffset(offset + PAGE_SIZE)}>{t("next")} <span className="icon-directional" aria-hidden="true">›</span></button>
            </div>
          )}
        </>
      )}
    </PageTemplate>
    </>
  );
}
