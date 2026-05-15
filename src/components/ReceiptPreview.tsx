import type { SaleResult } from "../types";
import { formatMoney } from "../money";
import { DEVICE } from "../types";

interface Props {
  sale: SaleResult;
  onNewSale: () => void;
}

export default function ReceiptPreview({ sale, onNewSale }: Props) {
  const fmt = (n: number) => formatMoney(n, DEVICE.currency_exponent);
  const soldAt = new Date(sale.sold_at).toLocaleString();

  return (
    <div className="modal-overlay">
      <div className="modal receipt-modal">
        <div className="receipt">
          <div className="receipt-header">
            <div className="receipt-biz">{sale.branch_name}</div>
            <div className="receipt-number">Receipt #{sale.receipt_number}</div>
            <div className="receipt-date">{soldAt}</div>
            <div className="receipt-cashier">Cashier: {sale.cashier_name}</div>
          </div>

          <div className="receipt-items">
            {sale.items.map((item, i) => (
              <div key={i} className="receipt-item">
                <span className="receipt-item-name">{item.product_name}</span>
                <span className="receipt-item-qty">× {item.quantity}</span>
                <span className="receipt-item-total">
                  {DEVICE.currency} {fmt(item.line_total_minor)}
                </span>
              </div>
            ))}
          </div>

          <div className="receipt-totals">
            {sale.tax_total_minor > 0 && (
              <div className="receipt-row">
                <span>Tax</span>
                <span>{DEVICE.currency} {fmt(sale.tax_total_minor)}</span>
              </div>
            )}
            {sale.discount_total_minor > 0 && (
              <div className="receipt-row">
                <span>Discount</span>
                <span>− {DEVICE.currency} {fmt(sale.discount_total_minor)}</span>
              </div>
            )}
            <div className="receipt-row receipt-total">
              <span>Total</span>
              <span>{DEVICE.currency} {fmt(sale.net_total_minor)}</span>
            </div>
          </div>

          <div className="receipt-payments">
            {sale.payments.map((p, i) => (
              <div key={i} className="receipt-row">
                <span>{p.method.charAt(0).toUpperCase() + p.method.slice(1)}</span>
                <span>{DEVICE.currency} {fmt(p.amount_minor)}</span>
              </div>
            ))}
            {sale.payments.find(p => p.change_minor && p.change_minor > 0) && (
              <div className="receipt-row receipt-change">
                <span>Change</span>
                <span>{DEVICE.currency} {fmt(sale.payments.find(p => p.change_minor)!.change_minor!)}</span>
              </div>
            )}
          </div>

          {sale.created_offline && (
            <div className="receipt-offline-note">⚠ Created offline — pending sync</div>
          )}
        </div>

        <div className="modal-actions">
          <button className="btn-secondary" onClick={() => window.print()}>Print</button>
          <button className="btn-primary" onClick={onNewSale}>New Sale</button>
        </div>
      </div>
    </div>
  );
}
