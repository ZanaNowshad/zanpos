import { useState } from "react";
import type { SaleForRefund, SaleItemForRefund, RefundResult } from "../types";
import { DEVICE } from "../types";
import { refundGetSale, refundCreate } from "../tauri/commands";
import { formatMoney } from "../money";

interface Props {
  cashierUserId: string;
  onClose: () => void;
}

export default function RefundModal({ cashierUserId, onClose }: Props) {
  const [receiptInput, setReceiptInput] = useState("");
  const [sale, setSale] = useState<SaleForRefund | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [reason, setReason] = useState("");
  const [result, setResult] = useState<RefundResult | null>(null);
  const [searching, setSearching] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSearch = async () => {
    if (!receiptInput.trim()) return;
    setSearching(true);
    setError(null);
    setSale(null);
    setSelected(new Set());
    try {
      const found = await refundGetSale(receiptInput.trim().toUpperCase());
      setSale(found);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Receipt not found");
    } finally {
      setSearching(false);
    }
  };

  const toggleItem = (item: SaleItemForRefund) => {
    setSelected(prev => {
      const next = new Set(prev);
      if (next.has(item.sale_item_id)) next.delete(item.sale_item_id);
      else next.add(item.sale_item_id);
      return next;
    });
  };

  const refundTotal = sale
    ? sale.items.filter(i => selected.has(i.sale_item_id)).reduce((s, i) => s + i.line_total_minor, 0)
    : 0;

  const handleConfirm = async () => {
    if (!sale || selected.size === 0) return;
    setSubmitting(true);
    setError(null);
    try {
      const items = sale.items
        .filter(i => selected.has(i.sale_item_id))
        .map(i => ({
          sale_item_id: i.sale_item_id,
          product_name_snapshot: i.product_name_snapshot,
          quantity: i.quantity,
          unit_price_minor: i.unit_price_minor,
          refund_amount_minor: i.line_total_minor,
        }));
      const refund = await refundCreate(sale.sale_id, items, reason || "Customer return", cashierUserId);
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
              {sale.items.map(item => (
                <label key={item.sale_item_id} className="refund-item">
                  <input
                    type="checkbox"
                    checked={selected.has(item.sale_item_id)}
                    onChange={() => toggleItem(item)}
                  />
                  <span className="refund-item-name">{item.product_name_snapshot}</span>
                  <span className="refund-item-qty">×{item.quantity}</span>
                  <span className="refund-item-total">
                    {DEVICE.currency} {formatMoney(item.line_total_minor, DEVICE.currency_exponent)}
                  </span>
                </label>
              ))}
            </div>

            <input
              className="field-input"
              placeholder="Reason (optional)"
              value={reason}
              onChange={e => setReason(e.target.value)}
            />

            <div className="refund-footer">
              <div className="refund-total-line">
                Refund total: <strong>{DEVICE.currency} {formatMoney(refundTotal, DEVICE.currency_exponent)}</strong>
              </div>
              <button
                className="modal-btn-danger"
                onClick={handleConfirm}
                disabled={submitting || selected.size === 0}
              >
                {submitting ? "Processing…" : `Refund ${selected.size} item${selected.size !== 1 ? "s" : ""}`}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
