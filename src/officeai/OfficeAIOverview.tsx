import { ArrowRight, CheckCircle2, CircleAlert, RefreshCw } from "lucide-react";
import type { OfficeAiOverviewSnapshot, OfficeTab } from "./officeAiTypes";
import { buildOfficePulseModel } from "./officeAiData";
import { useLanguage } from "../hooks/useLanguage";
import { officeAiTranslator } from "../i18n/officeAiStrings";

interface Props {
  snapshot: OfficeAiOverviewSnapshot;
  currencyExp: number;
  canUseManagerTools: boolean;
  pendingActionCount: number;
  onOpenTab: (tab: OfficeTab) => void;
  onRefresh: () => void;
}

export function splitBidiNumericText(text: string): string[] {
  return text.split(/((?:[A-Z]{3}\s*)?[+-]?\d[\d.,]*(?:\s?%)?)/g).filter(Boolean);
}

function BidiText({ text }: { text: string }) {
  return splitBidiNumericText(text).map((part, index) =>
    /\d/.test(part)
      ? <bdi key={index} className="numeric-ltr" dir="ltr">{part}</bdi>
      : part
  );
}

export default function OfficeAIOverview({
  snapshot,
  currencyExp,
  canUseManagerTools,
  pendingActionCount,
  onOpenTab,
  onRefresh,
}: Props) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  if (snapshot.loading && !snapshot.refreshedAt) {
    return (
      <div className="oa-home oa-home-loading" aria-label={t("loadingStorePulse")}>
        <div className="oa-skeleton oa-skeleton-title" />
        <div className="oa-signal-grid">
          {[0, 1, 2, 3].map(index => <div key={index} className="oa-skeleton oa-skeleton-signal" />)}
        </div>
        <div className="oa-skeleton oa-skeleton-list" />
      </div>
    );
  }

  const model = buildOfficePulseModel(snapshot, canUseManagerTools ? pendingActionCount : 0, currencyExp, t);

  return (
    <div className="oa-home">
      <section className="oa-home-intro" aria-labelledby="oa-store-pulse-title">
        <span className="oa-eyebrow">{t("storePulse")}</span>
        <h2 id="oa-store-pulse-title"><BidiText text={model.summary} /></h2>
        {snapshot.refreshedAt && (
          <p>{t("updated")} {new Date(snapshot.refreshedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</p>
        )}
      </section>

      {snapshot.errors.length > 0 && (
        <div className="oa-home-partial" role="status">
          <CircleAlert size={16} />
          <span><strong>{t("signalsUnavailable")}</strong> {t("restCurrent")}</span>
          <button onClick={onRefresh}><RefreshCw size={14} /> {t("retry")}</button>
        </div>
      )}

      <section className="oa-signal-grid" aria-label={t("storeSignals")}>
        {model.signals.map(signal => (
          <div className="oa-signal" key={signal.id}>
            <span>{signal.label}</span>
            <strong className="numeric-ltr">{signal.value}</strong>
            <small><BidiText text={signal.detail} /></small>
          </div>
        ))}
      </section>

      <section className="oa-attention" aria-labelledby="oa-attention-title">
        <div className="oa-section-heading">
          <div>
            <h2 id="oa-attention-title">{t("needsAttention")}</h2>
            <p>{t("attentionHint")}</p>
          </div>
          {snapshot.loading && <span className="oa-quiet-status"><RefreshCw size={13} className="oa-spin" /> {t("updating")}</span>}
        </div>

        {model.attention.length > 0 ? (
          <div className="oa-attention-list">
            {model.attention.map(item => (
              <div className={`oa-attention-row oa-attention-${item.severity}`} key={item.id}>
                <CircleAlert size={17} />
                <div>
                  <strong>{item.title}</strong>
                  <p><BidiText text={item.detail} /></p>
                </div>
                <button onClick={() => onOpenTab(item.destination)}>
                  {item.actionLabel}<ArrowRight className="icon-directional" size={14} />
                </button>
              </div>
            ))}
          </div>
        ) : (
          <div className="oa-all-normal">
            <CheckCircle2 size={17} />
            <span><strong>{t("allSystemsNormal")}</strong><small>{t("noActionNeeded")}</small></span>
          </div>
        )}
      </section>
    </div>
  );
}
