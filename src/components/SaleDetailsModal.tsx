import { X } from "lucide-react";
import type { SaleResult } from "../types";
import { DEVICE } from "../types";
import { formatMoney } from "../money";
import { useMemo } from "react";
import { useLanguage } from "../hooks/useLanguage";
import { modalTranslator, ownedModalLabel } from "../i18n/modalStrings";

interface Props {
  sale: SaleResult;
  onClose: () => void;
}

export default function SaleDetailsModal({ sale, onClose }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => modalTranslator(language), [language]);
  const fmt = (value: number) => `${DEVICE.currency} ${formatMoney(value, DEVICE.currency_exponent)}`;

  return (
    <button className="modal-overlay" type="button" onClick={e => e.target === e.currentTarget && onClose()}>
      <div className="sale-details-modal" role="dialog" aria-modal="true" aria-labelledby="sale-details-title">
        <header>
          <div>
            <span>{t("receipt")}</span>
            <h2 id="sale-details-title">{t("sale")} #{sale.receipt_number}</h2>
          </div>
          <button onClick={onClose} aria-label={t("close")}><X size={18} /></button>
        </header>
        <section className="sale-details-meta">
          <div><span>{t("cashier")}</span><strong>{sale.cashier_name}</strong></div>
          <div><span>{t("date")}</span><strong>{new Date(sale.sold_at).toLocaleString()}</strong></div>
          <div><span>{t("total")}</span><strong>{fmt(sale.net_total_minor)}</strong></div>
        </section>
        <div className="sale-details-list">
          {sale.items.length === 0 ? (
            <p>{t("noReceiptLines")}</p>
          ) : sale.items.map((item, index) => (
            <div key={`${item.product_name}-${index}`} className="sale-details-row">
              <div>
                <strong>{item.product_name}</strong>
                <span>{t("quantity")} {item.quantity} · {fmt(item.unit_price_minor)}</span>
              </div>
              <b>{fmt(item.line_total_minor)}</b>
            </div>
          ))}
        </div>
        <footer>
          <div><span>{t("discounts")}</span><strong>{fmt(sale.discount_total_minor)}</strong></div>
          <div><span>{t("taxCollected")}</span><strong>{fmt(sale.tax_total_minor)}</strong></div>
          <div><span>{t("paid")}</span><strong>{sale.payments.map(p => ownedModalLabel(language, p.method)).join(", ") || t("recorded")}</strong></div>
        </footer>
      </div>
    </button>
  );
}
