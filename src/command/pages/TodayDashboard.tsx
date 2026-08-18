import { ArrowRight, CheckCircle2, CircleAlert, RefreshCw } from "lucide-react";
import type { ReactNode } from "react";
import type { OfficeAiOverviewSnapshot, OfficeTab } from "../../officeai/officeAiTypes";
import { buildOfficePulseModel } from "../../officeai/officeAiData";
import { useLanguage } from "../../hooks/useLanguage";
import { commandTranslator, type CommandStringKey } from "../../i18n/commandStrings";
import { EmptyState, PageHeader } from "../../components/templates";
import ContextualAskBar from "../ContextualAskBar";
import CapabilityStrip from "../CapabilityStrip";
import { allNormal, buildCapabilities } from "../capabilities";

// ─── Bidi helper ────────────────────────────────────────────────────────────
function splitBidiNumericText(text: string): string[] {
  return text.split(/((?:[A-Z]{3}\s*)?[+-]?\d[\d.,]*(?:\s?%)?)/g).filter(Boolean);
}

function BidiText({ text }: { text: string }) {
  // The plain-text branch returns the string itself rather than a fragment:
  // an unkeyed <>…</> inside a mapped array is what produced React's "unique
  // key" warning, and a fragment wrapping a single string adds nothing.
  return splitBidiNumericText(text).map((part, i) =>
    /\d/.test(part) ? <bdi key={i} className="numeric-ltr" dir="ltr">{part}</bdi> : part
  ) as unknown as ReactNode;
}

// ─── Props ──────────────────────────────────────────────────────────────────
interface Props {
  snapshot: OfficeAiOverviewSnapshot;
  /** Real branch name from settings_get_branch; blank until it loads. */
  storeName?: string;
  currencyExp: number;
  canUseManagerTools: boolean;
  pendingActionCount: number;
  onOpenTab: (tab: OfficeTab) => void;
  onRefresh: () => void;
  onSendPrompt?: (prompt: string) => void;
}

