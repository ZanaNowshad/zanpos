import { useState, useEffect, useRef, useCallback } from "react";
import type { CustomerRow, DeliveryInput, PaymentInput } from "../types";
import { formatMoney, parseMoney } from "../money";
import { DEVICE } from "../types";
import * as cmd from "../tauri/commands";
import DeliveryForm from "./DeliveryForm";
import Dialpad, { applyDialpadKey } from "./Dialpad";
import { useFocusTrap } from "../hooks/useFocusTrap";

interface Props {
  netTotal: number;
  onConfirm: (payments: PaymentInput[], customerId?: string, delivery?: DeliveryInput, selectedCustomer?: CustomerRow) => void;
  onCancel: () => void;
  loading?: boolean;
  initialMethod?: PaymentInput["method"];
  splitMode?: boolean;
  sessionUserId?: string;
}

interface PaymentLine {
  id: number;
  method: PaymentInput["method"];
  amountStr: string;
  tenderedStr: string;
}

// activeField covers ALL dialpad-targetable numeric fields
type ActiveField =
  | { kind: "amount";   lineId: number }
  | { kind: "tendered"; lineId: number }
  | { kind: "phone" }
  | null;

const METHODS: { id: PaymentInput["method"]; icon: string; label: string; key?: string }[] = [
  { id: "cash",   icon: "💵", label: "Cash",   key: "C" },
  { id: "card",   icon: "💳", label: "Card",   key: "A" },
  { id: "wallet", icon: "📱", label: "Wallet", key: "W" },
  { id: "other",  icon: "•••", label: "Other" },
];

// H12: _lineCounter was a module-level mutable — it survived HMR, never reset, and
// was a concurrent-risk across multiple cart instances. Moved inside component as useRef.

