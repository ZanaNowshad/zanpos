import { Bot, CheckCircle2, Eye, Inbox, ReceiptText, RefreshCw, SearchCheck } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import type { CatalogImportProposal, PaymentConfirmation, WaMessage, SessionToken } from "../types";
import { DEVICE } from "../types";
import { catalogImportExtract, paymentConfirmationOverride } from "../tauri/commands";
import CatalogImportModal from "../components/CatalogImportModal";
import type { OfficeAiWorkflowInboxItem } from "./officeAiTypes";
import { officeAiWorkflowInbox } from "./officeAiData";
import { useLanguage } from "../hooks/useLanguage";
import { officeAiTranslator, type OfficeAiStringKey } from "../i18n/officeAiStrings";

interface Props {
  sessionToken: SessionToken;
  onSendPrompt: (prompt: string) => void;
}

function isMessage(source: OfficeAiWorkflowInboxItem["source"]): source is WaMessage {
  return Boolean(source && "chat_jid" in source);
}

function isPayment(source: OfficeAiWorkflowInboxItem["source"]): source is PaymentConfirmation {
  return Boolean(source && "receipt_number" in source);
}

export default function OfficeAIWorkflowInbox({ sessionToken, onSendPrompt }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => officeAiTranslator(language), [language]);
  const [items, setItems] = useState<OfficeAiWorkflowInboxItem[]>([]);
  const [filter, setFilter] = useState<"all" | "catalog" | "payment" | "whatsapp">("all");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [extractingId, setExtractingId] = useState<string | null>(null);
  const [proposal, setProposal] = useState<CatalogImportProposal | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setItems(await officeAiWorkflowInbox(sessionToken, t));
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
    } finally {
      setLoading(false);
    }
  }, [sessionToken, t]);

  useEffect(() => { void load(); }, [load]);

  const analyzeCatalog = async (item: OfficeAiWorkflowInboxItem) => {
    if (!isMessage(item.source)) return;
    setExtractingId(item.id);
    setError(null);
    try {
      const next = await catalogImportExtract(item.source.id, DEVICE.currency_exponent, sessionToken);
      setProposal(next);
    } catch (e) {
      setError(typeof e === "string" ? e : t("catalogExtractFailed"));
    } finally {
      setExtractingId(null);
    }
  };

  const overridePayment = async (item: OfficeAiWorkflowInboxItem, confirm: boolean) => {
    if (!isPayment(item.source)) return;
    setError(null);
    try {
      await paymentConfirmationOverride(item.source.id, confirm, sessionToken);
      await load();
    } catch (e) {
      setError(typeof e === "string" ? e : t("paymentOverrideFailed"));
    }
  };

  const filteredItems = items.filter(item => filter === "all" || item.kind === filter);
  const count = (kind: typeof filter) => kind === "all" ? items.length : items.filter(item => item.kind === kind).length;

  return (
    <div className="oa-workflow-inbox">
      <section className="oa-command-strip">
        <div className="oa-health-summary">
          <span className="oa-health-orb ok" />
          <div>
            <strong>{t("workflowInbox")}</strong>
            <span>{t("workflowInboxIntro")}</span>
          </div>
        </div>
        <button className="oa-command-tile oa-command-primary" onClick={load} disabled={loading}>
          <RefreshCw size={20} className={loading ? "oa-spin" : ""} />
          <span><strong>{t(loading ? "refreshingInbox" : "refreshInbox")}</strong><small>{t("refreshInboxHint")}</small></span>
        </button>
      </section>

      {error && <div className="oa-inline-warning">{error}</div>}

      <section className="oa-panel">
        <div className="oa-panel-header">
          <h2>{t("reviewQueue")}</h2>
          <span>{items.filter(i => i.unread).length} {t("unread")}</span>
        </div>
        <div className="oa-filter-line" role="tablist" aria-label={t("workflowFilters")}>
          {([
            ["all", "all"],
            ["catalog", "supplierBills"],
            ["payment", "BenefitPay"],
            ["whatsapp", "messages"],
          ] as Array<[typeof filter, OfficeAiStringKey | "BenefitPay"]>).map(([id, label]) => (
            <button
              key={id}
              role="tab"
              aria-selected={filter === id}
              className={`oa-filter-pill${filter === id ? " active" : ""}`}
              onClick={() => setFilter(id)}
            >
              {label === "BenefitPay" ? label : t(label)} <span>{count(id)}</span>
            </button>
          ))}
        </div>
        {filteredItems.length === 0 && !loading ? (
          <div className="oa-empty-state oa-empty-large">
            <Inbox size={24} />
            <strong>{t("noWorkflowItems")}</strong>
            <span>{t("noWorkflowItemsHint")}</span>
          </div>
        ) : (
          <div className="oa-list">
            {filteredItems.map(item => (
              <div key={item.id} className={`oa-workflow-row${item.unread ? " oa-workflow-unread" : ""}`}>
                <div className="oa-workflow-icon">
                  {item.kind === "payment" ? <ReceiptText size={17} /> : item.kind === "catalog" ? <SearchCheck size={17} /> : <Inbox size={17} />}
                </div>
                <span className="oa-workflow-copy">
                  <strong>{item.title}</strong>
                  <small>{item.detail}</small>
                  <em>{item.status}{item.timestamp ? ` · ${item.timestamp.slice(0, 16).replace("T", " ")}` : ""}</em>
                </span>
                <div className="oa-action-buttons">
                  {item.kind === "catalog" && (
                    <button className="oa-primary-mini" onClick={() => analyzeCatalog(item)} disabled={extractingId !== null}>
                      {t(extractingId === item.id ? "extracting" : "analyzeBill")}
                    </button>
                  )}
                  {item.kind === "payment" && isPayment(item.source) && item.source.status === "failed" && (
                    <>
                      <button className="oa-primary-mini" onClick={() => overridePayment(item, true)}>
                        {t("confirm")}
                      </button>
                      <button className="oa-ghost-mini" onClick={() => overridePayment(item, false)}>
                        {t("keepFailed")}
                      </button>
                    </>
                  )}
                  {isMessage(item.source) && (
                    <button
                      className="oa-ghost-mini"
                      onClick={() => {
                        const source = item.source as WaMessage;
                        onSendPrompt(`Review this WhatsApp message for store actions: ${source.body || item.detail}`);
                      }}
                    >
                      <Bot size={13} /> {t("askAiAction")}
                    </button>
                  )}
                  <button className="oa-ghost-mini" title={t("details")}>
                    <Eye size={13} />
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </section>

      {proposal && (
        <CatalogImportModal
          proposal={proposal}
          sessionToken={sessionToken}
          onClose={() => setProposal(null)}
          onApplied={() => {
            setProposal(null);
            void load();
          }}
        />
      )}
      {loading && <div className="oa-inline-success"><CheckCircle2 size={16} /><span>{t("loadingWorkflowSignals")}</span></div>}
    </div>
  );
}
