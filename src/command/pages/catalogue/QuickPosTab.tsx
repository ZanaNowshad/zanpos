import { useCallback, useEffect, useMemo, useState } from "react";
import { ChevronLeft, ChevronRight, Package, Search, X } from "lucide-react";
import PageTemplate from "../../../components/templates/PageTemplate";
import { quickPosLoad, quickPosSave, type QuickPosSlot } from "../../../tauri/commands";
import { adminListProducts } from "../../../tauri/commands";
import type { AdminProduct } from "../../../types";
import { DEVICE } from "../../../types";
import { formatMoney } from "../../../money";
import ProductThumb from "../../../components/ProductThumb";
import "./quickpos.css";

interface Props {
  sessionUserId: string;
}

/** Rows per page in the picker. Sized so a page fills the dialog without the
 *  list scrolling — paging and scrolling at once makes position hard to hold. */
const PICKER_PAGE = 8;

/**
 * Chooses the ten one-tap products shown on every till.
 *
 * Slots are positional: slot 3 is the third tile on the till, whether or not
 * slots 1 and 2 are filled, so a shop can group things by muscle memory rather
 * than by whatever order they happened to be picked.
 *
 * The choice is stored as product ids and resolved on read, so a repricing or a
 * new photo reaches the tills without anyone revisiting this screen — and a
 * product that is later deactivated shows here as an empty slot rather than a
 * tile the cashier cannot sell.
 */
export default function QuickPosTab({ sessionUserId }: Props) {
  const [slots, setSlots] = useState<QuickPosSlot[]>([]);
  const [picking, setPicking] = useState<number | null>(null);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<AdminProduct[]>([]);
  const [total, setTotal] = useState(0);
  const [offset, setOffset] = useState(0);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  const load = useCallback(async () => {
    try {
      setSlots(await quickPosLoad(sessionUserId));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [sessionUserId]);

  useEffect(() => { void load(); }, [load]);

  /* A new search starts at page one. Without this, typing while on page 4 of
     the old results asks the server for rows 49-60 of a much shorter list and
     the picker looks empty. */
  useEffect(() => { setOffset(0); }, [query, picking]);

  useEffect(() => {
    if (picking === null) return;
    let cancelled = false;
    const id = setTimeout(async () => {
      try {
        const page = await adminListProducts(sessionUserId, {
          search: query.trim() || undefined, limit: PICKER_PAGE, offset,
        });
        if (cancelled) return;
        setResults(page.items);
        setTotal(page.total);
      } catch {
        if (!cancelled) { setResults([]); setTotal(0); }
      }
    }, 220);
    return () => { cancelled = true; clearTimeout(id); };
  }, [picking, query, offset, sessionUserId]);

  const productIds = useMemo(() => slots.map(slot => slot.product_id), [slots]);

  const persist = useCallback(async (next: (string | null)[]) => {
    setSaving(true);
    setError(null);
    try {
      setSlots(await quickPosSave(sessionUserId, next));
      setSaved(true);
      setTimeout(() => setSaved(false), 2200);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      void load(); // never leave the grid showing something that did not save
    } finally {
      setSaving(false);
    }
  }, [sessionUserId, load]);

  const assign = (slot: number, productId: string | null) => {
    const next = [...productIds];
    next[slot] = productId;
    setPicking(null);
    setQuery("");
    void persist(next);
  };

  const money = (minor: number | null) =>
    `${DEVICE.currency} ${formatMoney(minor ?? 0, DEVICE.currency_exponent)}`;

  return (
    <PageTemplate
      header={{
        title: "Quick POS",
        subtitle: "Ten one-tap products on every till — for items with no barcode to scan",
        icon: <Package size={18} aria-hidden="true" />,
      }}
    >
      {error && <div className="qp-error" role="alert">{error}</div>}
      {saved && <div className="qp-saved" role="status">Saved — every till will pick this up.</div>}

      <ol className="qp-grid">
        {slots.map(slot => (
          <li key={slot.slot} className="qp-slot">
            <span className="qp-slot-no">{slot.slot + 1}</span>
            {slot.product_id ? (
              <>
                <span className="qp-thumb">
                  <ProductThumb imagePath={slot.image_path} productName={slot.name} />
                </span>
                <span className="qp-name">{slot.name}</span>
                <span className="qp-price">{money(slot.price_minor)}</span>
                <span className="qp-slot-actions">
                  <button type="button" onClick={() => { setPicking(slot.slot); setQuery(""); }} disabled={saving}>
                    Change
                  </button>
                  <button
                    type="button"
                    className="qp-clear"
                    onClick={() => assign(slot.slot, null)}
                    disabled={saving}
                    aria-label={`Clear slot ${slot.slot + 1}`}
                  >
                    <X size={15} aria-hidden="true" />
                  </button>
                </span>
              </>
            ) : (
              <button
                type="button"
                className="qp-empty"
                onClick={() => { setPicking(slot.slot); setQuery(""); }}
                disabled={saving}
              >
                <Package size={20} aria-hidden="true" />
                <span>Choose product</span>
              </button>
            )}
          </li>
        ))}
      </ol>

      {picking !== null && (
        <div className="qp-picker-backdrop" role="presentation" onClick={() => setPicking(null)}>
          <div
            className="qp-picker"
            role="dialog"
            aria-modal="true"
            aria-label={`Choose a product for slot ${picking + 1}`}
            onClick={e => e.stopPropagation()}
          >
            <header className="qp-picker-head">
              <h2>Slot {picking + 1}</h2>
              <button type="button" onClick={() => setPicking(null)} aria-label="Close">
                <X size={20} aria-hidden="true" />
              </button>
            </header>

            <label className="qp-search">
              <Search size={16} aria-hidden="true" />
              <input
                autoFocus
                value={query}
                onChange={e => setQuery(e.target.value)}
                placeholder="Search products by name, SKU or barcode…"
              />
            </label>

            <div className="qp-results">
              {results.length === 0 && <p className="qp-none">No matching products.</p>}
              {results.map(product => (
                <button
                  key={product.product_id}
                  type="button"
                  className="qp-result"
                  onClick={() => assign(picking, product.product_id)}
                >
                  <span className="qp-thumb">
                    <ProductThumb imagePath={product.image_path} productName={product.name} />
                  </span>
                  <span className="qp-result-text">
                    <strong>{product.name}</strong>
                    <small>{product.barcode ?? product.sku ?? "—"}</small>
                  </span>
                  <span className="qp-price">{money(product.price_minor)}</span>
                </button>
              ))}
            </div>

            {total > 0 && (
              <div className="qp-pager">
                <span className="qp-pager-count">
                  {offset + 1}–{Math.min(offset + PICKER_PAGE, total)} of {total}
                </span>
                <button
                  type="button"
                  disabled={offset === 0}
                  onClick={() => setOffset(Math.max(0, offset - PICKER_PAGE))}
                >
                  <ChevronLeft size={16} className="icon-directional" aria-hidden="true" />
                  Previous
                </button>
                <button
                  type="button"
                  disabled={offset + PICKER_PAGE >= total}
                  onClick={() => setOffset(offset + PICKER_PAGE)}
                >
                  Next
                  <ChevronRight size={16} className="icon-directional" aria-hidden="true" />
                </button>
              </div>
            )}
          </div>
        </div>
      )}
    </PageTemplate>
  );
}
