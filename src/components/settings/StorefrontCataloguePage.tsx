import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  LoaderCircle,
  MessageCircle,
  QrCode,
  Search,
  Sparkles,
} from "lucide-react";
import type {
  StorefrontProduct,
  StorefrontProductUpdate,
  StorefrontPublishResult,
  StorefrontSettings,
  StorefrontStatus,
} from "../../storefront/types";
import {
  clearPublishNudgeDismissal,
  formatStorefrontMoney,
  getPublishNudgeStorage,
  getStorefrontReadiness,
  nextStorefrontOffset,
  previousStorefrontOffset,
  publishNudgeWakeDelay,
  readPublishNudgeDismissal,
  shouldShowPublishNudge,
  storefrontPageRange,
  writePublishNudgeDismissal,
} from "../../storefront/storefrontUtils";
import * as storefront from "../../tauri/storefront";
import StorefrontReadinessSummary from "./StorefrontReadinessSummary";

interface Props {
  sessionUserId: string;
  settings: StorefrontSettings;
}

const PAGE_SIZE = 25;

function messageFrom(error: unknown, fallback: string) {
  return typeof error === "string"
    ? error
    : error instanceof Error ? error.message : fallback;
}

function releaseTime(value: string | null) {
  if (!value) return "Not published yet";
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(value));
}

