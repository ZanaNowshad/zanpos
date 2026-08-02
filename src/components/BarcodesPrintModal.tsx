import { useEffect, useMemo, useRef } from "react";
import type { AdminProduct } from "../types";
import { DEVICE } from "../types";
import { formatMoney } from "../money";
import JsBarcode from "jsbarcode";
import { useLanguage } from "../hooks/useLanguage";
import { modalTranslator } from "../i18n/modalStrings";

interface Props {
  products: AdminProduct[];
  onClose: () => void;
}

interface LabelProps {
  product: AdminProduct;
  exp: number;
  currency: string;
}

function BarcodeLabel({ product, exp, currency }: LabelProps) {
  const svgRef = useRef<SVGSVGElement>(null);

  const barcodeValue =
    product.barcode ?? product.sku ?? product.product_id;

  useEffect(() => {
    if (!svgRef.current) return;
    try {
      JsBarcode(svgRef.current, barcodeValue, {
        format: "CODE128",
        width: 1.5,
        height: 40,
        displayValue: true,
        fontSize: 10,
        margin: 4,
      });
    } catch {
      // Ignore invalid barcode characters — show the product_id fallback
      try {
        JsBarcode(svgRef.current, product.product_id, {
          format: "CODE128",
          width: 1.5,
          height: 40,
          displayValue: true,
          fontSize: 10,
          margin: 4,
        });
      } catch {
        // Completely swallow if all fail
      }
    }
  }, [barcodeValue, product.product_id]);

  return (
    <div className="barcode-label">
      <div className="barcode-label-name">{product.name}</div>
      <svg ref={svgRef} className="barcode-label-svg" />
      <div className="barcode-label-price">
        {currency} {formatMoney(product.price_minor, exp)}
      </div>
    </div>
  );
}

export default function BarcodesPrintModal({ products, onClose }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => modalTranslator(language), [language]);
  const exp = DEVICE.currency_exponent;
  const currency = DEVICE.currency;

  return (
    <button className="modal-overlay" type="button" onClick={e => e.target === e.currentTarget && onClose()}>
      <div className="modal barcodes-modal">
        <div className="barcodes-modal-header">
          <h2>{t("printLabels")}</h2>
          <div className="barcodes-modal-actions">
            <button className="btn-secondary" onClick={onClose}>{t("cancel")}</button>
            <button className="btn-primary" onClick={() => window.print()}>{t("printLabels")}</button>
          </div>
        </div>
        <p className="barcodes-hint">
          {products.length} {t("labelsReadyToPrint")}
        </p>
        <div className="barcode-labels-grid" id="barcode-print-area">
          {products.map(p => (
            <BarcodeLabel key={p.product_id} product={p} exp={exp} currency={currency} />
          ))}
        </div>
      </div>
    </div>
  );
}
