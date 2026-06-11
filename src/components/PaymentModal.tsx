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

export default function PaymentModal({
  netTotal, onConfirm, onCancel, loading,
  initialMethod, splitMode, sessionUserId,
}: Props) {
  const EXP = DEVICE.currency_exponent;
  const containerRef = useRef<HTMLDivElement>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);
  const _lineCounterRef = useRef(200);
  const handleConfirmRef = useRef<() => void>(() => {});
  const mkLine = (method: PaymentInput["method"] = "cash"): PaymentLine =>
    ({ id: _lineCounterRef.current++, method, amountStr: "", tenderedStr: "" });
  useFocusTrap(containerRef, onCancel);
  const fmt = (n: number) => `${formatMoney(n, EXP)}`;

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

  const [showCust, setShowCust]         = useState(false);
  const [custSearch, setCustSearch]     = useState("");
  const [custResults, setCustResults]   = useState<CustomerRow[]>([]);
  const [selectedCust, setSelectedCust] = useState<CustomerRow | null>(null);
  const [showCustDrop, setShowCustDrop] = useState(false);
  const searchTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const [showDelivery, setShowDelivery] = useState(false);
  const [deliveryData, setDeliveryData] = useState<Partial<DeliveryInput>>({});
  const [phoneRaw, setPhoneRaw]       = useState("");
  const [phoneError, setPhoneError]   = useState<string | null>(null);

  const [showSplit, setShowSplit] = useState(splitMode ?? false);

  useEffect(() => {
    if (!custSearch.trim()) { setCustResults([]); return; }
    let mounted = true;
    if (searchTimer.current) clearTimeout(searchTimer.current);
    searchTimer.current = setTimeout(async () => {
      const rows = await cmd.customerList(sessionUserId ?? "", custSearch.trim()).catch((e: unknown) => { console.warn("customerList failed:", e); return [] as CustomerRow[]; });
      if (!mounted) return;
      setCustResults(rows.slice(0, 6));
      setShowCustDrop(true);
    }, 250);
    return () => {
      mounted = false;
      if (searchTimer.current) clearTimeout(searchTimer.current);
    };
  }, [custSearch]);

  const updateLine = useCallback((id: number, patch: Partial<PaymentLine>) =>
    setLines(prev => prev.map(l => l.id === id ? { ...l, ...patch } : l)), []);
  const removeLine = (id: number) =>
    setLines(prev => prev.filter(l => l.id !== id));

  const allocatedMinor = lines.reduce(
    (s, l) => s + Math.max(parseMoney(l.amountStr, EXP), 0), 0
  );
  const remainingMinor = netTotal - allocatedMinor;

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
      setPhoneRaw(prev => {
        if (key === "⌫") return prev.slice(0, -1);
        if (key === "C")  return "";
        if (key === "." || key === "00") return prev;
        if (prev.length >= 8) return prev;
        return prev + key;
      });
    }
  }, [activeField, lines, updateLine]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement).tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      const setMethod = (m: PaymentLine["method"]) => {
        setLines(p => p.map((l, i) => i === 0 ? { ...l, method: m } : l));
        if (m !== "cash") {
          setActiveField(prev => (prev?.kind === "tendered" ? { kind: "amount", lineId: lines[0].id } : prev));
        }
      };
      const k = e.key.toLowerCase();
      if (e.key >= "0" && e.key <= "9") { e.preventDefault(); handleDialpadKey(e.key); }
      else if (e.key === "Backspace")   { e.preventDefault(); handleDialpadKey("⌫"); }
      else if (e.key === "Delete")      { e.preventDefault(); handleDialpadKey("C"); }
      else if (e.key === ".")           { e.preventDefault(); handleDialpadKey("."); }
      else if (k === "c" || e.key === "F1") { e.preventDefault(); setMethod("cash"); }
      else if (k === "a" || e.key === "F2") { e.preventDefault(); setMethod("card"); }
      else if (k === "w" || e.key === "F3") { e.preventDefault(); setMethod("wallet"); }
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

  useEffect(() => {
    const t = setTimeout(() => confirmRef.current?.focus(), 50);
    return () => clearTimeout(t);
  }, []);

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

  const canConfirm = (() => {
    if (!lines.length) return false;
    for (const l of lines) {
      if (parseMoney(l.amountStr, EXP) <= 0) return false;
      if (l.method === "cash") {
        const t = parseMoney(l.tenderedStr || l.amountStr, EXP);
        if (t < parseMoney(l.amountStr, EXP)) return false;
      }
    }
    if (Math.abs(allocatedMinor - netTotal) > 1) return false;
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
      ? { ...(deliveryData as DeliveryInput), customer_id: selectedCust?.customer_id, expected_payment_method: lines[0]?.method ?? "cash" }
      : undefined;
    onConfirm(payments, selectedCust?.customer_id, delivery, selectedCust ?? undefined);
  };
  // Keep ref current so Enter-key handler always calls the latest handleConfirm
  useEffect(() => { handleConfirmRef.current = handleConfirm; });
  useEffect(() => {
    const onEnterKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement).tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      if (e.key === "Enter" || e.key === "NumpadEnter") {
        e.preventDefault();
        if (canConfirm && !loading) handleConfirmRef.current();
      }
    };
    document.addEventListener("keydown", onEnterKey);
    return () => document.removeEventListener("keydown", onEnterKey);
  }, [canConfirm, loading]);

  const activeLabel =
    activeField?.kind === "tendered" ? "Tendered" :
    activeField?.kind === "phone"    ? "Phone" : "Amount";

  const mainLine = lines[0];
  const amt = parseMoney(mainLine?.amountStr ?? "", EXP);
  const tendered = parseMoney(mainLine?.tenderedStr ?? mainLine?.amountStr ?? "", EXP);
  const change = mainLine?.method === "cash" && tendered > amt ? tendered - amt : 0;

  return (
    <div className="modal-overlay" onClick={e => e.target === e.currentTarget && onCancel()}>
      <div className="pm-shell" role="dialog" aria-modal="true" aria-labelledby="pm-dialog-title" ref={containerRef}>

        {/* ── PANEL 1: Payment type + amounts + change ─────────────────── */}
        <div className="pm-left">

          <div className="pm-header">
            <span className="pm-title">Checkout</span>
            <span id="pm-dialog-title" className="pm-total-badge">{DEVICE.currency} {fmt(netTotal)}</span>
          </div>

          <div className="pm-methods">
            {METHODS.map(m => (
              <button
                key={m.id}
                className={`pm-method${mainLine.method === m.id ? " pm-method-active" : ""}`}
                onClick={() => updateLine(mainLine.id, { method: m.id })}
              >
                <span className="pm-method-icon">{m.icon}</span>
                <span className="pm-method-label">{m.label}</span>
              </button>
            ))}
          </div>

          <div
            className={`pm-amount-box${activeField?.kind === "amount" ? " pm-field-active" : ""}`}
            onClick={() => setActiveField({ kind: "amount", lineId: mainLine.id })}
          >
            <span className="pm-amount-label">Amount</span>
            <span className="pm-amount-value">
              {mainLine.amountStr || formatMoney(netTotal, EXP)}
              {activeField?.kind === "amount" && <span className="pm-cursor">|</span>}
            </span>
          </div>

          {mainLine.method === "cash" && (
            <div
              className={`pm-amount-box pm-tendered-box${activeField?.kind === "tendered" ? " pm-field-active" : ""}`}
              onClick={() => setActiveField({ kind: "tendered", lineId: mainLine.id })}
            >
              <span className="pm-amount-label">Tendered</span>
              <span className="pm-amount-value">
                {mainLine.tenderedStr || mainLine.amountStr || "0"}
                {activeField?.kind === "tendered" && <span className="pm-cursor">|</span>}
              </span>
            </div>
          )}

          {/* Change pill — prominent green bar below tendered */}
          {change > 0 && (
            <div className="pm-change-pill">
              Change: {DEVICE.currency} {fmt(change)}
            </div>
          )}

          {mainLine.method === "cash" && (
            <div className="pm-quick">
              <button className="pm-quick-btn pm-quick-exact" onClick={() => applyQuick(netTotal)}>
                Exact <kbd>E</kbd>
              </button>
              {quickAmts.slice(0, 3).map(a => (
                <button key={a} className="pm-quick-btn" onClick={() => applyQuick(a)}>
                  {DEVICE.currency} {fmt(a)}
                </button>
              ))}
            </div>
          )}

          {/* Split payment lines */}
          {showSplit && lines.length > 1 && (
            <div className="pm-split-section">
              {lines.slice(1).map(line => (
                <div key={line.id} className="pm-split-line">
                  <select
                    className="pm-split-method"
                    value={line.method}
                    onChange={e => updateLine(line.id, { method: e.target.value as PaymentInput["method"] })}
                  >
                    <option value="card">💳 Card</option>
                    <option value="cash">💵 Cash</option>
                    <option value="wallet">📱 Wallet</option>
                    <option value="other">••• Other</option>
                  </select>
                  <div
                    className={`pm-amount-box pm-split-amount${activeField?.kind === "amount" && activeField.lineId === line.id ? " pm-field-active" : ""}`}
                    onClick={() => setActiveField({ kind: "amount", lineId: line.id })}
                  >
                    <span className="pm-amount-value" style={{fontSize: "1rem"}}>
                      {line.amountStr || "0"}
                      {activeField?.kind === "amount" && activeField.lineId === line.id && <span className="pm-cursor">|</span>}
                    </span>
                  </div>
                  <button className="pm-split-remove" onClick={() => removeLine(line.id)}>×</button>
                </div>
              ))}
              <div className="pm-split-remaining">
                Remaining: {DEVICE.currency} {fmt(remainingMinor)}
              </div>
            </div>
          )}

          {/* Due / change bar — only shown when split active */}
          {showSplit && (
            <div className={`pm-remaining${remainingMinor < 0 ? " pm-remaining-change" : remainingMinor === 0 ? " pm-remaining-ok" : ""}`}>
              {remainingMinor > 0 ? `Due: ${DEVICE.currency} ${fmt(remainingMinor)}` :
               remainingMinor < 0 ? `Change: ${DEVICE.currency} ${fmt(-remainingMinor)}` :
               "✓ Fully paid"}
            </div>
          )}
        </div>

        {/* ── PANEL 2: Customer + Delivery details ─────────────────────── */}
        <div className="pm-mid">
          <div className="pm-mid-title">Options</div>

          {/* Split payment toggle */}
          {!showSplit && (
            <button
              className="pm-split-toggle"
              onClick={() => { setShowSplit(true); setLines(p => [...p, mkLine("card")]); }}
            >
              + Split Payment
            </button>
          )}

          {/* Customer section */}
          <div className="pm-section">
            <button
              className={`pm-section-hdr${showCust ? " pm-section-hdr-open" : ""}${selectedCust ? " pm-section-hdr-active" : ""}`}
              onClick={() => setShowCust(v => !v)}
            >
              <span>👤 {selectedCust ? selectedCust.name.split(" ")[0] : "Customer"}</span>
              <span className="pm-chevron">{showCust ? "▲" : "▼"}</span>
            </button>
            {showCust && (
              <div className="pm-section-body">
                {selectedCust ? (
                  <div className="pm-cust-chip">
                    {selectedCust.name}{selectedCust.phone ? ` · ${selectedCust.phone}` : ""}
                    <span className="pm-cust-pts">{selectedCust.loyalty_points} pts</span>
                    <button className="pm-cust-remove" onClick={() => { setSelectedCust(null); setCustSearch(""); }}>×</button>
                  </div>
                ) : (
                  <input
                    className="pm-cust-search"
                    placeholder="Search name or phone…"
                    value={custSearch}
                    onChange={e => { setCustSearch(e.target.value); if (!e.target.value) setShowCustDrop(false); }}
                    onBlur={() => setTimeout(() => setShowCustDrop(false), 180)}
                  />
                )}
                {showCustDrop && custResults.length > 0 && (
                  <div className="pm-cust-drop">
                    {custResults.map(c => (
                      <button key={c.customer_id} className="pm-cust-drop-item"
                        onMouseDown={() => { setSelectedCust(c); setCustSearch(""); setShowCustDrop(false); }}>
                        <span>{c.name}</span>
                        {c.phone && <span className="pm-cust-dd-phone">{c.phone}</span>}
                      </button>
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>

          {/* Delivery section */}
          <div className="pm-section">
            <button
              className={`pm-section-hdr${showDelivery ? " pm-section-hdr-open pm-section-hdr-active" : ""}`}
              onClick={() => setShowDelivery(v => !v)}
            >
              <span>🛵 Delivery</span>
              <span className="pm-chevron">{showDelivery ? "▲" : "▼"}</span>
            </button>
            {showDelivery && (
              <div className="pm-section-body pm-delivery-body">
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

        {/* ── PANEL 3: Numpad + save ──────────────────────────────────── */}
        <div className="pm-right">
          <div className="pm-field-indicator">{activeLabel}</div>
          <Dialpad onKey={handleDialpadKey} />
          <div className="pm-actions">
            <button
              ref={confirmRef}
              className="pm-confirm-btn"
              onClick={handleConfirm}
              disabled={!canConfirm || loading}
            >
              {loading ? "Processing…" : `${DEVICE.currency} ${fmt(netTotal)}`}
            </button>
            <button className="pm-cancel-btn" onClick={onCancel} disabled={loading}>
              Cancel
            </button>
          </div>
        </div>

      </div>
    </div>
  );
}
