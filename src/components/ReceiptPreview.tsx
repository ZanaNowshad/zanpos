import { useEffect, useState } from "react";
import type { BranchSettings, SaleResult } from "../types";
import { formatMoney } from "../money";
import { DEVICE } from "../types";
import { settingsGetBranch, printReceiptRaw, thermalGetConfig } from "../tauri/commands";
import { buildReceiptLines } from "../utils/receiptLines";

interface Props {
  sale: SaleResult;
  onNewSale: () => void;
  isReprint?: boolean;
  userId: string;
}

export default function ReceiptPreview({ sale, onNewSale, isReprint = false, userId }: Props) {
  const [settings, setSettings]       = useState<BranchSettings | null>(null);
  const [thermalEnabled, setThermalEnabled] = useState(false);
  const [printing, setPrinting]       = useState(false);
  const [printMsg, setPrintMsg]       = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    settingsGetBranch(userId).then(data => { if (!cancelled) setSettings(data); }).catch(() => {});
    thermalGetConfig(userId).then(c => { if (!cancelled) setThermalEnabled(c.enabled); }).catch(() => {});
    return () => { cancelled = true; };
  }, []);

  const fmt = (n: number) => formatMoney(n, DEVICE.currency_exponent);

  const handleHardwarePrint = async () => {
    setPrinting(true);
    setPrintMsg(null);
    try {
      const msg = await printReceiptRaw(userId, sale.branch_name, buildReceiptLines(sale, settings, isReprint));
      setPrintMsg(msg);
    } catch (e: unknown) {
      setPrintMsg(typeof e === "string" ? e : "Print failed");
    } finally {
      setPrinting(false);
    }
  };
  const soldAt = new Date(sale.sold_at).toLocaleString("en-BH", {
    timeZone: "Asia/Bahrain",
    year: "numeric", month: "short", day: "numeric",
    hour: "2-digit", minute: "2-digit",
  });

  return (
    <div className="modal-overlay">
      <div className="modal receipt-modal">
        <div className="receipt">
          {isReprint && (
            <div className="receipt-reprint-banner">*** DUPLICATE — NOT ORIGINAL ***</div>
          )}
          {/* ── Store header ── */}
          <div className="receipt-header">
            <div className="receipt-biz">{sale.branch_name}</div>
            {settings?.address && (
              <div className="receipt-addr">{settings.address}</div>
            )}
            {settings?.phone && (
              <div className="receipt-addr">📞 {settings.phone}</div>
            )}
            {settings?.tax_number && (
              <div className="receipt-addr">TRN: {settings.tax_number}</div>
            )}
            {settings?.receipt_header && (
              <div className="receipt-custom-header">{settings.receipt_header}</div>
            )}
            <div className="receipt-divider" />
            <div className="receipt-number">Receipt #{sale.receipt_number}</div>
            <div className="receipt-date">{soldAt}</div>
            <div className="receipt-cashier">Cashier: {sale.cashier_name}</div>
          </div>

          {/* ── Items ── */}
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

          {/* ── Totals ── */}
          <div className="receipt-totals">
            {sale.discount_total_minor > 0 && (
              <div className="receipt-row">
                <span>Discount</span>
                <span>− {DEVICE.currency} {fmt(sale.discount_total_minor)}</span>
              </div>
            )}
            {sale.tax_total_minor > 0 && (
              <>
                <div className="receipt-row">
                  <span>Subtotal (excl. VAT)</span>
                  <span>{DEVICE.currency} {fmt(sale.net_total_minor - sale.tax_total_minor)}</span>
                </div>
                <div className="receipt-row">
                  <span>VAT</span>
                  <span>{DEVICE.currency} {fmt(sale.tax_total_minor)}</span>
                </div>
              </>
            )}
            <div className="receipt-row receipt-total">
              <span>Total</span>
              <span>{DEVICE.currency} {fmt(sale.net_total_minor)}</span>
            </div>
          </div>

          {/* ── Payments ── */}
          <div className="receipt-payments">
            {sale.payments.map((p, i) => (
              <div key={i} className="receipt-row">
                <span>{p.method.charAt(0).toUpperCase() + p.method.slice(1)}</span>
                <span>{DEVICE.currency} {fmt(p.amount_minor)}</span>
              </div>
            ))}
            {(() => {
              const changePmt = sale.payments.find(p => p.change_minor != null && p.change_minor > 0);
              return changePmt ? (
                <div className="receipt-row receipt-change">
                  <span>Change</span>
                  <span>{DEVICE.currency} {fmt(changePmt.change_minor ?? 0)}</span>
                </div>
              ) : null;
            })()}
          </div>

          {/* ── Delivery section ── */}
          {sale.delivery && (() => {
            const d = sale.delivery!;
            const isPaid = d.payment_status === "paid";
            return (
              <div className="receipt-delivery-section">
                <div className={`receipt-delivery-banner${isPaid ? " receipt-delivery-banner-paid" : ""}`}>
                  {isPaid ? "✓ DELIVERY — PAID" : "⚠ DELIVERY — PAYMENT PENDING"}
                </div>
                {d.customer_name && <div className="receipt-delivery-row"><span>Customer</span><span>{d.customer_name}</span></div>}
                <div className="receipt-delivery-row"><span>Contact</span><span>{d.contact_number}</span></div>
                <div className="receipt-delivery-row"><span>Address</span><span>{[d.house_number, d.area, d.address_text].filter(Boolean).join(", ")}</span></div>
                {d.delivery_staff_name && <div className="receipt-delivery-row"><span>Rider</span><span>{d.delivery_staff_name}</span></div>}
                <div className="receipt-delivery-row">
                  <span>Expected</span>
                  <span>{d.expected_payment_method === "wallet" ? "BenefitPay" : d.expected_payment_method.charAt(0).toUpperCase() + d.expected_payment_method.slice(1)}</span>
                </div>
                {isPaid && d.paid_confirmed_at && (
                  <div className="receipt-delivery-row receipt-delivery-paid-at">
                    <span>Paid at</span><span>{new Date(d.paid_confirmed_at).toLocaleString()}</span>
                  </div>
                )}
              </div>
            );
          })()}

          {/* ── Footer ── */}
          {settings?.receipt_footer && (
            <div className="receipt-footer-text">{settings.receipt_footer}</div>
          )}

          {sale.created_offline && (
            <div className="receipt-offline-note">⚠ Created offline — pending sync</div>
          )}
        </div>

        {printMsg && (
          <div className="receipt-print-msg">{printMsg}</div>
        )}

        <div className="modal-actions">
          {thermalEnabled && (
            <button className="btn-secondary" onClick={handleHardwarePrint} disabled={printing}>
              {printing ? "Printing…" : "🖨 ESC/POS"}
            </button>
          )}
          <button className="btn-secondary" onClick={() => window.print()}>🖨 Browser Print</button>
          <button className="btn-primary" onClick={onNewSale}>New Sale</button>
        </div>
      </div>
    </div>
  );
}
