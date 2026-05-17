import { useEffect, useState } from "react";
import type { BranchSettings, SaleResult } from "../types";
import { formatMoney } from "../money";
import { DEVICE } from "../types";
import { settingsGetBranch, printReceiptRaw, thermalGetConfig } from "../tauri/commands";

interface Props {
  sale: SaleResult;
  onNewSale: () => void;
}

export default function ReceiptPreview({ sale, onNewSale }: Props) {
  const [settings, setSettings]       = useState<BranchSettings | null>(null);
  const [thermalEnabled, setThermalEnabled] = useState(false);
  const [printing, setPrinting]       = useState(false);
  const [printMsg, setPrintMsg]       = useState<string | null>(null);

  useEffect(() => {
    settingsGetBranch().then(setSettings).catch(() => {});
    thermalGetConfig().then(c => setThermalEnabled(c.enabled)).catch(() => {});
  }, []);

  const fmt = (n: number) => formatMoney(n, DEVICE.currency_exponent);

  /** Build plain-text receipt lines for ESC/POS (48-char width). */
  const buildTextLines = (): string[] => {
    const W = 48;
    const divider = "-".repeat(W);
    const pad = (l: string, r: string) => {
      const space = W - l.length - r.length;
      return l + " ".repeat(Math.max(1, space)) + r;
    };

    const lines: string[] = [];
    if (settings?.address)      lines.push(settings.address);
    if (settings?.phone)        lines.push(settings.phone);
    if (settings?.tax_number)   lines.push(`Tax Reg: ${settings.tax_number}`);
    if (settings?.receipt_header) lines.push(settings.receipt_header);
    lines.push(divider);
    lines.push(`Receipt: #${sale.receipt_number}`);
    lines.push(new Date(sale.sold_at).toLocaleString([], { dateStyle: "short", timeStyle: "short" }));
    lines.push(`Cashier: ${sale.cashier_name}`);
    lines.push(divider);

    for (const item of sale.items) {
      const left  = `${item.product_name} x${item.quantity}`;
      const right = `${DEVICE.currency} ${fmt(item.line_total_minor)}`;
      lines.push(pad(left, right));
    }

    lines.push(divider);
    if (sale.discount_total_minor > 0)
      lines.push(pad("Discount", `- ${DEVICE.currency} ${fmt(sale.discount_total_minor)}`));
    if (sale.tax_total_minor > 0)
      lines.push(pad("Tax", `${DEVICE.currency} ${fmt(sale.tax_total_minor)}`));
    lines.push(pad("TOTAL", `${DEVICE.currency} ${fmt(sale.net_total_minor)}`));
    lines.push(divider);

    for (const p of sale.payments) {
      const method = p.method.charAt(0).toUpperCase() + p.method.slice(1);
      lines.push(pad(method, `${DEVICE.currency} ${fmt(p.amount_minor)}`));
      if (p.change_minor && p.change_minor > 0)
        lines.push(pad("Change", `${DEVICE.currency} ${fmt(p.change_minor)}`));
    }

    if (settings?.receipt_footer) { lines.push(divider); lines.push(settings.receipt_footer); }
    return lines;
  };

  const handleHardwarePrint = async () => {
    setPrinting(true);
    setPrintMsg(null);
    try {
      const msg = await printReceiptRaw(sale.branch_name, buildTextLines());
      setPrintMsg(msg);
    } catch (e: unknown) {
      setPrintMsg(typeof e === "string" ? e : "Print failed");
    } finally {
      setPrinting(false);
    }
  };
  const soldAt = new Date(sale.sold_at).toLocaleString([], {
    year: "numeric", month: "short", day: "numeric",
    hour: "2-digit", minute: "2-digit",
  });

  return (
    <div className="modal-overlay">
      <div className="modal receipt-modal">
        <div className="receipt">
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
              <div className="receipt-addr">Tax Reg: {settings.tax_number}</div>
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
              <div className="receipt-row">
                <span>Tax</span>
                <span>{DEVICE.currency} {fmt(sale.tax_total_minor)}</span>
              </div>
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
            {sale.payments.find(p => p.change_minor && p.change_minor > 0) && (
              <div className="receipt-row receipt-change">
                <span>Change</span>
                <span>{DEVICE.currency} {fmt(sale.payments.find(p => p.change_minor)!.change_minor!)}</span>
              </div>
            )}
          </div>

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
