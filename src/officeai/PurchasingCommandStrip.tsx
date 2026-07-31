import { ClipboardCheck, PackagePlus } from "lucide-react";
import { useLanguage } from "../hooks/useLanguage";
import { operationsTranslator } from "../i18n/operationsStrings";

interface Props {
  marginWarning: boolean;
  onSendPrompt: (prompt: string) => void;
}

export default function PurchasingCommandStrip({ marginWarning, onSendPrompt }: Props) {
  const { language } = useLanguage();
  const t = operationsTranslator(language);

  return (
    <section className="oa-command-strip">
      <div className="oa-health-summary">
        <span className={`oa-health-orb ${marginWarning ? "warning" : "ok"}`} />
        <div>
          <strong>{t("purchasingCommandView")}</strong>
          <span>{t("purchasingCommandDescription")}</span>
        </div>
      </div>
      <button className="oa-command-tile oa-command-primary" onClick={() => onSendPrompt("Suggest reorder purchase orders from low stock, sales velocity, and supplier history.")}>
        <PackagePlus size={20} />
        <span><strong>{t("suggestReorderPos")}</strong><small>{t("suggestReorderDescription")}</small></span>
      </button>
      <button className="oa-command-tile" onClick={() => onSendPrompt("Review purchase bill images and prepare catalog, supplier, stock, and cost proposals.")}>
        <ClipboardCheck size={20} />
        <span><strong>{t("reviewSupplierBills")}</strong><small>{t("reviewSupplierBillsDescription")}</small></span>
      </button>
    </section>
  );
}
