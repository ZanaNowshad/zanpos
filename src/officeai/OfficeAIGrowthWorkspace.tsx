import { BarChart3, Medal, RefreshCw, Settings, Sparkles, TrendingUp, Users } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { formatMoney } from "../money";
import { appConfigLoad, customerList, operationalSettingsLoad, reportDateRange, reportTopProducts } from "../tauri/commands";
import type { CustomerRow, RangeSummary, TopProduct } from "../types";
import type { OfficeAiOverviewSnapshot, OfficeTab } from "./officeAiTypes";
import { useLanguage } from "../hooks/useLanguage";
import {
  officeAiFormat,
  officeAiTranslator,
  type OfficeAiStringKey,
} from "../i18n/officeAiStrings";

type GrowthMode = "insights" | "loyalty";

const RETENTION_PROMPTS = [
  { labelKey: "retentionPromptOne", prompt: "Which loyalty customers are worth contacting today?" },
  { labelKey: "retentionPromptTwo", prompt: "Suggest a simple loyalty reward that will not hurt margin." },
  { labelKey: "retentionPromptThree", prompt: "Find customers with points but no recent purchase." },
  { labelKey: "retentionPromptFour", prompt: "Draft a WhatsApp message for top loyalty customers." },
] satisfies Array<{ labelKey: OfficeAiStringKey; prompt: string }>;

const OPERATING_PROMPTS = [
  { labelKey: "operatingPromptOne", prompt: "Which products drove the last 30 days of sales?" },
  { labelKey: "operatingPromptTwo", prompt: "What should I reorder based on sales and low stock?" },
  { labelKey: "operatingPromptThree", prompt: "Find refund or discount patterns I should inspect." },
  { labelKey: "operatingPromptFour", prompt: "Summarize today's sales against the last 30 days." },
] satisfies Array<{ labelKey: OfficeAiStringKey; prompt: string }>;

interface Props {
  mode: GrowthMode;
  actorUserId: string;
  snapshot: OfficeAiOverviewSnapshot;
  currencyExp: number;
  onOpenTab: (tab: OfficeTab) => void;
  onSendPrompt: (prompt: string) => void;
}

function isoDate(d: Date): string {
  return d.toLocaleDateString("en-CA", { timeZone: "Asia/Bahrain" });
}

function defaultRange() {
  const to = new Date();
  const from = new Date();
  from.setDate(from.getDate() - 29);
  return { from: isoDate(from), to: isoDate(to) };
}

function money(minor: number, exp: number): string {
  return `BHD ${formatMoney(minor, exp)}`;
}

