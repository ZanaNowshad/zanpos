import { useState, useEffect, useRef } from "react";
import type { CustomerRow, DeliveryInput, PaymentInput } from "../types";
import { formatMoney, parseMoney } from "../money";
import { DEVICE } from "../types";
import * as cmd from "../tauri/commands";
import DeliveryForm from "./DeliveryForm";

interface Props {
  netTotal: number;
  onConfirm: (payments: PaymentInput[], customerId?: string, delivery?: DeliveryInput) => void;
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
  referenceStr: string;
}

type ActiveField = { lineId: number; field: "amount" | "tendered" } | null;

const METHOD_OPTIONS: { id: PaymentInput["method"]; icon: string; label: string }[] = [
  { id: "cash",   icon: "💵", label: "Cash"   },
  { id: "card",   icon: "💳", label: "Card"   },
  { id: "wallet", icon: "📱", label: "Wallet" },
  { id: "other",  icon: "•••", label: "Other" },
];

let lineIdCounter = 1;

function mkLine(method: PaymentInput["method"] = "cash"): PaymentLine {
  return { id: lineIdCounter++, method, amountStr: "", tenderedStr: "", referenceStr: "" };
}

// ── Dialpad component ──────────────────────────────────────────────────────
function Dialpad({ onKey }: { onKey: (k: string) => void }) {
  const rows = [
    ["7", "8", "9"],
    ["4", "5", "6"],
    ["1", "2", "3"],
    ["C", "0", "⌫"],
  ];
  return (
    <div className="dialpad">
      {rows.map((row, ri) => (
        <div key={ri} className="dialpad-row">
          {row.map(k => (
            <button
              key={k}
              className={`dialpad-key${k === "⌫" ? " dialpad-key-back" : ""}${k === "C" ? " dialpad-key-clear" : ""}`}
              onMouseDown={e => { e.preventDefault(); onKey(k); }}
            >
              {k}
            </button>
          ))}
        </div>
      ))}
      {/* Wide decimal key */}
      <div className="dialpad-row">
        <button className="dialpad-key dialpad-key-wide" onMouseDown={e => { e.preventDefault(); onKey("."); }}>.</button>
        <button className="dialpad-key dialpad-key-wide" onMouseDown={e => { e.preventDefault(); onKey("00"); }}>00</button>
      </div>
    </div>
  );
}

