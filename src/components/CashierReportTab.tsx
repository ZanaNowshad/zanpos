import { useMemo, useState } from "react";
import type { CashierSummaryRow } from "../types";
import { DEVICE } from "../types";
import { formatMoney } from "../money";
import { reportByCashier } from "../tauri/commands";
import { useLanguage } from "../hooks/useLanguage";
import { backOfficeTranslator } from "../i18n/backOfficeStrings";
import { PageTemplate, EmptyState, LoadingSkeleton } from "./templates";
import { Users } from "lucide-react";

const EXP = DEVICE.currency_exponent;
const CUR = DEVICE.currency;
function fmt(n: number) { return `${CUR} ${formatMoney(n, EXP)}`; }

function isoDate(d: Date) { return d.toLocaleDateString("en-CA", { timeZone: "Asia/Bahrain" }); }

export default function CashierReportTab({ sessionUserId }: { sessionUserId: string }) {
  const { language } = useLanguage();
  const t = useMemo(() => backOfficeTranslator(language), [language]);
  const today = isoDate(new Date());
  const [from, setFrom] = useState(today);
  const [to, setTo] = useState(today);
  const [rows, setRows] = useState<CashierSummaryRow[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await reportByCashier(sessionUserId, DEVICE.branch_id, from, to);
      setRows(data);
      setLoaded(true);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : t("failedLoadReport"));
    } finally {
      setLoading(false);
    }
  };

  const totalNet = rows.reduce((s, r) => s + r.net_total_minor, 0);
  const totalTx  = rows.reduce((s, r) => s + r.transaction_count, 0);

  return (
    <PageTemplate
      header={{
        title: t("cashierReport"),
        icon: <Users size={18} strokeWidth={1.7} aria-hidden="true" />,
      }}
      toolbar={
        <div className="report-filters">
          <label htmlFor="a11y-input-1" className="field-label">{t("from")}</label>
          <input id="a11y-input-1" className="field-input date-input" type="date" value={from} onChange={e => setFrom(e.target.value)} />
          <label htmlFor="a11y-input-2" className="field-label">{t("to")}</label>
          <input id="a11y-input-2" className="field-input date-input" type="date" value={to} onChange={e => setTo(e.target.value)} />
          <button className="btn-primary" onClick={load} disabled={loading}>
            {loading ? t("loading") : t("runReport")}
          </button>
        </div>
      }
    >
      {error && <div className="modal-error">{error}</div>}

      {loading ? (
        <LoadingSkeleton variant="table" count={4} />
      ) : loaded && rows.length === 0 ? (
        <EmptyState
          title={t("noSalesPeriod")}
          description={t("adjustDateRange")}
        />
      ) : rows.length > 0 ? (
        <>
          <div className="cashier-totals-bar">
            <span>{t("transactions")}: {totalTx}</span>
            <span>{t("totalNet")}: {fmt(totalNet)}</span>
          </div>
          <div className="cashier-table-wrapper">
            <table className="report-table">
              <thead>
                <tr>
                  <th>{t("cashier")}</th>
                  <th>{t("transactions")}</th>
                  <th>{t("netSales")}</th>
                  <th>{t("cash")}</th>
                  <th>{t("card")}</th>
                  <th>{t("discounts")}</th>
                  <th>{t("refunds")}</th>
                </tr>
              </thead>
              <tbody>
                {rows.map(r => (
                  <tr key={r.cashier_user_id}>
                    <td>{r.cashier_name}</td>
                    <td className="num-cell">{r.transaction_count}</td>
                    <td className="num-cell">{fmt(r.net_total_minor)}</td>
                    <td className="num-cell">{fmt(r.cash_total_minor)}</td>
                    <td className="num-cell">{fmt(r.card_total_minor)}</td>
                    <td className="num-cell">{fmt(r.discount_total_minor)}</td>
                    <td className="num-cell">
                      {r.refund_count > 0 ? `${r.refund_count} (- ${fmt(r.refund_total_minor)})` : "—"}
                    </td>
                  </tr>
                ))}
              </tbody>
              <tfoot>
                <tr className="report-total-row">
                  <td>{t("total")}</td>
                  <td className="num-cell">{totalTx}</td>
                  <td className="num-cell">{fmt(totalNet)}</td>
                  <td className="num-cell">{fmt(rows.reduce((s, r) => s + r.cash_total_minor, 0))}</td>
                  <td className="num-cell">{fmt(rows.reduce((s, r) => s + r.card_total_minor, 0))}</td>
                  <td className="num-cell">{fmt(rows.reduce((s, r) => s + r.discount_total_minor, 0))}</td>
                  <td className="num-cell">
                    {rows.reduce((s, r) => s + r.refund_count, 0)} (- {fmt(rows.reduce((s, r) => s + r.refund_total_minor, 0))})
                  </td>
                </tr>
              </tfoot>
            </table>
          </div>
        </>
      ) : null}
    </PageTemplate>
  );
}