export default function StorefrontCataloguePage({ sessionUserId, settings }: Props) {
  const [status, setStatus] = useState<StorefrontStatus | null>(null);
  const [products, setProducts] = useState<StorefrontProduct[]>([]);
  const [previewProducts, setPreviewProducts] = useState<StorefrontProduct[]>([]);
  const [total, setTotal] = useState(0);
  const [offset, setOffset] = useState(0);
  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(true);
  const [publishing, setPublishing] = useState(false);
  const [updatingId, setUpdatingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<StorefrontPublishResult | null>(null);
  const [publishNudgeDismissedAt, setPublishNudgeDismissedAt] = useState<number | null>(null);
  const [publishNudgeNow, setPublishNudgeNow] = useState<number | null>(null);
  const requestSequence = useRef(0);

  const loadPage = useCallback(async (query: string, nextOffset: number) => {
    const sequence = ++requestSequence.current;
    setLoading(true);
    try {
      const page = await storefront.storefrontProductsList(sessionUserId, {
        search: query || undefined,
        offset: nextOffset,
        limit: PAGE_SIZE,
      });
      if (sequence !== requestSequence.current) return;
      setProducts(page.items);
      setTotal(page.total);
      setOffset(page.offset);
    } catch (loadError) {
      if (sequence === requestSequence.current) {
        setError(messageFrom(loadError, "The catalogue page could not be loaded."));
      }
    } finally {
      if (sequence === requestSequence.current) setLoading(false);
    }
  }, [sessionUserId]);

  const loadPreview = useCallback(async () => {
    try {
      const page = await storefront.storefrontProductsList(sessionUserId, {
        offset: 0,
        limit: 3,
        publishedOnly: true,
      });
      setPreviewProducts(page.items);
    } catch {
      setPreviewProducts([]);
    }
  }, [sessionUserId]);

  const loadStatus = useCallback(async () => {
    try {
      setStatus(await storefront.storefrontStatus(sessionUserId));
    } catch (loadError) {
      setError(messageFrom(loadError, "ZanShop publishing status could not be loaded."));
    }
  }, [sessionUserId]);

  useEffect(() => {
    void Promise.all([loadPreview(), loadStatus()]);
  }, [loadPreview, loadStatus]);

  useEffect(() => {
    const timer = globalThis.setTimeout(() => {
      setSearch(searchInput.trim());
      setOffset(0);
    }, 300);
    return () => globalThis.clearTimeout(timer);
  }, [searchInput]);

  useEffect(() => {
    void loadPage(search, offset);
  }, [loadPage, offset, search]);

  useEffect(() => {
    const browserStorage = getPublishNudgeStorage();
    setPublishNudgeDismissedAt(readPublishNudgeDismissal(browserStorage));
    setPublishNudgeNow(Date.now());
  }, []);

  useEffect(() => {
    if (status?.dirty_product_count !== 0) return;
    clearPublishNudgeDismissal(getPublishNudgeStorage());
    setPublishNudgeDismissedAt(null);
  }, [status?.dirty_product_count]);

  useEffect(() => {
    if (!settings.enabled || !status || status.dirty_product_count <= 0 || publishNudgeNow == null) {
      return;
    }
    const delay = publishNudgeWakeDelay(publishNudgeDismissedAt, publishNudgeNow);
    if (delay == null || delay <= 0) return;
    const timer = globalThis.setTimeout(() => setPublishNudgeNow(Date.now()), delay);
    return () => globalThis.clearTimeout(timer);
  }, [publishNudgeDismissedAt, publishNudgeNow, settings.enabled, status]);

  const readiness = status ? getStorefrontReadiness(settings, status) : null;
  const pageRange = useMemo(
    () => storefrontPageRange(offset, products.length, total),
    [offset, products.length, total],
  );
  const showPublishNudge = publishNudgeNow != null && status
    ? shouldShowPublishNudge({
        enabled: settings.enabled,
        dirtyProductCount: status.dirty_product_count,
        dismissedAtMs: publishNudgeDismissedAt,
        nowMs: publishNudgeNow,
      })
    : false;

  async function updateProduct(productId: string, update: StorefrontProductUpdate) {
    setUpdatingId(productId);
    setError(null);
    try {
      const updated = await storefront.storefrontProductUpdate(sessionUserId, productId, update);
      setProducts(current => current.map(item => item.product_id === productId ? updated : item));
      await Promise.all([loadStatus(), loadPreview()]);
      setResult(null);
    } catch (updateError) {
      setError(messageFrom(updateError, "The product could not be updated."));
      await loadPage(search, offset);
    } finally {
      setUpdatingId(null);
    }
  }

  async function publish() {
    setPublishing(true);
    setError(null);
    setResult(null);
    try {
      const publishResult = await storefront.storefrontPublish(sessionUserId);
      setResult(publishResult);
      await Promise.allSettled([
        loadStatus(),
        loadPage(search, offset),
        loadPreview(),
      ]);
      if (publishResult.success) {
        clearPublishNudgeDismissal(getPublishNudgeStorage());
        setPublishNudgeDismissedAt(null);
        setPublishNudgeNow(Date.now());
      }
    } catch (publishError) {
      setError(messageFrom(publishError, "Publishing failed. Your last live release is unchanged."));
    } finally {
      setPublishing(false);
    }
  }

  function patchProduct(productId: string, update: Partial<StorefrontProduct>) {
    setProducts(current => current.map(item =>
      item.product_id === productId ? { ...item, ...update } : item,
    ));
  }

  function dismissPublishNudge() {
    const dismissedAt = Date.now();
    writePublishNudgeDismissal(getPublishNudgeStorage(), dismissedAt);
    setPublishNudgeDismissedAt(dismissedAt);
    setPublishNudgeNow(dismissedAt);
  }

  if (!status && loading) {
    return <div className="sf-loading"><LoaderCircle aria-hidden="true" /> Loading catalogue…</div>;
  }

  return (
    <>
      {status && readiness && <StorefrontReadinessSummary readiness={readiness} status={status} />}
      {showPublishNudge && status && (
        <div className="sf-publish-nudge" role="status">
          <div>
            <strong>{status.dirty_product_count} catalogue {status.dirty_product_count === 1 ? "change is" : "changes are"} waiting</strong>
            <span>Publish when you are ready to bring ZanShop up to date.</span>
          </div>
          <button type="button" onClick={dismissPublishNudge}>Remind me next week</button>
        </div>
      )}

      <div className="sf-workspace">
        <main className="sf-controls">
          <section className="sf-section sf-products" aria-labelledby="sf-products-title">
            <div className="sf-section-heading">
              <div><span>Catalogue</span><h3 id="sf-products-title">Products customers can browse</h3></div>
              {status && <span className="sf-count">{status.published_product_count} of {status.eligible_product_count} shown</span>}
            </div>
            <label className="sf-search">
              <Search aria-hidden="true" />
              <span className="sr-only">Search ZanShop products</span>
              <input value={searchInput} onChange={event => setSearchInput(event.target.value)} placeholder="Search English or Arabic names" />
            </label>
            {total > 0 && (
              <div className="sf-pagination">
                <span>{loading ? "Loading…" : `${pageRange.start}–${pageRange.end} of ${total.toLocaleString()}`}</span>
                <button type="button" disabled={offset === 0 || loading || updatingId !== null} onClick={() => setOffset(previousStorefrontOffset(offset, PAGE_SIZE))}>
                  <span className="icon-directional" aria-hidden="true">‹</span> Previous
                </button>
                <button type="button" disabled={offset + PAGE_SIZE >= total || loading || updatingId !== null} onClick={() => setOffset(nextStorefrontOffset(offset, PAGE_SIZE, total))}>
                  Next <span className="icon-directional" aria-hidden="true">›</span>
                </button>
              </div>
            )}
            <div className="sf-product-list" aria-busy={loading}>
              {products.map(product => (
                <article className={`sf-product-row ${product.publish_error ? "has-error" : ""}`} key={product.product_id}>
                  <label className="sf-product-visibility">
                    <span>
                      <strong>Shown in ZanShop</strong>
                      <small>{product.published ? "Included in next release" : "Hidden from customers"}</small>
                    </span>
                    <input
                      type="checkbox"
                      checked={product.published}
                      disabled={updatingId !== null}
                      onChange={event => void updateProduct(product.product_id, { published: event.target.checked })}
                    />
                    <span className="sf-toggle-track" aria-hidden="true" />
                  </label>
                  <div className="sf-product-identity">
                    <strong>{product.name}</strong>
                    <span>{formatStorefrontMoney(product.price_minor, product.currency)}{product.dirty ? " · Change waiting" : ""}</span>
                  </div>
                  <label htmlFor="a11y-input-1" className="sf-arabic-field">
                    <span>Public Arabic name</span>
                    <input id="a11y-input-1" dir="rtl" disabled={updatingId !== null} value={product.name_ar ?? ""} placeholder="الاسم بالعربية" onChange={event => patchProduct(product.product_id, { name_ar: event.target.value })} onBlur={event => void updateProduct(product.product_id, { name_ar: event.target.value.trim() || null })} />
                  </label>
                  <label htmlFor="a11y-input-2" className="sf-arabic-field sf-description-field">
                    <span>Arabic description</span>
                    <input id="a11y-input-2" dir="rtl" disabled={updatingId !== null} value={product.description_ar ?? ""} placeholder="وصف قصير للعميل" onChange={event => patchProduct(product.product_id, { description_ar: event.target.value })} onBlur={event => void updateProduct(product.product_id, { description_ar: event.target.value.trim() || null })} />
                  </label>
                  <label htmlFor="a11y-input-3" className="sf-feature-check">
                    <input id="a11y-input-3" type="checkbox" checked={product.featured} disabled={updatingId !== null} onChange={event => void updateProduct(product.product_id, { featured: event.target.checked })} />
                    <Sparkles aria-hidden="true" /> Featured
                  </label>
                  <label className="sf-order-field">Order<input type="number" min="0" disabled={updatingId !== null} value={product.sort_order} onChange={event => patchProduct(product.product_id, { sort_order: Number(event.target.value) })} onBlur={event => void updateProduct(product.product_id, { sort_order: Math.max(0, Number(event.target.value) || 0) })} /></label>
                  {product.publish_error && <span className="sf-product-error">{product.publish_error}</span>}
                </article>
              ))}
              {!loading && products.length === 0 && <p className="sf-empty">No products match this search.</p>}
            </div>
          </section>
        </main>

        <aside className="sf-journey" aria-label="Customer journey preview">
          <div className="sf-preview-heading"><span>Customer view</span><strong>{settings.locale === "ar" ? "معاينة المتجر" : "Shop preview"}</strong></div>
          <div className="sf-phone">
            <div className="sf-phone-bar"><span>{settings.locale === "ar" ? "منتجاتنا" : "Our shelf"}</span><span>•••</span></div>
            <div className="sf-phone-products">
              {previewProducts.map((product, index) => (
                <div className={index === 0 && product.featured ? "is-featured" : ""} key={product.product_id}>
                  <span className="sf-product-image">{product.image_url ? <img src={product.image_url} alt="" /> : product.name.slice(0, 1)}</span>
                  <strong>{settings.locale === "ar" ? product.name_ar || product.name : product.name}</strong>
                  <small>{formatStorefrontMoney(product.price_minor, product.currency)}</small>
                </div>
              ))}
              {previewProducts.length === 0 && <p>Select products to fill this shelf.</p>}
            </div>
            <div className="sf-whatsapp-cta"><MessageCircle aria-hidden="true" /> Order on WhatsApp</div>
          </div>
          <div className="sf-qr-ready">
            <QrCode aria-hidden="true" />
            <span><strong>QR-ready link</strong><small>{settings.public_url || "Add a public URL in Setup."}</small></span>
          </div>
          {status && (
            <dl className="sf-release-details">
              <div><dt>Last release</dt><dd>{releaseTime(status.last_release_at)}</dd></div>
              <div><dt>Release ID</dt><dd>{status.last_release_id ?? "—"}</dd></div>
              <div><dt>Failed items</dt><dd className={status.failed_product_count ? "is-danger" : ""}>{status.failed_product_count}</dd></div>
            </dl>
          )}
          <button className="sf-publish-button" onClick={publish} disabled={!readiness?.ready || publishing || !settings.enabled}>
            {publishing ? <><LoaderCircle aria-hidden="true" /> Publishing release…</> : <><Sparkles aria-hidden="true" /> Publish storefront</>}
          </button>
          {result && <div className={`sf-publish-result ${result.success ? "is-success" : "is-error"}`} role="status"><strong>{result.success ? `${result.published_count} products published` : "Release completed with issues"}</strong><span>{result.failed_count ? `${result.failed_count} failed. Your previous versions remain safe.` : `Release ${result.release_id ?? ""} is live.`}</span></div>}
        </aside>
      </div>
      {(error || status?.last_error) && <div className="sf-error-banner" role="alert"><strong>Catalogue needs attention</strong><span>{error ?? status?.last_error}</span></div>}
    </>
  );
}