export default function PaymentModal({
  netTotal, onConfirm, onCancel, loading,
  initialMethod, splitMode, sessionUserId,
}: Props) {
  const EXP = DEVICE.currency_exponent;
  const containerRef = useRef<HTMLDivElement>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);
  // Stable per-instance line ID counter (replaces the former module-level mutable)
  const _lineCounterRef = useRef(200);
  const mkLine = (method: PaymentInput["method"] = "cash"): PaymentLine =>
    ({ id: _lineCounterRef.current++, method, amountStr: "", tenderedStr: "" });
  useFocusTrap(containerRef, onCancel);
  const fmt = (n: number) => `${DEVICE.currency} ${formatMoney(n, EXP)}`;

  // ── Payment lines ────────────────────────────────────────────────────
  const [lines, setLines] = useState<PaymentLine[]>(() => {
    const first = mkLine(initialMethod ?? "cash");
    if (!splitMode) {
      const s = formatMoney(netTotal, EXP);
      first.amountStr = s;
      if (!initialMethod || initialMethod === "cash") first.tenderedStr = s;
    }
    return splitMode ? [first, mkLine("card")] : [first];
  });

  const [activeField, setActiveField] = useState<ActiveField>(
    () => ({ kind: "amount", lineId: lines[0].id })
  );

  // ── Customer accordion ───────────────────────────────────────────────
  const [showCust, setShowCust]         = useState(false);
  const [custSearch, setCustSearch]     = useState("");
  const [custResults, setCustResults]   = useState<CustomerRow[]>([]);
  const [selectedCust, setSelectedCust] = useState<CustomerRow | null>(null);
  const [showCustDrop, setShowCustDrop] = useState(false);
  const searchTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // ── Delivery accordion ───────────────────────────────────────────────
  const [showDelivery, setShowDelivery] = useState(false);
  const [deliveryData, setDeliveryData] = useState<Partial<DeliveryInput>>({});
  // phone state is LIFTED here so the dialpad can control it
  const [phoneRaw, setPhoneRaw]       = useState("");
  const [phoneError, setPhoneError]   = useState<string | null>(null);

  // ── Split accordion ──────────────────────────────────────────────────
  const [showSplit, setShowSplit] = useState(splitMode ?? false);

  // Customer search debounce
  // L8: Add mounted guard so async setState cannot fire on an unmounted component
  useEffect(() => {
    if (!custSearch.trim()) { setCustResults([]); return; }
    let mounted = true;
    if (searchTimer.current) clearTimeout(searchTimer.current);
    searchTimer.current = setTimeout(async () => {
      const rows = await cmd.customerList(sessionUserId ?? "", custSearch.trim()).catch(() => [] as CustomerRow[]);
      if (!mounted) return;
      setCustResults(rows.slice(0, 6));
      setShowCustDrop(true);
    }, 250);
    return () => {
      mounted = false;
      if (searchTimer.current) clearTimeout(searchTimer.current);
    };
  }, [custSearch]);

  const updateLine = (id: number, patch: Partial<PaymentLine>) =>
    setLines(prev => prev.map(l => l.id === id ? { ...l, ...patch } : l));
  const removeLine = (id: number) =>
    setLines(prev => prev.filter(l => l.id !== id));

  const allocatedMinor = lines.reduce(
    (s, l) => s + Math.max(parseMoney(l.amountStr, EXP), 0), 0
  );
  const remainingMinor = netTotal - allocatedMinor;

  // ── Dialpad handler — covers amount, tendered, AND phone ────────────
  const handleDialpadKey = useCallback((key: string) => {
    if (!activeField) return;

    if (activeField.kind === "amount" || activeField.kind === "tendered") {
      const line = lines.find(l => l.id === activeField.lineId);
      if (!line) return;
      const cur = activeField.kind === "amount" ? line.amountStr : line.tenderedStr;
      const next = applyDialpadKey(cur, key);
      if (activeField.kind === "amount") updateLine(activeField.lineId, { amountStr: next });
      else updateLine(activeField.lineId, { tenderedStr: next });

    } else if (activeField.kind === "phone") {
      // Digits only, max 8, no decimal/00 for phone numbers
      setPhoneRaw(prev => {
        if (key === "⌫") return prev.slice(0, -1);
        if (key === "C")  return "";
        if (key === "." || key === "00") return prev; // ignore
        if (prev.length >= 8) return prev;            // max 8 digits
        return prev + key;
      });
    }
  }, [activeField, lines]);

  // Physical keyboard → dialpad (digits/backspace work while modal is open)
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement).tagName;
      // Let text inputs handle their own typing
      if (tag === "INPUT" || tag === "TEXTAREA") return;

      const setMethod = (m: PaymentLine["method"]) => {
        setLines(p => p.map((l, i) => i === 0 ? { ...l, method: m } : l));
        // non-cash methods have no tendered — keep dialpad on amount
        if (m !== "cash") {
          setActiveField(prev => (prev?.kind === "tendered" ? { kind: "amount", lineId: lines[0].id } : prev));
        }
      };
      const k = e.key.toLowerCase();

      if (e.key >= "0" && e.key <= "9") { e.preventDefault(); handleDialpadKey(e.key); }
      else if (e.key === "Backspace")   { e.preventDefault(); handleDialpadKey("⌫"); }
      else if (e.key === "Delete")      { e.preventDefault(); handleDialpadKey("C"); }
      else if (e.key === ".")           { e.preventDefault(); handleDialpadKey("."); }
      // Single-key method switch (C/A/W) + legacy F1/F2/F3 aliases.
      else if (k === "c" || e.key === "F1") { e.preventDefault(); setMethod("cash"); }
      else if (k === "a" || e.key === "F2") { e.preventDefault(); setMethod("card"); }
      else if (k === "w" || e.key === "F3") { e.preventDefault(); setMethod("wallet"); }
      // "E" = Exact: snap cash tendered to the amount due (most common cash path).
      else if (k === "e") {
        e.preventDefault();
        const due = formatMoney(netTotal, EXP);
        setLines(p => p.map((l, i) => i === 0 ? { ...l, amountStr: due, tenderedStr: due } : l));
      }
      else if (e.key === "Escape")      { e.preventDefault(); onCancel(); }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [activeField, handleDialpadKey, onCancel, lines, netTotal, EXP]);

  // Fast checkout: focus the Confirm button on open so the cashier can press
  // Enter immediately after F9 to complete the sale (cash amount is pre-filled).
  useEffect(() => {
    const t = setTimeout(() => confirmRef.current?.focus(), 50);
    return () => clearTimeout(t);
  }, []);

  // Quick amount suggestions (round notes above total)
  const quickAmts = (() => {
    const unit = Math.pow(10, EXP);
    return [5, 10, 20, 50]
      .map(n => Math.round(n * unit))
      .filter(n => n > netTotal);
  })();

  const applyQuick = (minor: number) => {
    const cashLine = lines.find(l => l.method === "cash");
    if (!cashLine) return;
    const s = formatMoney(minor, EXP);
    updateLine(cashLine.id, { amountStr: s, tenderedStr: s });
  };

  // Validation
  const canConfirm = (() => {
    if (!lines.length) return false;
    for (const l of lines) {
      if (parseMoney(l.amountStr, EXP) <= 0) return false;
      if (l.method === "cash") {
        const t = parseMoney(l.tenderedStr || l.amountStr, EXP);
        if (t < parseMoney(l.amountStr, EXP)) return false;
      }
    }
    if (Math.abs(allocatedMinor - netTotal) > 1) return false; // 1-fil tolerance for BHD rounding
    if (showDelivery) {
      if (!deliveryData.contact_number) return false;
      if (!deliveryData.house_number?.trim()) return false;
      if (!deliveryData.address_text?.trim()) return false;
    }
    return true;
  })();

  const handleConfirm = () => {
    const payments: PaymentInput[] = lines.map(l => {
      const amount = parseMoney(l.amountStr, EXP);
      if (l.method === "cash") {
        const tendered = Math.max(parseMoney(l.tenderedStr || l.amountStr, EXP), amount);
        return { method: l.method, amount_minor: amount, tendered_minor: tendered };
      }
      return { method: l.method, amount_minor: amount };
    });
    const delivery: DeliveryInput | undefined = showDelivery
      ? { ...(deliveryData as DeliveryInput), expected_payment_method: lines[0]?.method ?? "cash" }
      : undefined;
    onConfirm(payments, selectedCust?.customer_id, delivery, selectedCust ?? undefined);
  };

  // Remaining / change display
  const remainingText =
    remainingMinor > 0 ? `Still owed: ${fmt(remainingMinor)}` :
    remainingMinor < 0 ? `Change: ${fmt(-remainingMinor)}` :
    "✓ Fully paid";
  const remainingCls =
    `pm-remaining${remainingMinor < 0 ? " pm-remaining-change" : remainingMinor === 0 ? " pm-remaining-ok" : ""}`;

  // Field label for dialpad indicator
  const fieldLabel =
    activeField?.kind === "tendered" ? "Tendered" :
    activeField?.kind === "phone"    ? "Phone No." : "Amount";

  return (
    <div className="modal-overlay" onClick={e => e.target === e.currentTarget && onCancel()}>
      <div className="pm-shell" role="dialog" aria-modal="true" aria-labelledby="pm-dialog-title" ref={containerRef}>

        {/* ── LEFT: payment form ───────────────────────────────────────── */}
        <div className="pm-left">

          {/* Header: title + total */}
          <div className="pm-header">
            <span className="pm-title" id="pm-dialog-title">Checkout</span>
            <span className="pm-total-badge">{fmt(netTotal)}</span>
          </div>

          {/* Payment line(s) */}
          {lines.map((line, idx) => {
            const amt      = parseMoney(line.amountStr, EXP);
            const tendered = parseMoney(line.tenderedStr || line.amountStr, EXP);
            const change   = line.method === "cash" && tendered > amt ? tendered - amt : 0;
            const isActiveAmt = activeField?.kind === "amount"   && activeField.lineId === line.id;
            const isActiveTen = activeField?.kind === "tendered" && activeField.lineId === line.id;

            return (
              <div key={line.id} className={`pm-line${lines.length > 1 ? " pm-line-split" : ""}`}>

                {/* Method pills — compact single row */}
                <div className="pm-methods">
                  {METHODS.map(m => (
                    <button
                      key={m.id}
                      className={`pm-method${line.method === m.id ? " pm-method-active" : ""}`}
                      onClick={() => {
                        updateLine(line.id, { method: m.id });
                        // If switching away from cash, tendered field disappears —
                        // reset activeField so dialpad stays connected to amount
                        if (m.id !== "cash" && activeField?.kind === "tendered" && activeField.lineId === line.id) {
                          setActiveField({ kind: "amount", lineId: line.id });
                        }
                        // Never steal focus away to confirm button — let the user keep typing
                      }}
                    >
                      <span className="pm-method-icon">{m.icon}</span>
                      <span className="pm-method-label">{m.label}</span>
                      {m.key && <kbd className="pm-method-key">{m.key}</kbd>}
                    </button>
                  ))}
                  {lines.length > 1 && (
                    <button className="pm-remove-line" onClick={() => removeLine(line.id)} title="Remove">×</button>
                  )}
                </div>

                {/* Amount — tappable field, dialpad-connected */}
                <div
                  className={`pm-field${isActiveAmt ? " pm-field-active" : ""}`}
                  onClick={() => setActiveField({ kind: "amount", lineId: line.id })}
                >
                  <span className="pm-field-label">Amount</span>
                  <span className="pm-field-value">
                    {line.amountStr || (
                      <span className="pm-field-ph">
                        {idx === 0 && lines.length === 1 ? formatMoney(netTotal, EXP) : "0.000"}
                      </span>
                    )}
                  </span>
                  {isActiveAmt && <span className="pm-cursor">|</span>}
                </div>

                {/* Tendered — cash only, dialpad-connected */}
                {line.method === "cash" && (
                  <div
                    className={`pm-field${isActiveTen ? " pm-field-active" : ""}`}
                    onClick={() => setActiveField({ kind: "tendered", lineId: line.id })}
                  >
                    <span className="pm-field-label">Tendered</span>
                    <span className="pm-field-value">
                      {line.tenderedStr || (
                        <span className="pm-field-ph">{line.amountStr || "0.000"}</span>
                      )}
                    </span>
                    {isActiveTen && <span className="pm-cursor">|</span>}
                    {change > 0 && <span className="pm-change">Change {fmt(change)}</span>}
                  </div>
                )}
              </div>
            );
          })}

          {/* Quick amounts — cash only, only when useful. "E" key = exact amount. */}
          {lines.length === 1 && lines[0].method === "cash" && (
            <div className="pm-quick">
              <button className="pm-quick-btn pm-quick-exact" onClick={() => applyQuick(netTotal)} title="Exact amount (press E)">
                Exact <kbd className="pm-quick-key">E</kbd>
              </button>
              {quickAmts.slice(0, 3).map(a => (
                <button key={a} className="pm-quick-btn" onClick={() => applyQuick(a)}>
                  {fmt(a)}
                </button>
              ))}
            </div>
          )}

          {/* Remaining / change */}
          <div className={remainingCls}>{remainingText}</div>

          {/* Optional sections — collapsed by default ──────────────── */}
          <div className="pm-accordions">

            {/* Split payment */}
            {!showSplit ? (
              <button
                className="pm-accord-toggle pm-accord-add"
                onClick={() => { setShowSplit(true); setLines(p => [...p, mkLine("card")]); }}
              >
                + Split payment
              </button>
            ) : (
              <button
                className="pm-accord-toggle"
                onClick={() => { setShowSplit(false); setLines(p => p.slice(0, 1)); }}
              >
                − Remove split
              </button>
            )}

            {/* Customer */}
            <button
              className={`pm-accord-toggle${selectedCust ? " pm-accord-filled" : ""}`}
              onClick={() => setShowCust(v => !v)}
            >
              <span>👤 {selectedCust ? selectedCust.name : "Customer"}</span>
              <span className="pm-accord-arrow">{showCust ? "▲" : "▼"}</span>
            </button>
            {showCust && (
              <div className="pm-accord-body">
                {selectedCust ? (
                  <div className="cust-chip">
                    <span>{selectedCust.name}{selectedCust.phone ? ` · ${selectedCust.phone}` : ""}</span>
                    <span className="cust-chip-pts">{selectedCust.loyalty_points} pts</span>
                    <button className="cust-chip-remove"
                      onClick={() => { setSelectedCust(null); setCustSearch(""); }}>×</button>
                  </div>
                ) : (
                  <div className="cust-search-wrap">
                    <input
                      className="cust-search-input"
                      placeholder="Search name or phone…"
                      value={custSearch}
                      onChange={e => { setCustSearch(e.target.value); if (!e.target.value) setShowCustDrop(false); }}
                      onBlur={() => setTimeout(() => setShowCustDrop(false), 180)}
                    />
                    {showCustDrop && custResults.length > 0 && (
                      <div className="cust-dropdown">
                        {custResults.map(c => (
                          <button key={c.customer_id} className="cust-dropdown-item"
                            onMouseDown={() => { setSelectedCust(c); setCustSearch(""); setShowCustDrop(false); }}>
                            <span className="cust-dd-name">{c.name}</span>
                            {c.phone && <span className="cust-dd-phone">{c.phone}</span>}
                          </button>
                        ))}
                      </div>
                    )}
                  </div>
                )}
              </div>
            )}

            {/* Delivery */}
            <button
              className={`pm-accord-toggle${showDelivery ? " pm-accord-filled" : ""}`}
              onClick={() => setShowDelivery(v => !v)}
            >
              <span>🛵 Delivery</span>
              <span className="pm-accord-arrow">{showDelivery ? "▲" : "▼"}</span>
            </button>
            {showDelivery && (
              <div className="pm-accord-body">
                <DeliveryForm
                  value={deliveryData}
                  onChange={setDeliveryData}
                  selectedCustomer={selectedCust}
                  expectedPaymentMethod={lines[0]?.method ?? "cash"}
                  actorUserId={sessionUserId ?? DEVICE.device_id}
                  phoneRaw={phoneRaw}
                  phoneError={phoneError}
                  onPhoneChange={(raw, normalized, err) => {
                    setPhoneRaw(raw);
                    setPhoneError(err);
                    setDeliveryData(prev => ({ ...prev, contact_number: normalized ?? "" }));
                  }}
                  onPhoneFocus={() => setActiveField({ kind: "phone" })}
                />
              </div>
            )}
          </div>
        </div>

        {/* ── RIGHT: dialpad ───────────────────────────────────────────── */}
        <div className="pm-right">
          <div className="pm-field-indicator">{fieldLabel}</div>

          <Dialpad onKey={handleDialpadKey} />

          <div className="pm-actions">
            <button
              ref={confirmRef}
              className="pm-confirm-btn"
              onClick={handleConfirm}
              disabled={!canConfirm || loading}
            >
              {loading ? "Processing…" : (
                <>
                  <span className="pm-confirm-check">✓</span>
                  <span className="pm-confirm-label">Confirm Payment</span>
                  <span className="pm-confirm-total">{fmt(netTotal)}</span>
                </>
              )}
            </button>
            <button className="pm-cancel-btn" onClick={onCancel} disabled={loading}>
              ✕ Cancel
            </button>
          </div>
        </div>

      </div>
    </div>
  );
}
