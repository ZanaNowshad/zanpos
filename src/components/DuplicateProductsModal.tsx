import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { DuplicateGroup, DuplicateProduct } from "../types";
import { DEVICE } from "../types";
import { formatMoney } from "../money";
import {
  adminFindDuplicateProducts,
  adminMergeProducts,
  adminDeleteProduct,
} from "../tauri/commands";
import { useFocusTrap } from "../hooks/useFocusTrap";
import { useLanguage } from "../hooks/useLanguage";
import { modalTranslator } from "../i18n/modalStrings";
import { detailTranslator } from "../i18n/detailStrings";

interface Props {
  sessionUserId: string;
  onClose: () => void;
  /** Notify parent (ProductsTab) to refresh its product list after a change. */
  onResolved: () => void;
}

type MergePlan = { keeperId: string; sources: string[] };

type Pending =
  | { kind: "merge-group"; keeperName: string; keeperId: string; sources: string[] }
  | { kind: "delete-others"; keeperName: string; sources: string[] }
  | { kind: "delete-one"; name: string; productId: string }
  | { kind: "merge-all"; plan: MergePlan[]; archiveCount: number };

const gidOf = (g: DuplicateGroup, i: number) => `${i}|${g.match_type}|${g.match_key}`;

export default function DuplicateProductsModal({ sessionUserId, onClose, onResolved }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => modalTranslator(language), [language]);
  const dt = useMemo(() => detailTranslator(language), [language]);
  const modalRef = useRef<HTMLDivElement>(null);
  useFocusTrap(modalRef, onClose);
  const exp = DEVICE.currency_exponent;
  const cur = DEVICE.currency;

  const [groups, setGroups] = useState<DuplicateGroup[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [keepers, setKeepers] = useState<Record<string, string>>({});
  const [transferHistory, setTransferHistory] = useState(true);
  const [includeInactive, setIncludeInactive] = useState(false);
  const [pending, setPending] = useState<Pending | null>(null);
  const [busy, setBusy] = useState(false);

  const scan = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const g = await adminFindDuplicateProducts(sessionUserId, includeInactive);
      setGroups(g);
      // Default keeper per group: most stock, then highest price, then first listed.
      const def: Record<string, string> = {};
      g.forEach((grp, i) => {
        const best = [...grp.products].sort(
          (a, b) => b.total_stock - a.total_stock || b.price_minor - a.price_minor,
        )[0];
        if (best) def[gidOf(grp, i)] = best.product_id;
      });
      setKeepers(def);
    } catch (e) {
      setError(typeof e === "string" ? e : dt("failedLookup"));
    } finally {
      setLoading(false);
    }
  }, [dt, sessionUserId, includeInactive]);

  useEffect(() => { scan(); }, [scan]);

  const runPending = async () => {
    if (!pending) return;
    setBusy(true);
    setError(null);
    try {
      if (pending.kind === "merge-group") {
        for (const sid of pending.sources) {
          await adminMergeProducts(sessionUserId, sid, pending.keeperId, transferHistory);
        }
      } else if (pending.kind === "delete-others" || pending.kind === "delete-one") {
        const ids = pending.kind === "delete-one" ? [pending.productId] : pending.sources;
        for (const id of ids) await adminDeleteProduct(sessionUserId, id);
      } else if (pending.kind === "merge-all") {
        // Best-effort across groups — a product caught in two groups may already
        // be archived by an earlier merge, so tolerate per-call failures.
        const archived = new Set<string>();
        let failed = 0;
        for (const p of pending.plan) {
          for (const sid of p.sources) {
            if (archived.has(sid) || sid === p.keeperId) continue;
            try {
              await adminMergeProducts(sessionUserId, sid, p.keeperId, transferHistory);
              archived.add(sid);
            } catch {
              failed += 1;
            }
          }
        }
        if (failed > 0) setError(`${failed} merge(s) were skipped (already resolved in another group).`);
      }
      setPending(null);
      onResolved();
      await scan();
    } catch (e) {
      setError(typeof e === "string" ? e : dt("failedDismiss"));
      setPending(null);
    } finally {
      setBusy(false);
    }
  };

  const totalDupes = groups.reduce((n, g) => n + Math.max(0, g.products.length - 1), 0);

  const onMergeAll = () => {
    const plan: MergePlan[] = [];
    let archiveCount = 0;
    groups.forEach((g, i) => {
      const keeperId = keepers[gidOf(g, i)];
      const sources = g.products.filter(p => p.product_id !== keeperId).map(p => p.product_id);
      if (keeperId && sources.length > 0) {
        plan.push({ keeperId, sources });
        archiveCount += sources.length;
      }
    });
    if (plan.length > 0) setPending({ kind: "merge-all", plan, archiveCount });
  };

  const keeperName = (g: DuplicateGroup, keeperId: string) =>
    g.products.find(p => p.product_id === keeperId)?.name ?? "kept product";

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div
        ref={modalRef}
        className="modal dup-modal"
        role="dialog"
        aria-modal="true"
        aria-label={t("duplicates")}
        onClick={e => e.stopPropagation()}
      >
        <div className="ghost-modal-header">
          <div className="ghost-modal-title">
            <h2>{t("duplicates")}</h2>
            <p className="ghost-modal-sub">
              {dt("catalogScanDescription")}
            </p>
          </div>
          <button className="bo-form-modal-close" onClick={onClose} aria-label={t("close")}>✕</button>
        </div>

        <div className="dup-toolbar">
          <label className="dup-check">
            <input
              type="checkbox"
              checked={includeInactive}
              onChange={e => setIncludeInactive(e.target.checked)}
            />
            {dt("includeArchived")}
          </label>
          <label className="dup-check">
            <input
              type="checkbox"
              checked={transferHistory}
              onChange={e => setTransferHistory(e.target.checked)}
            />
            {dt("reassignSalesHistory")}
          </label>
          <span className="dup-toolbar-spacer" />
          <button className="btn-secondary" onClick={scan} disabled={loading || busy}>{t("refresh")}</button>
          {totalDupes > 0 && (
            <button className="btn-primary" onClick={onMergeAll} disabled={busy}>
              {dt("mergeAll")} ({totalDupes})
            </button>
          )}
        </div>

        <div className="dup-modal-body">
          {error && <div className="ghost-error">{error}</div>}

          {pending && (
            <div className="dup-confirm">
              <div className="dup-confirm-text">
                {pending.kind === "merge-group" &&
                  `${dt("mergeConfirmPrefix")} “${pending.keeperName}”? ${dt("mergeConfirmSuffix")}`}
                {pending.kind === "delete-others" &&
                  `${dt("archiveOthersPrefix")} “${pending.keeperName}”? ${dt("archiveOthersSuffix")}`}
                {pending.kind === "delete-one" &&
                  `${dt("archiveProduct")} “${pending.name}”? ${dt("archiveOneSuffix")}`}
                {pending.kind === "merge-all" &&
                  dt("mergeAllConfirm")}
              </div>
              <div className="dup-confirm-actions">
                <button className="btn-secondary" onClick={() => setPending(null)} disabled={busy}>{t("cancel")}</button>
                <button className="btn-danger" onClick={runPending} disabled={busy}>
                  {busy ? dt("working") : t("confirm")}
                </button>
              </div>
            </div>
          )}

          {loading && <div className="ghost-loading">{dt("scanningCatalog")}</div>}

          {!loading && groups.length === 0 && (
            <div className="dup-empty">
              <div className="dup-empty-icon">✓</div>
              <p className="dup-empty-title">{dt("noDuplicatesFound")}</p>
              <p className="dup-empty-hint">{dt("catalogLooksClean")}</p>
            </div>
          )}

          {!loading && groups.map((g, i) => {
            const gid = gidOf(g, i);
            const keeperId = keepers[gid];
            const sources = g.products.filter(p => p.product_id !== keeperId).map(p => p.product_id);
            return (
              <div key={gid} className="dup-group">
                <div className="dup-group-head">
                  <span className={`dup-match dup-match-${g.match_type.replace(/\s+/g, "-").toLowerCase()}`}>
                    {g.match_type}
                  </span>
                  <span className="dup-confidence">{g.confidence}% {dt("confidence")}</span>
                  <span className="dup-group-key">“{g.match_key}”</span>
                  <span className="dup-group-count">{g.products.length} {dt("products")}</span>
                </div>
                {g.reason && <div className="dup-group-reason">{g.reason}</div>}

                <div className="dup-rows">
                  {g.products.map((p: DuplicateProduct) => {
                    const isKeeper = p.product_id === keeperId;
                    return (
                      <label key={p.product_id} className={`dup-row${isKeeper ? " dup-row-keeper" : ""}`}>
                        <input
                          type="radio"
                          name={`keep-${gid}`}
                          checked={isKeeper}
                          onChange={() => setKeepers(k => ({ ...k, [gid]: p.product_id }))}
                        />
                        {p.image_path ? (
                          <img
                            className="dup-row-img"
                            src={convertFileSrc(p.image_path)}
                            alt=""
                            onError={e => { (e.target as HTMLImageElement).style.display = "none"; }}
                          />
                        ) : <span className="dup-row-img dup-row-img-empty" />}
                        <div className="dup-row-info">
                          <div className="dup-row-name">
                            {p.name}
                            {isKeeper && <span className="dup-keep-tag">{dt("keep")}</span>}
                            {!p.is_active && <span className="dup-inactive-tag">{dt("archived")}</span>}
                          </div>
                          <div className="dup-row-meta">
                            {p.category_name}
                            {p.sku && ` · SKU ${p.sku}`}
                            {p.barcode && ` · #${p.barcode}`}
                          </div>
                        </div>
                        <div className="dup-row-right">
                          <span className="dup-row-price">{cur} {formatMoney(p.price_minor, exp)}</span>
                          <span className="dup-row-stock">{Math.round(p.total_stock)} {dt("inStock")}</span>
                        </div>
                        {!isKeeper && (
                          <button
                            type="button"
                            className="dup-row-del"
                            title={dt("archiveProduct")}
                            onClick={e => {
                              e.preventDefault();
                              setPending({ kind: "delete-one", name: p.name, productId: p.product_id });
                            }}
                          >✕</button>
                        )}
                      </label>
                    );
                  })}
                </div>

                <div className="dup-group-actions">
                  <button
                    className="btn-primary"
                    disabled={busy || !keeperId || sources.length === 0}
                    onClick={() => setPending({
                      kind: "merge-group",
                      keeperId,
                      keeperName: keeperName(g, keeperId),
                      sources,
                    })}
                  >
                    {dt("mergeIntoSelected")} ({sources.length})
                  </button>
                  <button
                    className="btn-secondary"
                    disabled={busy || !keeperId || sources.length === 0}
                    onClick={() => setPending({
                      kind: "delete-others",
                      keeperName: keeperName(g, keeperId),
                      sources,
                    })}
                  >
                    {dt("archiveOtherDuplicates")} ({sources.length})
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
