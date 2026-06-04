import { useEffect, useRef, useState } from "react";
import { DEVICE } from "../types";
import { formatMoney, parseMoney } from "../money";
import Dialpad, { applyDialpadKey } from "./Dialpad";

interface Props {
  onAdd: (name: string, priceMajor: string, quantity: string) => Promise<void>;
  onCancel: () => void;
}

interface Suggestion {
  id: string;
  name: string;
  price: string;
}

const STORAGE_KEY = "zanpos_custom_suggestions";

function loadSuggestions(): Suggestion[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? (JSON.parse(raw) as Suggestion[]) : [];
  } catch { return []; }
}

function saveSuggestions(list: Suggestion[]) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(list));
}

type ActiveField = "price" | "qty";

export default function CustomItemModal({ onAdd, onCancel }: Props) {
  const [name, setName]         = useState("");
  const [price, setPrice]       = useState("");
  const [qty, setQty]           = useState("1");
  const [activeField, setActiveField] = useState<ActiveField>("price");
  const [error, setError]       = useState<string | null>(null);
  const [loading, setLoading]   = useState(false);
  const nameRef = useRef<HTMLInputElement>(null);
  const priceRef = useRef<HTMLInputElement>(null);
  const qtyRef = useRef<HTMLInputElement>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);

  // Suggestions
  const [suggestions, setSuggestions] = useState<Suggestion[]>(loadSuggestions);
  const [managing, setManaging]       = useState(false);
  const [newSugName, setNewSugName]   = useState("");
  const [newSugPrice, setNewSugPrice] = useState("");
  const [sugError, setSugError]       = useState<string | null>(null);

  const EXP = DEVICE.currency_exponent;
  const cur = DEVICE.currency;

  // Focus the price input on open so the cashier can type immediately,
  // then Tab → qty → Add to Cart for a full keyboard-driven flow.
  useEffect(() => {
    const t = setTimeout(() => { priceRef.current?.focus(); priceRef.current?.select(); }, 50);
    return () => clearTimeout(t);
  }, []);

  // Derived values — price uses integer parsing; qty and line total are display-only previews
  const priceMinor   = parseMoney(price, EXP);
  const qtyNum       = parseFloat(qty);
  const qtyValid     = !isNaN(qtyNum) && qtyNum > 0;
  const lineTotal    = priceMinor > 0 && qtyValid ? Math.round(priceMinor * qtyNum) : 0;

  // Name is optional — defaults to "NO BARCODE ITEM" if left blank.
  const canConfirm = priceMinor > 0 && qtyValid;

  // Dialpad key handler — routes to active field
  const handleDialpadKey = (key: string) => {
    setError(null);
    if (activeField === "price") {
      setPrice(prev => applyDialpadKey(prev, key));
    } else {
      // Qty: whole numbers only — block decimal and 00
      if (key === "." || key === "00") return;
      setQty(prev => applyDialpadKey(prev, key));
    }
  };

  const handleAdd = async () => {
    setError(null);
    if (priceMinor <= 0)   { setError("Enter a valid price"); return; }
    if (!qtyValid)                          { setError("Enter a valid quantity"); return; }
    const finalName = name.trim() || "NO BARCODE ITEM";
    setLoading(true);
    try { await onAdd(finalName, price, qty); }
    catch (e: unknown) { setError(typeof e === "string" ? e : "Failed to add item"); }
    finally { setLoading(false); }
  };

  const applySuggestion = (s: Suggestion) => {
    setName(s.name);
    setPrice(s.price);
    setManaging(false);
    setError(null);
    setActiveField("qty");
  };

  const addSuggestion = () => {
    setSugError(null);
    if (!newSugName.trim()) { setSugError("Name required"); return; }
    // Price is optional — blank/zero means "price varies" (cashier enters at sale time)
    const p = parseFloat(newSugPrice);
    if (newSugPrice.trim() !== "" && (isNaN(p) || p < 0)) {
      setSugError("Enter a valid price, or leave blank for variable price");
      return;
    }
    const priceToSave = newSugPrice.trim() === "" ? "" : newSugPrice;
    const updated = [
      ...suggestions,
      { id: crypto.randomUUID(), name: newSugName.trim(), price: priceToSave },
    ];
    setSuggestions(updated);
    saveSuggestions(updated);
    setNewSugName("");
    setNewSugPrice("");
  };

  const removeSuggestion = (id: string) => {
    const updated = suggestions.filter(s => s.id !== id);
    setSuggestions(updated);
    saveSuggestions(updated);
  };

  return (
    <div className="modal-overlay" onClick={e => e.target === e.currentTarget && onCancel()}>
      <div className="custom-item-shell">

        {/* ── Left: form ── */}
        <div className="modal custom-item-left">
          <h2 className="modal-title">Custom Item</h2>

          {/* Suggestion chips */}
          <div className="ci-suggest-row">
            {suggestions.map(s => {
              const hasPrice = s.price && parseFloat(s.price) > 0;
              return (
                <button
                  key={s.id}
                  className="ci-suggest-chip"
                  onClick={() => applySuggestion(s)}
                  title={hasPrice ? `${s.name} — ${cur} ${s.price}` : `${s.name} — price varies`}
                >
                  <span className="ci-chip-name">{s.name}</span>
                  <span className="ci-chip-price">{hasPrice ? `${cur} ${s.price}` : "varies"}</span>
                </button>
              );
            })}
            <button
              className={`ci-manage-btn${managing ? " ci-manage-btn-active" : ""}`}
              onClick={() => { setManaging(m => !m); setSugError(null); }}
              title={managing ? "Close" : "Add / manage suggestions"}
            >
              {managing ? "✕" : "+"}
            </button>
          </div>

          {/* Manage panel */}
          {managing && (
            <div className="ci-manage-panel">
              {suggestions.length > 0 && (
                <div className="ci-manage-list">
                  {suggestions.map(s => {
                    const hasPrice = s.price && parseFloat(s.price) > 0;
                    return (
                      <div key={s.id} className="ci-manage-row">
                        <span className="ci-manage-name">{s.name}</span>
                        <span className="ci-manage-price">{hasPrice ? `${cur} ${s.price}` : "varies"}</span>
                        <button className="ci-manage-del" onClick={() => removeSuggestion(s.id)} title="Remove">×</button>
                      </div>
                    );
                  })}
                </div>
              )}
              <div className="ci-manage-add">
                <input
                  className="ci-manage-input"
                  placeholder="Name"
                  value={newSugName}
                  onChange={e => setNewSugName(e.target.value)}
                  onKeyDown={e => e.key === "Enter" && addSuggestion()}
                  autoFocus
                />
                <input
                  className="ci-manage-input ci-manage-price-input"
                  type="number"
                  inputMode="decimal"
                  min="0"
                  step={Math.pow(10, -EXP).toFixed(EXP)}
                  placeholder="varies"
                  value={newSugPrice}
                  onChange={e => setNewSugPrice(e.target.value)}
                  onKeyDown={e => e.key === "Enter" && addSuggestion()}
                />
                <button className="btn-primary ci-manage-save" onClick={addSuggestion}>Save</button>
              </div>
              {sugError && <div className="ci-manage-error">{sugError}</div>}
            </div>
          )}

          {/* Description */}
          <label className="ce-note-label">Description <span className="ce-label-optional">(optional)</span></label>
          <input
            ref={nameRef}
            className="ce-note-input"
            type="text"
            placeholder="Leave blank for 'NO BARCODE ITEM'"
            value={name}
            onChange={e => { setName(e.target.value); setError(null); }}
            onKeyDown={e => e.key === "Escape" && onCancel()}
            maxLength={80}
          />

          {/* Price / Qty inputs — keyboard-typeable; Tab moves price → qty → Add */}
          <div className="ci-field-tabs">
            <div className={`ci-field-tab${activeField === "price" ? " ci-field-tab-active" : ""}`}>
              <span className="ce-tab-label">Price ({cur})</span>
              <input
                ref={priceRef}
                className="ci-field-input"
                type="text"
                inputMode="decimal"
                tabIndex={1}
                value={price}
                placeholder={`0.${"0".repeat(EXP)}`}
                onFocus={() => setActiveField("price")}
                onChange={e => {
                  // Allow only digits + one decimal point
                  const v = e.target.value.replace(/[^0-9.]/g, "").replace(/(\..*)\./g, "$1");
                  setPrice(v); setError(null);
                }}
                onKeyDown={e => {
                  if (e.key === "Enter") { e.preventDefault(); qtyRef.current?.focus(); qtyRef.current?.select(); }
                }}
              />
            </div>
            <div className={`ci-field-tab${activeField === "qty" ? " ci-field-tab-active" : ""}`}>
              <span className="ce-tab-label">Qty</span>
              <input
                ref={qtyRef}
                className="ci-field-input"
                type="text"
                inputMode="numeric"
                tabIndex={2}
                value={qty}
                placeholder="1"
                onFocus={() => setActiveField("qty")}
                onChange={e => {
                  // Whole numbers only
                  const v = e.target.value.replace(/[^0-9]/g, "");
                  setQty(v); setError(null);
                }}
                onKeyDown={e => {
                  if (e.key === "Enter") { e.preventDefault(); if (canConfirm) confirmRef.current?.focus(); }
                }}
              />
            </div>
          </div>

          {/* Active field big display */}
          <div className="ce-amount-block" onClick={() => {}}>
            {activeField === "price" ? (
              <>
                <span className="ce-amount-cur">{cur}</span>
                <span className={`ce-amount-value${!price ? " ce-amount-placeholder" : ""}`}
                  style={price ? { color: "var(--accent)" } : {}}>
                  {price || `0.${"0".repeat(EXP)}`}
                </span>
              </>
            ) : (
              <>
                <span className="ce-amount-cur" style={{ fontSize: "0.9rem" }}>QTY</span>
                <span className={`ce-amount-value${!qty ? " ce-amount-placeholder" : ""}`}
                  style={qty ? { color: "var(--accent)" } : {}}>
                  {qty || "1"}
                </span>
              </>
            )}
            <span className="touch-cursor">|</span>
          </div>

          {/* Line total */}
          {lineTotal > 0 && (
            <div className="ci-line-total">
              <span className="ci-line-total-label">
                {qtyNum !== 1 ? `${cur} ${formatMoney(priceMinor, EXP)} × ${qty}` : "Total"}
              </span>
              <span className="ci-line-total-value">{cur} {formatMoney(lineTotal, EXP)}</span>
            </div>
          )}

          {error && <div className="modal-error">{error}</div>}
        </div>

        {/* ── Right: dialpad ── */}
        <div className="payment-dialpad-panel">
          <div className="dialpad-field-indicator">
            {activeField === "price" ? `Enter ${cur} Price` : "Enter Quantity"}
          </div>

          <Dialpad onKey={handleDialpadKey} />

          <div className="dialpad-actions">
            <button
              ref={confirmRef}
              className="dialpad-confirm-btn"
              tabIndex={3}
              onClick={handleAdd}
              disabled={!canConfirm || loading}
            >
              {loading ? (
                <span className="dialpad-confirm-label">Adding…</span>
              ) : (
                <>
                  <span className="dialpad-confirm-icon">✓</span>
                  <span className="dialpad-confirm-label">Add to Cart</span>
                  {lineTotal > 0 && (
                    <span className="dialpad-confirm-total">{cur} {formatMoney(lineTotal, EXP)}</span>
                  )}
                </>
              )}
            </button>
            <button className="dialpad-cancel-btn" onClick={onCancel} disabled={loading}>
              ✕ Cancel
            </button>
          </div>
        </div>

      </div>
    </div>
  );
}
