import { useState } from "react";
import type { SaleForRefund, SaleItemForRefund, RefundResult } from "../types";
import { DEVICE } from "../types";
import { refundGetSale, refundCreate } from "../tauri/commands";
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

export default function RefundModal({ cashierUserId, onClose }: Props) {
  const [receiptInput, setReceiptInput] = useState("");
  const [sale, setSale]                 = useState<SaleForRefund | null>(null);
  // qty = 0 means "don't refund this line"
  const [refundQtys, setRefundQtys]     = useState<QtyMap>(new Map());
  const [reason, setReason]             = useState("");
  const [reasonCode, setReasonCode]     = useState("customer_return");
  const [result, setResult]             = useState<RefundResult | null>(null);
  const [searching, setSearching]       = useState(false);
  const [submitting, setSubmitting]     = useState(false);
  const [error, setError]               = useState<string | null>(null);

  const handleSearch = async () => {
    if (!receiptInput.trim()) return;
    setSearching(true);
    setError(null);
    setSale(null);
    setRefundQtys(new Map());
    try {
      const found = await refundGetSale(receiptInput.trim().toUpperCase());
      setSale(found);
      setRefundQtys(initQtyMap(found.items));
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Receipt not found");
    } finally {
      setSearching(false);
    }
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
      );
      setResult(refund);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Refund failed");
    } finally {
      setSubmitting(false);
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

        <div className="refund-search">
          <input
            className="field-input"
            placeholder="Receipt number (e.g. MAIN-POS01-00000001)"
            value={receiptInput}
            onChange={e => setReceiptInput(e.target.value)}
            onKeyDown={e => e.key === "Enter" && handleSearch()}
          />
          <button className="modal-btn-secondary" onClick={handleSearch} disabled={searching}>
            {searching ? "…" : "Search"}
          </button>
        </div>

        {error && <div className="modal-error">{error}</div>}

        {sale && (
          <>
            <div className="refund-sale-info">
              <span className="refund-receipt">{sale.receipt_number}</span>
              <span className="refund-cashier">{sale.cashier_name}</span>
              <span className={`refund-status refund-status-${sale.status}`}>{sale.status}</span>
            </div>

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
      </div>
    </div>
  );
}
