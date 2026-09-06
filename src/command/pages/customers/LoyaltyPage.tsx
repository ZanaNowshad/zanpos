import { useCallback, useEffect, useMemo, useState } from "react";
import { Award, ExternalLink } from "lucide-react";
import type { CustomerRow, SessionToken } from "../../../types";

interface LoyaltySummary {
  outstanding_points: number;
  holders: number;
  total_customers: number;
  contactable_holders: number;
}
import {
  customerLoyaltySummary, customerTopBalances, operationalSettingsLoad,
} from "../../../tauri/commands";
import { useLanguage } from "../../../hooks/useLanguage";
import { operationsTranslator } from "../../../i18n/operationsStrings";
import { DataTable, EmptyState, LoadingSkeleton, PageTemplate } from "../../../components/templates";
import type { Column } from "../../../components/templates";
import "./customers.css";

interface Props {
  sessionToken: SessionToken;
  /** Opens the directory so a balance can be acted on where the customer is. */
  onOpenDirectory: () => void;
  /** Where the earn rule is actually edited. */
  onOpenSettings?: () => void;
}

/**
 * Loyalty, at programme level.
 *
 * The directory answers "what does this customer have?". This page answers
 * "what does the store owe, and to whom?" — every figure a sum or count over
 * rows already on file.
 *
 * What the previous Loyalty view showed and this one does not:
 *   • Today's sales total — a Today-domain figure, present only to fill a
 *     four-card grid.
 *   • "Find customers with points but no recent purchase" and similar prompts.
 *     No command joins a customer to a sale, so nothing could answer them.
 */
export default function LoyaltyPage({ sessionToken, onOpenDirectory, onOpenSettings }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => operationsTranslator(language), [language]);

  const [summary, setSummary] = useState<LoyaltySummary | null>(null);
  const [ranked, setRanked] = useState<CustomerRow[]>([]);
  const [pointsPerBhd, setPointsPerBhd] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      // The earn rule is a store setting and may legitimately be unreadable
      // here; a missing rule must not blank out the balances, which are the
      // point of the page.
      // Totals are a branch-scoped SQL aggregate, and the ranking is ordered in
      // SQL. Neither is derived from the directory's page, which would describe
      // only the rows currently on screen.
      const [nextSummary, nextRanked, settings] = await Promise.all([
        customerLoyaltySummary(sessionToken),
        customerTopBalances(sessionToken, 25),
        operationalSettingsLoad().catch(() => null),
      ]);
      setSummary(nextSummary);
      setRanked(nextRanked);
      setPointsPerBhd(settings?.loyalty_points_per_bhd ?? null);
    } catch (e) {
      setError(typeof e === "string" ? e : t("customersLoadFailed"));
      setSummary(null);
      setRanked([]);
    } finally {
      setLoading(false);
    }
  }, [sessionToken, t]);

  useEffect(() => { void load(); }, [load]);

  const columns: Column<CustomerRow>[] = [
    {
      id: "name",
      header: t("fullName"),
      cell: c => (
        <>
          <span className="zp-cell-primary">{c.name}</span>
          {/* A phone number is a Latin identifier. Without an explicit
              direction the bidi algorithm moves the leading "+" to the end
              in Arabic, rendering "+973 3600 1122" as "1122 3600 973+". */}
          {c.phone
            ? <bdi className="zp-cell-sub" dir="ltr">{c.phone}</bdi>
            : <span className="zp-cell-sub">{t("noPhone")}</span>}
        </>
      ),
    },
    {
      id: "points",
      header: t("loyaltyPoints"),
      align: "end",
      numeric: true,
      width: "140px",
      cell: c => (
        <span className="zp-status zp-cust-points">
          <Award size={13} aria-hidden="true" />{c.loyalty_points}
        </span>
      ),
    },
  ];

  const header = {
    title: t("loyalty"),
    subtitle: t("loyaltyNoHistory"),
    secondaryActions: onOpenSettings
      ? [{ label: t("earnRuleSettings"), onClick: onOpenSettings }]
      : undefined,
    primaryAction: { label: t("openCustomers"), onClick: onOpenDirectory },
  };

  if (loading) {
    return <PageTemplate header={header}><LoadingSkeleton variant="table" /></PageTemplate>;
  }

  if (error) {
    return (
      <PageTemplate header={header}>
        <EmptyState
          variant="degraded"
          title={t("customersLoadFailed")}
          description={error}
          stillWorks={t("sellingUnaffectedShort")}
          actions={[{ label: t("retry"), onClick: () => void load(), primary: true }]}
        />
      </PageTemplate>
    );
  }

  return (
    <PageTemplate header={header}>
      <dl className="zp-cust-facts zp-loyalty-facts">
        <div>
          <dt>{t("outstandingPoints")}</dt>
          <dd className="zp-cust-balance">{summary?.outstanding_points ?? 0}</dd>
        </div>
        <div>
          <dt>{t("customersWithBalance")}</dt>
          <dd>{summary?.holders ?? 0} / {summary?.total_customers ?? 0}</dd>
        </div>
        <div>
          <dt>{t("reachableByPhone")}</dt>
          <dd>{summary?.contactable_holders ?? 0}</dd>
        </div>
        <div>
          <dt>{t("earnRule")}</dt>
          <dd>{pointsPerBhd == null ? "—" : `${pointsPerBhd} pt / BHD`}</dd>
        </div>
      </dl>

      <h2 className="zp-cust-section-title">{t("topLoyaltyBalances")}</h2>

      {ranked.length === 0 ? (
        <EmptyState
          variant="first-use"
          icon={<Award size={32} strokeWidth={1.5} />}
          title={t("noLoyaltyPoints")}
          description={t("noLoyaltyPointsHint")}
          actions={[{ label: t("openCustomers"), onClick: onOpenDirectory, primary: true }]}
        />
      ) : (
        <>
          <DataTable
            caption={t("topLoyaltyBalances")}
            rows={ranked}
            rowKey={c => c.customer_id}
            columns={columns}
          />
          <p className="zp-cust-note">
            {t("loyaltyAdjustedInDirectory")}{" "}
            <button type="button" className="zp-cust-link" onClick={onOpenDirectory}>
              {t("openCustomers")} <ExternalLink size={12} aria-hidden="true" />
            </button>
          </p>
        </>
      )}
    </PageTemplate>
  );
}
