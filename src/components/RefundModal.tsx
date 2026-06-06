import { useState } from "react";
import type { SaleForRefund, SaleItemForRefund, SaleListRow, RefundResult } from "../types";
import { DEVICE } from "../types";
import { refundGetSale, refundCreate, reportSalesList, authValidateManagerPin } from "../tauri/commands";
import { formatMoney } from "../money";

interface Props {
  cashierUserId: string;
  onClose: () => void;
}

const REASON_CODE_LABELS: Record<string, string> = {
  customer_return: "Customer return / changed mind",
  defective:       "Defective / damaged",
  wrong_item:      "Wrong item delivered",
  exchange:        "Exchange",
  other:           "Other",
};

/** Quantity map: sale_item_id → how many units to refund (0 = skip). */
type QtyMap = Map<string, number>;

function initQtyMap(items: SaleItemForRefund[]): QtyMap {
  const m = new Map<string, number>();
  for (const item of items) {
    m.set(item.sale_item_id, parseFloat(item.quantity) || 0);
  }
  return m;
}

type RefundMode = "receipt" | "browse";

function todayStr() {
  return new Date().toLocaleDateString("en-CA", { timeZone: "Asia/Bahrain" });
}

export default function RefundModal({ cashierUserId, onClose }: Props) {
  const [mode, setMode]               = useState<RefundMode>("receipt");
  const [receiptInput, setReceiptInput] = useState("");
  const [sale, setSale]               = useState<SaleForRefund | null>(null);
  const [refundQtys, setRefundQtys]   = useState<QtyMap>(new Map());
  const [reason, setReason]           = useState("");
  const [reasonCode, setReasonCode]   = useState("customer_return");
  const [result, setResult]           = useState<RefundResult | null>(null);
  const [searching, setSearching]     = useState(false);
  const [submitting, setSubmitting]   = useState(false);
  const [error, setError]             = useState<string | null>(null);
  // Browse mode state
  const [browseDate, setBrowseDate]   = useState(todayStr());
  const [salesList, setSalesList]     = useState<SaleListRow[]>([]);
  const [listLoading, setListLoading] = useState(false);
  // Cross-device manager override
  const [showPinEntry, setShowPinEntry]     = useState(false);
  const [managerPin, setManagerPin]         = useState("");
  const [pinError, setPinError]             = useState<string | null>(null);
  const [pinLoading, setPinLoading]         = useState(false);
  const [overrideToken, setOverrideToken]   = useState<string | null>(null);

  const isCrossDevice = sale ? (sale.origin_device_id && sale.origin_device_id !== (DEVICE as any).device_id) : false;

  const handleSearch = async () => {
    if (!receiptInput.trim()) return;
    setSearching(true);
    setError(null);
    setSale(null);
    setRefundQtys(new Map());
    try {
      const found = await refundGetSale(receiptInput.trim().toUpperCase(), cashierUserId);
      setSale(found);
      setRefundQtys(initQtyMap(found.items));
    } catch (e: unknown) {
      const msg = typeof e === "string" ? e : "Receipt not found";
      setError(msg.toLowerCase().includes("not permitted") || msg.toLowerCase().includes("permission")
        ? "Only managers and owners can look up receipts."
        : msg);
    } finally {
      setSearching(false);
    }
  };

  const loadBrowseList = async (d: string) => {
    setListLoading(true);
    setError(null);
    setSalesList([]);
    setSale(null);
    setRefundQtys(new Map());
    try {
      const page = await reportSalesList(cashierUserId, DEVICE.branch_id, d, d);
      setSalesList(page.items.filter(r => r.status !== "voided"));
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to load sales");
    } finally {
      setListLoading(false);
    }
  };

  const handleBrowseDateChange = (d: string) => {
    setBrowseDate(d);
    loadBrowseList(d);
  };

  const handleBrowseSelect = async (row: SaleListRow) => {
    setSearching(true);
    setError(null);
    setSale(null);
    setRefundQtys(new Map());
    try {
      const found = await refundGetSale(row.receipt_number, cashierUserId);
      setSale(found);
      setRefundQtys(initQtyMap(found.items));
    } catch (e: unknown) {
      const msg = typeof e === "string" ? e : "Failed to load sale";
      setError(msg.toLowerCase().includes("not permitted") || msg.toLowerCase().includes("permission")
        ? "Only managers and owners can look up receipts."
        : msg);
    } finally {
      setSearching(false);
    }
  };

  const switchMode = (m: RefundMode) => {
    setMode(m);
    setError(null);
    setSale(null);
    setRefundQtys(new Map());
    setReceiptInput("");
    if (m === "browse") loadBrowseList(browseDate);
  };

  const setQty = (saleItemId: string, raw: string) => {
    const n = Math.max(0, parseInt(raw, 10) || 0);
    setRefundQtys(prev => new Map(prev).set(saleItemId, n));
  };

  const toggleAll = (item: SaleItemForRefund) => {
    const current = refundQtys.get(item.sale_item_id) ?? 0;
    const max = Math.floor(parseFloat(item.quantity));
    setRefundQtys(prev => new Map(prev).set(item.sale_item_id, current === 0 ? max : 0));
  };

  const selectedItems = sale
    ? sale.items.filter(i => (refundQtys.get(i.sale_item_id) ?? 0) > 0)
    : [];

  const refundTotal = selectedItems.reduce((sum, item) => {
    const qty    = refundQtys.get(item.sale_item_id) ?? 0;
    return sum + qty * item.unit_price_minor;
  }, 0);

  const handleConfirm = async () => {
    if (!sale || selectedItems.length === 0) return;
    setSubmitting(true);
    setError(null);
    try {
      const items = selectedItems.map(i => {
        const qty = refundQtys.get(i.sale_item_id) ?? 0;
        return {
          sale_item_id:          i.sale_item_id,
          product_name_snapshot: i.product_name_snapshot,
          quantity:              qty.toString(),
          unit_price_minor:      i.unit_price_minor,
          refund_amount_minor:   qty * i.unit_price_minor,
        };
      });
      const refund = await refundCreate(
        sale.sale_id,
        items,
        reason || REASON_CODE_LABELS[reasonCode] || "Customer return",
        cashierUserId,
        reasonCode,
        overrideToken ?? undefined,
      );
      setResult(refund);
      setOverrideToken(null);
    } catch (e: unknown) {
      const msg = typeof e === "string" ? e : "Refund failed";
      if (msg.toLowerCase().includes("manager override") || msg.toLowerCase().includes("manager pin")) {
        setShowPinEntry(true);
        setManagerPin("");
        setPinError(null);
      } else if (msg.toLowerCase().includes("not permitted") || msg.toLowerCase().includes("permission")) {
        setError("Only managers and owners can process refunds.");
      } else {
        setError(msg);
      }
    } finally {
      setSubmitting(false);
    }
  };

  const handlePinSubmit = async () => {
    if (!managerPin.trim()) return;
    setPinLoading(true);
    setPinError(null);
    try {
      const token = await authValidateManagerPin(managerPin);
      setOverrideToken(token);
      setShowPinEntry(false);
      setManagerPin("");
      // Re-trigger refund with the token
      handleConfirm();
    } catch (e: unknown) {
      const msg = typeof e === "string" ? e : "Invalid PIN";
      setPinError(msg.includes("Invalid") || msg.includes("Permission") ? "Invalid manager PIN" : msg);
    } finally {
      setPinLoading(false);
    }
  };

  if (result) {
    return (
      <div className="modal-overlay">
        <div className="modal refund-modal">
          <div className="refund-success">
            <div className="refund-success-icon">✓</div>
            <h2>Refund Complete</h2>
            <div className="refund-receipt-num">{result.refund_receipt_number}</div>
            <div className="refund-amount">
              {DEVICE.currency} {formatMoney(result.refund_total_minor, DEVICE.currency_exponent)}
            </div>
            <button className="modal-btn-primary" onClick={onClose}>Done</button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal refund-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2 className="modal-title">Refund</h2>
          <button className="modal-close" onClick={onClose}>✕</button>
        </div>

        {/* Mode selector */}
        <div className="refund-mode-tabs">
          <button
            className={`refund-mode-tab ${mode === "receipt" ? "refund-mode-tab-active" : ""}`}
            onClick={() => switchMode("receipt")}
          >🧾 By Receipt</button>
          <button
            className={`refund-mode-tab ${mode === "browse" ? "refund-mode-tab-active" : ""}`}
            onClick={() => switchMode("browse")}
          >📋 Browse Sales</button>
        </div>

        {mode === "receipt" && (
          <div className="refund-search">
            <input
              className="field-input"
              placeholder="Receipt number (e.g. MAIN-POS01-00000001)"
              value={receiptInput}
              onChange={e => setReceiptInput(e.target.value)}
              onKeyDown={e => e.key === "Enter" && handleSearch()}
              autoFocus
            />
            <button className="modal-btn-secondary" onClick={handleSearch} disabled={searching}>
              {searching ? "…" : "Search"}
            </button>
          </div>
        )}

        {mode === "browse" && !sale && (
          <div className="refund-browse-section">
            <div className="refund-browse-header">
              <label>Date:</label>
              <input
                className="field-input refund-browse-date"
                type="date"
                value={browseDate}
                onChange={e => handleBrowseDateChange(e.target.value)}
              />
            </div>
            {listLoading && <div className="refund-browse-loading">Loading…</div>}
            <div className="refund-browse-list">
              {salesList.map(row => (
                <button
                  key={row.sale_id}
                  className="refund-browse-row"
                  onClick={() => handleBrowseSelect(row)}
                  disabled={searching}
                >
                  <span className="refund-br-receipt">{row.receipt_number}</span>
                  <span className="refund-br-time">
                    {new Date(row.sold_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
                  </span>
                  <span className="refund-br-cashier">{row.cashier_name}</span>
                  <span className="refund-br-total">
                    {DEVICE.currency} {formatMoney(row.net_total_minor, DEVICE.currency_exponent)}
                  </span>
                </button>
              ))}
              {!listLoading && salesList.length === 0 && (
                <div className="refund-browse-empty">No refundable sales for {browseDate}</div>
              )}
            </div>
            {searching && <div className="refund-browse-loading">Loading sale…</div>}
          </div>
        )}

        {mode === "browse" && sale && (
          <div className="refund-browse-back">
            <button className="modal-btn-secondary" onClick={() => { setSale(null); setRefundQtys(new Map()); }}>
              ← Back to list
            </button>
            <span className="refund-receipt">{sale.receipt_number}</span>
          </div>
        )}

        {error && <div className="modal-error">{error}</div>}

        {sale && (
          <>
            <div className="refund-sale-info">
              <span className="refund-receipt">{sale.receipt_number}</span>
              <span className="refund-cashier">{sale.cashier_name}</span>
              <span className={`refund-status refund-status-${sale.status}`}>{sale.status}</span>
            </div>

            {isCrossDevice && (
              <div className="refund-cross-device-banner">
                This sale was made on another device. Manager override is required for refund.
              </div>
            )}

            <div className="refund-items">
              {sale.items.map(item => {
                const maxQty  = Math.floor(parseFloat(item.quantity));
                const current = refundQtys.get(item.sale_item_id) ?? 0;
                const lineRefund = current * item.unit_price_minor;

                return (
                  <div key={item.sale_item_id} className={`refund-item ${current > 0 ? "refund-item-selected" : ""}`}>
                    {/* Checkbox toggles full quantity */}
                    <input
                      type="checkbox"
                      checked={current > 0}
                      onChange={() => toggleAll(item)}
                    />

                    <span className="refund-item-name">{item.product_name_snapshot}</span>

                    {/* Quantity stepper */}
                    <div className="refund-qty-stepper">
                      <button
                        className="refund-qty-btn"
                        onClick={() => setQty(item.sale_item_id, String(current - 1))}
                        disabled={current <= 0}
                      >−</button>
                      <input
                        className="refund-qty-input"
                        type="number"
                        min={0}
                        max={maxQty}
                        value={current}
                        onChange={e => setQty(item.sale_item_id, e.target.value)}
                      />
                      <button
                        className="refund-qty-btn"
                        onClick={() => setQty(item.sale_item_id, String(current + 1))}
                        disabled={current >= maxQty}
                      >+</button>
                      <span className="refund-qty-of">/ {maxQty}</span>
                    </div>

                    <span className="refund-item-total">
                      {DEVICE.currency} {formatMoney(lineRefund, DEVICE.currency_exponent)}
                    </span>
                  </div>
                );
              })}
            </div>

            <select
              className="field-input refund-reason-select"
              value={reasonCode}
              onChange={e => setReasonCode(e.target.value)}
            >
              {Object.entries(REASON_CODE_LABELS).map(([code, label]) => (
                <option key={code} value={code}>{label}</option>
              ))}
            </select>

            <input
              className="field-input"
              placeholder="Additional notes (optional)"
              value={reason}
              onChange={e => setReason(e.target.value)}
            />

            <div className="refund-footer">
              <div className="refund-total-line">
                Refund total: <strong>
                  {DEVICE.currency} {formatMoney(refundTotal, DEVICE.currency_exponent)}
                </strong>
              </div>
              <button
                className="modal-btn-danger"
                onClick={handleConfirm}
                disabled={submitting || selectedItems.length === 0}
              >
                {submitting
                  ? "Processing…"
                  : `Refund ${selectedItems.length} line${selectedItems.length !== 1 ? "s" : ""}`}
              </button>
            </div>
          </>
        )}
        {showPinEntry && (
          <div className="modal-overlay pin-overlay" onClick={() => setShowPinEntry(false)}>
            <div className="modal pin-modal" onClick={e => e.stopPropagation()}>
              <div className="modal-header">
                <h3>Manager Override Required</h3>
                <button className="modal-close" onClick={() => setShowPinEntry(false)}>✕</button>
              </div>
              <p className="pin-explain">
                Cross-device refunds require a manager or owner to authorise.
                Enter your PIN to proceed.
              </p>
              <input
                className="field-input"
                type="password"
                inputMode="numeric"
                maxLength={6}
                placeholder="Manager PIN"
                value={managerPin}
                onChange={e => setManagerPin(e.target.value)}
                onKeyDown={e => e.key === "Enter" && handlePinSubmit()}
                autoFocus
              />
              {pinError && <div className="modal-error">{pinError}</div>}
              <div className="pin-actions">
                <button className="modal-btn-secondary" onClick={() => setShowPinEntry(false)}>Cancel</button>
                <button className="modal-btn-primary" onClick={handlePinSubmit} disabled={pinLoading || !managerPin.trim()}>
                  {pinLoading ? "Verifying…" : "Authorise"}
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