export default function TodayDashboard({
  snapshot,
  storeName,
  currencyExp,
  canUseManagerTools,
  pendingActionCount,
  onOpenTab,
  onRefresh,
  onSendPrompt,
}: Props) {
  const { language } = useLanguage();
  const t = commandTranslator(language);

  // ── Loading skeleton ────────────────────────────────────────────────────
  if (snapshot.loading && !snapshot.refreshedAt) {
    return (
      <div className="oa-home oa-home-loading" aria-label={t("loadingStorePulse" as CommandStringKey)}>
        <div className="oa-skeleton oa-skeleton-title" />
        <div className="oa-signal-grid">
          {[0, 1, 2, 3].map((i) => <div key={i} className="oa-skeleton oa-skeleton-signal" />)}
        </div>
        <div className="oa-skeleton oa-skeleton-list" />
      </div>
    );
  }

  const model = buildOfficePulseModel(snapshot, canUseManagerTools ? pendingActionCount : 0, currencyExp, t as never);
  const capabilities = buildCapabilities(snapshot);

  /**
   * Capability state belongs to the strip above; the attention list is for
   * things needing a decision. Without this split WhatsApp appeared three
   * times on one screen — once in the strip and twice in attention, because
   * the pulse model and the health findings each emit a row for it.
   */
  const capabilityOwned = new Set(
    capabilities
      .filter(c => c.severity !== "ok" && c.severity !== "info")
      .map(c => c.id),
  );
  const attentionPrefix: Record<string, string> = {
    sync: "sync", whatsapp: "whatsapp", ai: "ai", storefront: "storefront", devices: "devices",
  };
  const attention = model.attention.filter(item => {
    // ids are either "<area>:<code>" or "health:<area>:<code>".
    const parts = String(item.id).split(":");
    const area = parts[0] === "health" ? parts[1] : parts[0];
    const capId = attentionPrefix[area];
    return !(capId && capabilityOwned.has(capId as never));
  });

  /**
   * Suggestions follow what is actually on the page. Offering "explain these
   * sync issues" on a healthy day is noise; offering it while 11 events are
   * queued is the question the user was about to ask. Capped at three so the
   * chips never become a second navigation bar.
   */
  const askSuggestions: { label: string; prompt: string }[] = [];
  const degraded = capabilities.filter(c => c.severity !== "ok" && c.severity !== "info");
  if ((snapshot.today?.transaction_count ?? 0) > 0) {
    askSuggestions.push({ label: "Explain today's sales", prompt: "Summarize today's sales and highlight anything unusual." });
  }
  if (attention.length > 0) {
    askSuggestions.push({ label: "What needs my attention?", prompt: "Explain the exceptions on my Today screen and what I should do first." });
  }
  if (degraded.some(c => c.id === "sync")) {
    askSuggestions.push({ label: "Explain these sync issues", prompt: "Explain the pending sync events and whether any data is at risk." });
  }
  if (snapshot.outOfStockCount > 0 || snapshot.lowStockCount > 0) {
    askSuggestions.push({ label: "Show low-stock products", prompt: "Which products are out of stock or below their reorder level?" });
  }
  if (askSuggestions.length === 0) {
    askSuggestions.push({ label: "How is the store doing?", prompt: "Give me a short summary of the store's current position." });
  }
  askSuggestions.length = Math.min(askSuggestions.length, 3);

  // All-clear requires BOTH no exceptions and every capability healthy —
  // otherwise a disconnected WhatsApp could hide behind "all systems normal".
  if (model.allSystemsNormal && attention.length === 0 && allNormal(capabilities)) {
    return (
      <div className="oa-home">
        <PageHeader
          title={t("today" as CommandStringKey)}
          subtitle={t("allSystemsNormal" as CommandStringKey)}
          icon={<CheckCircle2 size={18} strokeWidth={1.75} />}
          secondaryActions={[{ label: t("refresh" as CommandStringKey), onClick: onRefresh }]}
        />
        <EmptyState
          icon={<CheckCircle2 size={32} strokeWidth={1.5} />}
          title={t("allSystemsNormal" as CommandStringKey)}
          description={t("noActionNeeded" as CommandStringKey)}
        />
      </div>
    );
  }

  const hour = new Date().getHours();
  const greeting = t(
    (hour < 12 ? "goodMorning" : hour < 18 ? "goodAfternoon" : "goodEvening") as CommandStringKey,
  );

  return (
    <div className="oa-home">
      {/* ── Header ── */}
      {/* Greeting first, store name as the page's identity, then the one-line
          state of the shop. The summary sentence stays — it is the honest
          answer to "how is today going" and is built from real signals. */}
      <section className="oa-home-intro" aria-labelledby="today-title">
        <span className="oa-eyebrow">{greeting}</span>
        <h1 id="today-title" className="oa-home-greeting">
          {storeName || t("today" as CommandStringKey)}
        </h1>
        <p className="oa-home-summary"><BidiText text={model.summary} /></p>
        {snapshot.refreshedAt && (
          <p className="oa-home-updated">
            {t("updated" as CommandStringKey)}{" "}
            {new Date(snapshot.refreshedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
          </p>
        )}
      </section>

      {/* ── A. Store condition — six capabilities, each reporting on its own.
             This is what stops a cloud outage reading as "the shop is down". ── */}
      <section className="oa-home-condition" aria-labelledby="condition-title">
        <div className="oa-section-heading">
          <h3 id="condition-title">{t("storeCondition" as CommandStringKey)}</h3>
          <small>{t("storeConditionHint" as CommandStringKey)}</small>
        </div>
        <CapabilityStrip
          capabilities={capabilities}
          labelFor={key => t(key as CommandStringKey)}
          actionLabelFor={key => t(key as CommandStringKey)}
          onAction={cap => cap.action && onOpenTab(cap.action.tab)}
        />
      </section>

      {/* ── Partial-data warning ── */}
      {snapshot.errors.length > 0 && (
        <div className="oa-home-partial" role="status">
          <CircleAlert size={16} />
          <span><strong>{t("signalsUnavailable" as CommandStringKey)}</strong> {t("restCurrent" as CommandStringKey)}</span>
          <button onClick={onRefresh}><RefreshCw size={14} /> {t("retry" as CommandStringKey)}</button>
        </div>
      )}

      <div className="oa-home-split">
      {/* ── Signal grid ── */}
      <section className="oa-signal-grid" aria-label={t("storeSignals" as CommandStringKey)}>
        {model.signals.map((signal) => (
          <div className="oa-signal" key={signal.id}>
            <span>{signal.label}</span>
            <strong className="numeric-ltr">{signal.value}</strong>
            <small><BidiText text={signal.detail} /></small>
          </div>
        ))}
      </section>

      {/* ── Attention feed ── */}
      {attention.length > 0 && (
        <section className="oa-attention" aria-labelledby="attention-title">
          <div className="oa-section-heading">
            <h3 id="attention-title">{t("needsAttention" as CommandStringKey)}</h3>
            <small>{t("attentionHint" as CommandStringKey)}</small>
          </div>
          <div className="oa-attention-list">
            {attention.map((item) => (
              <div key={item.id} className={`oa-attention-row oa-attention-${item.severity}`}>
                <div className="oa-attention-body">
                  <strong>{item.title}</strong>
                  <span>{item.detail}</span>
                </div>
                <button onClick={() => onOpenTab(item.destination)}>
                  {item.actionLabel} <ArrowRight size={14} />
                </button>
              </div>
            ))}
          </div>
        </section>
      )}
      </div>

      {/* ── Contextual AI ── */}
      {onSendPrompt && (
        <ContextualAskBar
          context="Today dashboard — store overview"
          suggestions={askSuggestions}
          onSendPrompt={onSendPrompt}
        />
      )}
    </div>
  );
}
