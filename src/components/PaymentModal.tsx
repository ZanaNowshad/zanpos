import { useState, useEffect, useRef } from "react";
import type { CustomerRow, PaymentInput } from "../types";
import { formatMoney, parseMoney } from "../money";
import { DEVICE } from "../types";
import * as cmd from "../tauri/commands";

interface Props {
  netTotal: number;
  onConfirm: (payments: PaymentInput[], customerId?: string) => void;
  onCancel: () => void;
  loading?: boolean;
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

export default function PaymentModal({ netTotal, onConfirm, onCancel, loading }: Props) {
  const [lines, setLines] = useState<PaymentLine[]>([mkLine("cash")]);
  const EXP = DEVICE.currency_exponent;
  const fmt = (n: number) => `${DEVICE.currency} ${formatMoney(n, EXP)}`;

  // Customer selection
  const [custSearch, setCustSearch]       = useState("");
  const [custResults, setCustResults]     = useState<CustomerRow[]>([]);
  const [selectedCust, setSelectedCust]   = useState<CustomerRow | null>(null);
  const [showCustDrop, setShowCustDrop]   = useState(false);
  const searchRef = useRef<ReturnType<typeof setTimeout> | null>(null);

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
    return allocatedMinor >= netTotal;
  })();

  const handleConfirm = () => {
    const payments: PaymentInput[] = lines.map(l => {
      const amount = parseMoney(l.amountStr, EXP);
      if (l.method === "cash") {
        const tendered = parseMoney(l.tenderedStr || l.amountStr, EXP);
        return { method: l.method, amount_minor: amount, tendered_minor: Math.max(tendered, amount) };
      }
      const ref = l.referenceStr.trim();
      return { method: l.method, amount_minor: amount, ...(ref ? { external_reference: ref } : {}) };
    });
    onConfirm(payments, selectedCust?.customer_id);
  };

  return (
    <div className="modal-overlay" onClick={e => e.target === e.currentTarget && onCancel()}>
      <div className="modal payment-modal">
        <h2>Payment</h2>

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

        <div className="payment-total">
          <span>Total due</span>
          <span className="payment-total-amount">{fmt(netTotal)}</span>
        </div>

        {/* Payment lines */}
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
                      min="0"
                      step="0.001"
                      placeholder={line.amountStr || "0.000"}
                      value={line.tenderedStr}
                      onChange={e => updateLine(line.id, { tenderedStr: e.target.value })}
                    />
                    {change > 0 && <span className="split-change">Change: {fmt(change)}</span>}
                  </div>
                )}
                {line.method !== "cash" && (
                  <div className="split-ref-row">
                    <label>Ref / Auth #</label>
                    <input
                      className="split-ref-input"
                      type="text"
                      placeholder="Card last 4 / approval code…"
                      value={line.referenceStr}
                      onChange={e => updateLine(line.id, { referenceStr: e.target.value })}
                    />
                  </div>
                )}
              </div>
            );
          })}
        </div>

        {/* Remaining / overage */}
        <div className={`split-remaining ${remainingMinor <= 0 ? "split-remaining-ok" : ""}`}>
          {remainingMinor > 0
            ? `Remaining: ${fmt(remainingMinor)}`
            : remainingMinor < 0
              ? `Overpaid by: ${fmt(-remainingMinor)}`
              : "Fully paid"}
        </div>

        <button className="split-add-btn" onClick={addLine}>+ Add payment method</button>

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
