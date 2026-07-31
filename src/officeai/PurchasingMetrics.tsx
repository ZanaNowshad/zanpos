import { PackagePlus, Truck, Users } from "lucide-react";
import type { MarginSummary } from "../types";
import { useLanguage } from "../hooks/useLanguage";
import { operationsTranslator } from "../i18n/operationsStrings";
import type { PurchasingCommandModel } from "./purchasingPresentation";
import { purchasingMoney } from "./purchasingPresentation";

interface Props {
  model: PurchasingCommandModel;
  margin: MarginSummary | null;
  currencyExp: number;
}

export default function PurchasingMetrics({ model, margin, currencyExp }: Props) {
  const { language } = useLanguage();
  const t = operationsTranslator(language);
  return (
    <section className="oa-metric-grid">
      <div className="oa-metric-card">
        <div className="oa-card-label"><Users size={15} /> {t("suppliers")}</div>
        <div className="oa-big-number">{model.activeSupplierCount}</div>
        <div className="oa-card-sub">{model.supplierCount} {t("totalSupplierRecords")}</div>
      </div>
      <div className="oa-metric-card">
        <div className="oa-card-label"><Truck size={15} /> {t("openPos")}</div>
        <div className="oa-big-number">{model.openPoCount}</div>
        <div className="oa-card-sub">{purchasingMoney(model.receivingValueMinor, currencyExp)} {t("waitingToReceive")}</div>
      </div>
      <div className="oa-metric-card oa-metric-wide">
        <div className="oa-card-label"><PackagePlus size={15} /> {t("grossMargin")}</div>
        <div className="oa-big-number">{margin ? purchasingMoney(margin.gross_margin_minor, currencyExp) : t("loading")}</div>
        <div className="oa-metric-row">
          <span>{t("revenue")} {purchasingMoney(margin?.revenue_minor ?? 0, currencyExp)}</span>
          <span>{t("cogs")} {purchasingMoney(margin?.cogs_minor ?? 0, currencyExp)}</span>
          <span>{model.unknownCostLineCount} {t("unknownCostLines")}</span>
        </div>
      </div>
    </section>
  );
}
