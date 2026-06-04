import { useState } from "react";
import type { CashierSummaryRow } from "../types";
import { DEVICE } from "../types";
import { formatMoney } from "../money";
import { reportByCashier } from "../tauri/commands";

const EXP = DEVICE.currency_exponent;
const CUR = DEVICE.currency;
function fmt(n: number) { return `${CUR} ${formatMoney(n, EXP)}`; }

function isoDate(d: Date) { return d.toLocaleDateString("en-CA", { timeZone: "Asia/Bahrain" }); }

export default function CashierReportTab({ sessionUserId: _sid }: { sessionUserId: string }) {
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
      const data = await reportByCashier(DEVICE.branch_id, from, to);
      setRows(data);
      setLoaded(true);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to load cashier report");
    } finally {
      setLoading(false);
    }
  };

  const totalNet = rows.reduce((s, r) => s + r.net_total_minor, 0);
  const totalTx  = rows.reduce((s, r) => s + r.transaction_count, 0);

  return (
    <div className="tab-content cashier-report-tab">
      <h3 className="tab-title">Sales by Cashier</h3>

      <div className="report-filters">
        <label className="field-label">From</label>
        <input className="field-input date-input" type="date" value={from} onChange={e => setFrom(e.target.value)} />
        <label className="field-label">To</label>
        <input className="field-input date-input" type="date" value={to} onChange={e => setTo(e.target.value)} />
        <button className="btn-primary" onClick={load} disabled={loading}>
          {loading ? "Loading…" : "Run Report"}
        </button>
      </div>

      {error && <div className="modal-error">{error}</div>}

      {loaded && rows.length === 0 && (
        <div className="report-empty">No sales found for this period.</div>
      )}

      {rows.length > 0 && (
        <>
          <div className="cashier-totals-bar">
            <span>{totalTx} transactions</span>
            <span>Total net: {fmt(totalNet)}</span>
          </div>
          <div className="cashier-table-wrapper">
            <table className="report-table">
              <thead>
                <tr>
                  <th>Cashier</th>
                  <th>Tx</th>
                  <th>Net Sales</th>
                  <th>Cash</th>
                  <th>Card</th>
                  <th>Discounts</th>
                  <th>Refunds</th>
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
                  <td>Total</td>
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
      )}
    </div>
  );
}
