import type { RangeSummary, TopProduct } from "../types";
import { formatMoney } from "../money";
import type { BackOfficeStringKey } from "../i18n/backOfficeStrings";

type Translate = (key: BackOfficeStringKey) => string;
type Download = (filename: string, rows: (string | number)[][]) => void;

export interface ReportExportContext {
  t: Translate;
  downloadCSV: Download;
  from: string;
  to: string;
  currencyExponent: number;
}

/**
 * CSV builders for the report tabs.
 *
 * Each one mirrors the table above it column for column, which is the whole
 * point: an export that quietly differs from what is on screen is worse than no
 * export. Split out of ReportsTab for size — they are pure given a range and
 * the rows they format.
 */
export function reportCsvExports(
  ctx: ReportExportContext,
  data: {
    topProducts: TopProduct[];
    taxRows: { day: string; transaction_count: number; tax_minor: number; cumulative_minor: number }[];
    taxTxCount: number;
    taxTotal: number;
    summary: RangeSummary | null;
  },
) {
  const { t, downloadCSV, from, to, currencyExponent: EXP } = ctx;
  const { topProducts, taxRows, taxTxCount, taxTotal, summary } = data;

  const handleExportProductsCSV = () => {
    const header = ["#", t("product"), t("quantitySold"), t("transactions"), t("revenue")];
    const rows = topProducts.map((p, i) => [
      i + 1,
      p.product_name,
      parseFloat(p.total_quantity),
      p.transaction_count,
      formatMoney(p.revenue_minor, EXP),
    ]);
    downloadCSV(`zanpos-top-products-${from}-${to}.csv`, [header, ...rows]);
  };

  const handleExportTaxCSV = () => {
    const header = [t("date"), t("transactions"), t("vatCollected"), t("cumulativeVat")];
    const rows = taxRows.map(r => [
      r.day,
      r.transaction_count,
      formatMoney(r.tax_minor, EXP),
      formatMoney(r.cumulative_minor, EXP),
    ]);
    const totalRow = [t("total"), taxTxCount, formatMoney(taxTotal, EXP), ""];
    downloadCSV(`zanpos-tax-${from}-${to}.csv`, [header, ...rows, totalRow]);
  };

  const handleExportSummaryCSV = () => {
    if (!summary) return;
    const rows: (string | number)[][] = [
      [t("metric"), t("value")],
      [t("period"), `${from} ${t("to")} ${to}`],
      [t("transactions"), summary.transaction_count],
      [t("grossSales"), formatMoney(summary.gross_total_minor, EXP)],
      [t("discounts"), formatMoney(summary.discount_total_minor, EXP)],
      [t("taxReport"), formatMoney(summary.tax_total_minor, EXP)],
      [t("netRevenue"), formatMoney(summary.net_total_minor, EXP)],
      [t("cash"), formatMoney(summary.cash_total_minor, EXP)],
      [t("card"), formatMoney(summary.card_total_minor, EXP)],
      [t("refunds"), `${summary.refund_count} (${formatMoney(summary.refund_total_minor, EXP)})`],
    ];
    downloadCSV(`zanpos-summary-${from}-${to}.csv`, rows);
  };

  return { handleExportProductsCSV, handleExportTaxCSV, handleExportSummaryCSV };
}