// ── Main component ─────────────────────────────────────────────────────────
export default function PaymentModal({ netTotal, onConfirm, onCancel, loading, initialMethod, splitMode, sessionUserId }: Props) {
  const EXP = DEVICE.currency_exponent;
  const [lines, setLines] = useState<PaymentLine[]>(() => {
    const first = mkLine(initialMethod ?? "cash");
    if (!splitMode) {
      if (!initialMethod || initialMethod === "cash") {
        const exactStr = formatMoney(netTotal, DEVICE.currency_exponent);
        first.amountStr = exactStr;
        first.tenderedStr = exactStr;
      } else {
        first.amountStr = formatMoney(netTotal, DEVICE.currency_exponent);
      }
    }
    if (splitMode) return [first, mkLine("card")];
    return [first];
  });

  const fmt = (n: number) => `${DEVICE.currency} ${formatMoney(n, EXP)}`;

  // Active field for dialpad targeting
  const [activeField, setActiveField] = useState<ActiveField>(() =>
    ({ lineId: 1, field: "amount" })
  );

  // Customer selection
  const [custSearch, setCustSearch]     = useState("");
  const [custResults, setCustResults]   = useState<CustomerRow[]>([]);
  const [selectedCust, setSelectedCust] = useState<CustomerRow | null>(null);
  const [showCustDrop, setShowCustDrop] = useState(false);
  const searchRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Delivery state
  const [isDelivery, setIsDelivery]     = useState(false);
  const [deliveryData, setDeliveryData] = useState<Partial<DeliveryInput>>({});

  useEffect(() => {
    if (custSearch.trim().length === 0) { setCustResults([]); return; }
    if (searchRef.current) clearTimeout(searchRef.current);
    searchRef.current = setTimeout(async () => {
      try {
        const rows = await cmd.customerList(custSearch.trim());
        setCustResults(rows.slice(0, 8));
        setShowCustDrop(true);
      } catch { /* ignore */ }
    }, 250);
    return () => { if (searchRef.current) clearTimeout(searchRef.current); };
  }, [custSearch]);

  const allocatedMinor = lines.reduce((sum, l) => {
    const amt = parseMoney(l.amountStr, EXP);
    return sum + (amt > 0 ? amt : 0);
  }, 0);
  const remainingMinor = netTotal - allocatedMinor;

  const updateLine = (id: number, patch: Partial<PaymentLine>) =>
    setLines(prev => prev.map(l => l.id === id ? { ...l, ...patch } : l));
  const removeLine = (id: number) => setLines(prev => prev.filter(l => l.id !== id));
  const addLine    = () => setLines(prev => [...prev, mkLine("cash")]);

  // ── Dialpad key handler ────────────────────────────────────────────────
  const handleDialpadKey = (key: string) => {
    const target = activeField ?? { lineId: lines[0]?.id, field: "amount" as const };
    if (!target) return;
    const line = lines.find(l => l.id === target.lineId);
    if (!line) return;

    const current = target.field === "amount" ? line.amountStr : line.tenderedStr;

    let next: string;
    if (key === "⌫") {
      next = current.slice(0, -1);
    } else if (key === "C") {
      next = "";
    } else if (key === ".") {
      if (current.includes(".")) return;
      next = (current || "0") + ".";
    } else if (key === "00") {
      next = current.includes(".") ? current : current + "00";
    } else {
      // digit
      if (current === "0") next = key;
      else next = current + key;
    }

    if (target.field === "amount") {
      updateLine(target.lineId, { amountStr: next });
    } else {
      updateLine(target.lineId, { tenderedStr: next });
    }
  };

  const canConfirm = (() => {
    if (lines.length === 0) return false;
    for (const l of lines) {
      if (parseMoney(l.amountStr, EXP) <= 0) return false;
      if (l.method === "cash" && parseMoney(l.tenderedStr || l.amountStr, EXP) < parseMoney(l.amountStr, EXP)) return false;
    }
    if (allocatedMinor < netTotal) return false;
    if (isDelivery) {
      if (!deliveryData.contact_number) return false;
      if (!deliveryData.address_text?.trim()) return false;
    }
    return true;
  })();

  const handleConfirm = () => {
    const payments: PaymentInput[] = lines.map(l => {
      const amount = parseMoney(l.amountStr, EXP);
      if (l.method === "cash") {
        const tendered = parseMoney(l.tenderedStr || l.amountStr, EXP);
        return { method: l.method, amount_minor: amount, tendered_minor: Math.max(tendered, amount) };
      }
      return { method: l.method, amount_minor: amount };
    });
    let deliveryInput: DeliveryInput | undefined;
    if (isDelivery) {
      deliveryInput = {
        ...(deliveryData as DeliveryInput),
        expected_payment_method: lines[0]?.method ?? "cash",
      };
    }
    onConfirm(payments, selectedCust?.customer_id, deliveryInput);
  };

  // Quick-amount suggestions
  const quickAmounts: number[] = (() => {
    const unit = Math.pow(10, EXP);
    return [5, 10, 30]
      .map(major => Math.round(major * unit))
      .filter(minor => minor > netTotal);
  })();

  const applyQuickAmount = (minor: number) => {
    const cashLine = lines.find(l => l.method === "cash");
    if (!cashLine) return;
    const majorStr = formatMoney(minor, EXP);
    updateLine(cashLine.id, { amountStr: majorStr, tenderedStr: majorStr });
  };

  // ── Remaining display ──────────────────────────────────────────────────
  const remainingCls = `split-remaining ${
    remainingMinor < 0 ? "split-remaining-change" :
    remainingMinor === 0 ? "split-remaining-ok" : ""
  }`;
  const remainingText = remainingMinor > 0
    ? `Still owed: ${fmt(remainingMinor)}`
    : remainingMinor < 0 ? `Change due: ${fmt(-remainingMinor)}`
    : "✓ Fully paid";

  return (
    <div className="modal-overlay" onClick={e => e.target === e.currentTarget && onCancel()}>
      <div className="payment-shell">

        {/* ── Left: Payment form ────────────────────────────────────── */}
        <div className="modal payment-modal payment-modal-touch">
          <div className="modal-header">
            <span className="modal-title">Payment</span>
          </div>

          {/* Customer picker */}
          <div className="cust-picker-section">
            <label className="cust-picker-label">Customer (optional)</label>
            {selectedCust ? (
              <div className="cust-chip">
                <span>{selectedCust.name}{selectedCust.phone ? ` · ${selectedCust.phone}` : ""}</span>
                <span className="cust-chip-pts">{selectedCust.loyalty_points} pts</span>
                <button className="cust-chip-remove" onClick={() => { setSelectedCust(null); setCustSearch(""); }}>×</button>
              </div>
            ) : (
              <div className="cust-search-wrap">
                <input
                  className="cust-search-input"
                  placeholder="Search by name or phone…"
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

          {/* Total */}
          <div className="payment-total">
            <span className="payment-total-label">Total Due</span>
            <span className="payment-total-amount">{fmt(netTotal)}</span>
          </div>

          {/* Quick amounts */}
          {lines.length === 1 && lines[0].method === "cash" && quickAmounts.length > 0 && (
            <div className="quick-amounts">
              {quickAmounts.map(minor => (
                <button key={minor} className="quick-amt-btn"
                  onClick={() => applyQuickAmount(minor)}>
                  {fmt(minor)}
                </button>
              ))}
            </div>
          )}

          {/* Payment lines */}
          <div className="split-lines">
            {lines.map((line, idx) => {
              const amt     = parseMoney(line.amountStr, EXP);
              const tendered = parseMoney(line.tenderedStr || line.amountStr, EXP);
              const change  = line.method === "cash" && tendered > amt ? tendered - amt : 0;
              const isActiveAmt = activeField?.lineId === line.id && activeField.field === "amount";
              const isActiveTen = activeField?.lineId === line.id && activeField.field === "tendered";
              return (
                <div key={line.id} className="split-line">
                  {/* Method cards */}
                  <div className="method-cards">
                    {METHOD_OPTIONS.map(m => (
                      <button
                        key={m.id}
                        className={`method-card${line.method === m.id ? " method-card-active" : ""}`}
                        onClick={() => updateLine(line.id, { method: m.id })}
                      >
                        <span className="method-card-icon">{m.icon}</span>
                        <span className="method-card-label">{m.label}</span>
                      </button>
                    ))}
                    {lines.length > 1 && (
                      <button className="split-remove-btn" onClick={() => removeLine(line.id)} title="Remove">×</button>
                    )}
                  </div>

                  {/* Amount input */}
                  <div className={`touch-input-row${isActiveAmt ? " touch-input-active" : ""}`}
                    onClick={() => setActiveField({ lineId: line.id, field: "amount" })}>
                    <span className="touch-input-label">Amount</span>
                    <span className="touch-input-value">
                      {line.amountStr || <span className="touch-input-placeholder">{idx === 0 && lines.length === 1 ? formatMoney(netTotal, EXP) : "0.000"}</span>}
                    </span>
                    {isActiveAmt && <span className="touch-cursor">|</span>}
                  </div>

                  {/* Tendered (cash only) */}
                  {line.method === "cash" && (
                    <div className={`touch-input-row${isActiveTen ? " touch-input-active" : ""}`}
                      onClick={() => setActiveField({ lineId: line.id, field: "tendered" })}>
                      <span className="touch-input-label">Tendered</span>
                      <span className="touch-input-value">
                        {line.tenderedStr || <span className="touch-input-placeholder">{line.amountStr || "0.000"}</span>}
                      </span>
                      {isActiveTen && <span className="touch-cursor">|</span>}
                      {change > 0 && <span className="split-change">Change {fmt(change)}</span>}
                    </div>
                  )}
                </div>
              );
            })}
          </div>

          {/* Remaining */}
          <div className={remainingCls}>{remainingText}</div>

          <button className="split-add-btn" onClick={addLine}>+ Add payment method</button>

          {/* Delivery toggle */}
          <div className="delivery-toggle-row">
            <label className="delivery-toggle-label">
              <input type="checkbox" className="delivery-toggle-cb"
                checked={isDelivery} onChange={e => setIsDelivery(e.target.checked)} />
              <span>🛵 Mark as Delivery</span>
            </label>
            {isDelivery && <span className="delivery-toggle-hint">Payment will be pending until confirmed by manager</span>}
          </div>

          {isDelivery && (
            <DeliveryForm
              value={deliveryData}
              onChange={setDeliveryData}
              selectedCustomer={selectedCust}
              expectedPaymentMethod={lines[0]?.method ?? "cash"}
              actorUserId={sessionUserId ?? DEVICE.device_id}
            />
          )}
        </div>

        {/* ── Right: Dialpad panel ──────────────────────────────────── */}
        <div className="payment-dialpad-panel">
          {/* Active field indicator */}
          <div className="dialpad-field-indicator">
            {activeField?.field === "tendered" ? "Entering: Tendered" : "Entering: Amount"}
          </div>

          <Dialpad onKey={handleDialpadKey} />

          <div className="dialpad-actions">
            <button
              className="dialpad-confirm-btn"
              onClick={handleConfirm}
              disabled={!canConfirm || loading}
            >
              {loading ? "Processing…" : (
                <>
                  <span className="dialpad-confirm-icon">✓</span>
                  <span className="dialpad-confirm-label">Confirm</span>
                  <span className="dialpad-confirm-total">{fmt(netTotal)}</span>
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