export default function OfficeAIGrowthWorkspace({
  mode,
  actorUserId,
  snapshot,
  currencyExp,
  onOpenTab,
  onSendPrompt,
}: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => officeAiTranslator(language), [language]);
  const [summary, setSummary] = useState<RangeSummary | null>(null);
  const [topProducts, setTopProducts] = useState<TopProduct[]>([]);
  const [customers, setCustomers] = useState<CustomerRow[]>([]);
  const [pointsPerBhd, setPointsPerBhd] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const range = useMemo(() => defaultRange(), []);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const appConfig = await appConfigLoad();
      const branchId = appConfig.branch_id;
      const [nextSummary, nextTopProducts, nextCustomers, settings] = await Promise.all([
        reportDateRange(actorUserId, branchId, range.from, range.to),
        reportTopProducts(actorUserId, branchId, range.from, range.to),
        customerList(actorUserId, ""),
        operationalSettingsLoad().catch(() => null),
      ]);
      setSummary(nextSummary);
      setTopProducts(nextTopProducts);
      setCustomers(nextCustomers);
      setPointsPerBhd(settings?.loyalty_points_per_bhd ?? null);
    } catch (e) {
      setError(typeof e === "string" ? e : t("growthLoadFailed"));
    } finally {
      setLoading(false);
    }
  }, [actorUserId, range.from, range.to, t]);

  useEffect(() => { void load(); }, [load]);

  const loyaltyCustomers = customers
    .filter(c => c.loyalty_points > 0)
    .sort((a, b) => b.loyalty_points - a.loyalty_points);
  const totalPoints = loyaltyCustomers.reduce((sum, c) => sum + c.loyalty_points, 0);
  const avgTicket = summary && summary.transaction_count > 0
    ? Math.round(summary.net_total_minor / summary.transaction_count)
    : 0;

  if (mode === "loyalty") {
    return (
      <div className="oa-growth">
        <section className="oa-command-strip">
          <div className="oa-health-summary">
            <span className="oa-health-orb ok" />
            <div>
              <strong>{t("loyaltyCommandView")}</strong>
              <span>{t("loyaltyIntro")}</span>
            </div>
          </div>
          <button className="oa-command-tile oa-command-primary" onClick={() => onOpenTab("customers")}>
            <Users size={20} />
            <span><strong>{t("openCustomers")}</strong><small>{t("openCustomersHint")}</small></span>
          </button>
          <button className="oa-command-tile" onClick={() => onOpenTab("settings")}>
            <Settings size={20} />
            <span><strong>{t("loyaltySettings")}</strong><small>{t("loyaltySettingsHint")}</small></span>
          </button>
          <button className="oa-command-tile" onClick={() => onSendPrompt("Find loyalty customers who should receive a retention offer this week.")}>
            <Sparkles size={20} />
            <span><strong>{t("askAiRetention")}</strong><small>{t("askAiRetentionHint")}</small></span>
          </button>
        </section>

        {error && <div className="oa-inline-warning">{error}<button onClick={load}>{t("retry")}</button></div>}

        <section className="oa-metric-grid">
          <div className="oa-metric-card">
            <div className="oa-card-label"><Medal size={15} /> {t("outstandingPoints")}</div>
            <div className="oa-big-number">{totalPoints}</div>
            <div className="oa-card-sub">{loyaltyCustomers.length} customers with a balance</div>
          </div>
          <div className="oa-metric-card">
            <div className="oa-card-label"><Users size={15} /> {t("customerBase")}</div>
            <div className="oa-big-number">{customers.length}</div>
            <div className="oa-card-sub">{customers.filter(c => c.phone).length} {t("reachableByPhone")}</div>
          </div>
          <div className="oa-metric-card">
            <div className="oa-card-label"><TrendingUp size={15} /> {t("today")}</div>
            <div className="oa-big-number">{snapshot.today ? money(snapshot.today.net_total_minor, currencyExp) : t("noData")}</div>
            <div className="oa-card-sub">{officeAiFormat(t("transactionsToday"), { count: snapshot.today?.transaction_count ?? 0 })}</div>
          </div>
          <div className="oa-metric-card">
            <div className="oa-card-label"><Settings size={15} /> {t("earnRule")}</div>
            <div className="oa-status-line">{pointsPerBhd == null ? t("notLoaded") : `${pointsPerBhd} pt/BHD`}</div>
            <div className="oa-card-sub">{t("simpleRewardsHint")}</div>
          </div>
        </section>

        <section className="oa-two-column">
          <div className="oa-panel">
            <div role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}  className="oa-panel-header"><h2>{t("topLoyaltyBalances")}</h2><button onClick={load} disabled={loading}>{t(loading ? "loading" : "refresh")}</button></div>
            {loyaltyCustomers.length ? (
              <div className="oa-list">
                {loyaltyCustomers.slice(0, 8).map(c => (
                  <div key={c.customer_id} className="oa-list-row">
                    <Medal size={16} />
                    <span><strong>{c.name}</strong><small>{c.phone ?? t("noPhone")} - {c.loyalty_points} {t("points")}</small></span>
                  </div>
                ))}
              </div>
            ) : (
              <div className="oa-empty-state">{t("noLoyaltyPoints")}</div>
            )}
          </div>
          <div className="oa-panel">
            <div className="oa-panel-header"><h2>{t("retentionPrompts")}</h2></div>
            <div className="oa-shortcut-grid">
              {RETENTION_PROMPTS.map(item => (
                <button key={item.prompt} onClick={() => onSendPrompt(item.prompt)}>{t(item.labelKey)}</button>
              ))}
            </div>
          </div>
        </section>
      </div>
    );
  }

  return (
    <div className="oa-growth">
      <section className="oa-command-strip">
        <div className="oa-health-summary">
          <span className="oa-health-orb ok" />
          <div>
            <strong>{t("insightsCommandView")}</strong>
            <span>{t("insightsIntro")}</span>
          </div>
        </div>
        <button className="oa-command-tile oa-command-primary" onClick={() => onOpenTab("reports")}>
          <BarChart3 size={20} />
          <span><strong>{t("openReports")}</strong><small>{t("openReportsHint")}</small></span>
        </button>
        <button className="oa-command-tile" onClick={() => onSendPrompt("Analyze the last 30 days of sales and tell me the highest-impact actions.")}>
          <Sparkles size={20} />
          <span><strong>{t("askAiActions")}</strong><small>{t("askAiActionsHint")}</small></span>
        </button>
      </section>

      {error && <div className="oa-inline-warning">{error}<button onClick={load}>{t("retry")}</button></div>}

      <section className="oa-metric-grid">
        <div className="oa-metric-card oa-metric-wide">
          <div className="oa-card-label"><TrendingUp size={15} /> {t("lastThirtyDays")}</div>
          <div className="oa-big-number">{summary ? money(summary.net_total_minor, currencyExp) : t("loading")}</div>
          <div className="oa-metric-row">
            <span>{summary?.transaction_count ?? 0} transactions</span>
            <span>{summary?.refund_count ?? 0} refunds</span>
            <span>Avg {money(avgTicket, currencyExp)}</span>
          </div>
        </div>
        <div className="oa-metric-card">
          <div className="oa-card-label"><BarChart3 size={15} /> {t("topProducts")}</div>
          <div className="oa-big-number">{topProducts.length}</div>
          <div className="oa-card-sub">{t("rankedByRevenue")}</div>
        </div>
        <div className="oa-metric-card">
          <div className="oa-card-label"><RefreshCw size={15} /> {t("syncImpact")}</div>
          <div className="oa-big-number">{snapshot.sync?.pending_events ?? 0}</div>
          <div className="oa-card-sub">{t("pendingEventsHint")}</div>
        </div>
      </section>

      <section className="oa-two-column">
        <div className="oa-panel">
          <div role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}  className="oa-panel-header"><h2>{t("productSignals")}</h2><button onClick={load} disabled={loading}>{t(loading ? "loading" : "refresh")}</button></div>
          {topProducts.length ? (
            <div className="oa-list">
              {topProducts.slice(0, 8).map(p => (
                <div key={p.product_name} className="oa-list-row">
                  <BarChart3 size={16} />
                  <span><strong>{p.product_name}</strong><small>{p.total_quantity} sold - {money(p.revenue_minor, currencyExp)} revenue</small></span>
                </div>
              ))}
            </div>
          ) : (
            <div className="oa-empty-state">{t("noProductSignals")}</div>
          )}
        </div>
        <div className="oa-panel">
          <div className="oa-panel-header"><h2>{t("operatingPrompts")}</h2></div>
          <div className="oa-shortcut-grid">
            {OPERATING_PROMPTS.map(item => (
              <button key={item.prompt} onClick={() => onSendPrompt(item.prompt)}>{t(item.labelKey)}</button>
            ))}
          </div>
        </div>
      </section>
    </div>
  );
}
