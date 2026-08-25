import { useEffect, useRef } from "react";
import type { AdminProduct } from "../types";
import { DEVICE } from "../types";
import { formatMoney } from "../money";
import JsBarcode from "jsbarcode";
import { useTranslation } from "react-i18next";
import ModalShell from "./modal/ModalShell";
import { ModalActions } from "./modal/ModalParts";

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
  const { t } = useTranslation("modal");
  const exp = DEVICE.currency_exponent;
  const currency = DEVICE.currency;

  return (
    /* `barcodes-modal` is kept purely so the print stylesheet can still name
       this dialog and hide everything that is not the label sheet. */
    <ModalShell
      kicker="Catalogue"
      title={t("printLabels")}
      subtitle={`${products.length} ${t("labelsReadyToPrint")}`}
      size="xl"
      className="barcodes-modal"
      onClose={onClose}
      footer={
        <ModalActions note="Labels print three across at 50 x 30mm.">
          <button type="button" className="btn-secondary" onClick={onClose}>{t("cancel")}</button>
          <button type="button" className="btn-primary" onClick={() => window.print()}>
            {t("printLabels")}
          </button>
        </ModalActions>
      }
    >
      <div className="barcode-labels-grid" id="barcode-print-area">
        {products.map(p => (
          <BarcodeLabel key={p.product_id} product={p} exp={exp} currency={currency} />
        ))}
      </div>
    </ModalShell>
  );
}
