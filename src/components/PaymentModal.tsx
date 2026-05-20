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
  /** Pre-select a payment method, bypassing the "choose method" step. */
  initialMethod?: PaymentInput["method"];
  /** When true, open a second payment line for split payments immediately. */
  splitMode?: boolean;
  /** Logged-in user ID — forwarded to DeliveryForm for RBAC. */
  sessionUserId?: string;
}

interface PaymentLine {
  id: number;
  method: PaymentInput["method"];
  amountStr: string;
  tenderedStr: string;
  referenceStr: string;
}

let lineIdCounter = 1;

function mkLine(method: PaymentInput["method"] = "cash"): PaymentLine {
  return { id: lineIdCounter++, method, amountStr: "", tenderedStr: "", referenceStr: "" };
}

export default function PaymentModal({ netTotal, onConfirm, onCancel, loading, initialMethod, splitMode, sessionUserId }: Props) {
  const EXP = DEVICE.currency_exponent;
  const [lines, setLines] = useState<PaymentLine[]>(() => {
    const first = mkLine(initialMethod ?? "cash");
    if (!splitMode) {
      if (!initialMethod || initialMethod === "cash") {
        // Cash: pre-fill exact amount so cashier just confirms
        const exactStr = formatMoney(netTotal, DEVICE.currency_exponent);
        first.amountStr = exactStr;
        first.tenderedStr = exactStr;
      } else {
        // Card / wallet: pre-fill amount only
        first.amountStr = formatMoney(netTotal, DEVICE.currency_exponent);
      }
    }
    if (splitMode) return [first, mkLine("card")];
    return [first];
  });
  const fmt = (n: number) => `${DEVICE.currency} ${formatMoney(n, EXP)}`;
  // Customer selection
  const [custSearch, setCustSearch]       = useState("");
  const [custResults, setCustResults]     = useState<CustomerRow[]>([]);
  const [selectedCust, setSelectedCust]   = useState<CustomerRow | null>(null);
  const [showCustDrop, setShowCustDrop]   = useState(false);
  const searchRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Delivery state
  const [isDelivery, setIsDelivery] = useState(false);
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

  const updateLine = (id: number, patch: Partial<PaymentLine>) => {
    setLines(prev => prev.map(l => l.id === id ? { ...l, ...patch } : l));
  };
  const removeLine = (id: number) => setLines(prev => prev.filter(l => l.id !== id));
  const addLine = () => setLines(prev => [...prev, mkLine("cash")]);

  const canConfirm = (() => {
    if (lines.length === 0) return false;
    // Every line must have a positive amount
    for (const l of lines) {
      if (parseMoney(l.amountStr, EXP) <= 0) return false;
      if (l.method === "cash" && parseMoney(l.tenderedStr || l.amountStr, EXP) < parseMoney(l.amountStr, EXP)) return false;
    }
    // Total must cover the net total (cash lines may exceed — change will be given)
    if (allocatedMinor < netTotal) return false;
    // Delivery validation
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
    let deliveryInput: DeliveryInput | undefined = undefined;
    if (isDelivery) {
      deliveryInput = {
        ...(deliveryData as DeliveryInput),
        expected_payment_method: lines[0]?.method ?? "cash",
      };
    }
    onConfirm(payments, selectedCust?.customer_id, deliveryInput);
  };

  // Quick-amount suggestions for cash — fixed denominations
  const quickAmounts: number[] = (() => {
    const unit = Math.pow(10, EXP);
    return [5, 10, 30]
      .map(major => Math.round(major * unit))
      .filter(minor => minor > netTotal);
  })();

  const applyQuickAmount = (minor: number) => {
    // Apply to the first cash line (or all lines if single)
    const cashLine = lines.find(l => l.method === "cash");
    if (!cashLine) return;
    const majorStr = formatMoney(minor, EXP);
    updateLine(cashLine.id, { amountStr: majorStr, tenderedStr: majorStr });
  };

  return (
    <div className="modal-overlay" onClick={e => e.target === e.currentTarget && onCancel()}>
      <div className="modal payment-modal">
        <div className="modal-header">
          <span className="modal-title">Payment</span>
        </div>

        {/* ── Customer picker ── */}
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
                    <button
                      key={c.customer_id}
                      className="cust-dropdown-item"
                      onMouseDown={() => { setSelectedCust(c); setCustSearch(""); setShowCustDrop(false); }}
                    >
                      <span className="cust-dd-name">{c.name}</span>
                      {c.phone && <span className="cust-dd-phone">{c.phone}</span>}
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>

        {/* ── Giant total — impossible to miss ── */}
        <div className="payment-total">
          <span className="payment-total-label">Total Due</span>
          <span className="payment-total-amount">{fmt(netTotal)}</span>
        </div>

        {/* ── Quick cash amounts ── */}
        {lines.length === 1 && lines[0].method === "cash" && (
          <div className="quick-amounts">
            {quickAmounts.map(minor => (
              <button
                key={minor}
                className="quick-amt-btn"
                onClick={() => applyQuickAmount(minor)}
                title={`Round up to ${fmt(minor)}`}
              >
                {fmt(minor)}
              </button>
            ))}
          </div>
        )}

        {/* ── Payment lines ── */}
        <div className="split-lines">
          {lines.map((line, idx) => {
            const amt = parseMoney(line.amountStr, EXP);
            const tendered = parseMoney(line.tenderedStr || line.amountStr, EXP);
            const change = line.method === "cash" && tendered > amt ? tendered - amt : 0;
            return (
              <div key={line.id} className="split-line">
                <div className="split-line-row">
                  <select
                    className="split-method-select"
                    value={line.method}
                    onChange={e => updateLine(line.id, { method: e.target.value as PaymentInput["method"] })}
                  >
                    <option value="cash">Cash</option>
                    <option value="card">Card</option>
                    <option value="wallet">Wallet</option>
                    <option value="other">Other</option>
                  </select>
                  <input
                    className="split-amount-input"
                    type="number"
                    inputMode="decimal"
                    min="0"
                    step="0.001"
                    placeholder={idx === 0 && lines.length === 1 ? formatMoney(netTotal, EXP) : "0.000"}
                    value={line.amountStr}
                    onChange={e => updateLine(line.id, { amountStr: e.target.value })}
                    autoFocus={idx === 0}
                  />
                  {lines.length > 1 && (
                    <button className="split-remove-btn" onClick={() => removeLine(line.id)} title="Remove">×</button>
                  )}
                </div>
                {line.method === "cash" && (
                  <div className="split-cash-row">
                    <label>Tendered</label>
                    <input
                      className="split-tendered-input"
                      type="number"
                      inputMode="decimal"
                      min="0"
                      step="0.001"
                      placeholder={line.amountStr || "0.000"}
                      value={line.tenderedStr}
                      onChange={e => updateLine(line.id, { tenderedStr: e.target.value })}
                    />
                    {change > 0 && <span className="split-change">Change: {fmt(change)}</span>}
                  </div>
                )}
              </div>
            );
          })}
        </div>

        {/* ── Remaining / change — prominent feedback ── */}
        <div className={`split-remaining ${
          remainingMinor < 0 ? "split-remaining-change" :
          remainingMinor === 0 ? "split-remaining-ok" : ""
        }`}>
          {remainingMinor > 0
            ? `Still owed: ${fmt(remainingMinor)}`
            : remainingMinor < 0
              ? `Change due: ${fmt(-remainingMinor)}`
              : "✓ Fully paid"}
        </div>

        <button className="split-add-btn" onClick={addLine}>+ Add payment method</button>

        {/* ── Delivery toggle ── */}
        <div className="delivery-toggle-row">
          <label className="delivery-toggle-label">
            <input
              type="checkbox"
              className="delivery-toggle-cb"
              checked={isDelivery}
              onChange={e => setIsDelivery(e.target.checked)}
            />
            <span>🛵 Mark as Delivery</span>
          </label>
          {isDelivery && <span className="delivery-toggle-hint">Payment will be pending until confirmed by manager</span>}
        </div>

        {/* ── Delivery form ── */}
        {isDelivery && (
          <DeliveryForm
            value={deliveryData}
            onChange={setDeliveryData}
            selectedCustomer={selectedCust}
            expectedPaymentMethod={lines[0]?.method ?? "cash"}
            actorUserId={sessionUserId ?? DEVICE.device_id}
          />
        )}

        <div className="modal-actions">
          <button className="btn-secondary" onClick={onCancel} disabled={loading}>Cancel</button>
          <button
            className="btn-primary"
            onClick={handleConfirm}
            disabled={!canConfirm || loading}
          >
            {loading ? "Processing…" : "Confirm Payment"}
          </button>
        </div>
      </div>
    </div>
  );
}
