import { useState } from "react";
import type { EodCashupReport } from "../types";
import { DEVICE } from "../types";
import { formatMoney } from "../money";
import { reportEodCashup } from "../tauri/commands";

const EXP = DEVICE.currency_exponent;
const CUR = DEVICE.currency;
function fmt(n: number) { return `${CUR} ${formatMoney(n, EXP)}`; }
function fmtOpt(n: number | null | undefined) { return n != null ? fmt(n) : "—"; }

function isoDate(d: Date) { return d.toLocaleDateString("en-CA", { timeZone: "Asia/Bahrain" }); }

export default function EodCashupTab({ sessionUserId }: { sessionUserId: string }) {
  const [date, setDate] = useState(isoDate(new Date()));
  const [report, setReport] = useState<EodCashupReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await reportEodCashup(sessionUserId, DEVICE.branch_id, date);
      setReport(data);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to load EOD report");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="tab-content eod-cashup-tab">
      <h3 className="tab-title">End-of-Day Cash-Up</h3>

      <div className="report-filters">
        <label className="field-label">Business Date</label>
        <input className="field-input date-input" type="date" value={date} onChange={e => setDate(e.target.value)} />
        <button className="btn-primary" onClick={load} disabled={loading}>
          {loading ? "Loading…" : "Run Report"}
        </button>
      </div>

      {error && <div className="modal-error">{error}</div>}

      {report && report.shifts.length === 0 && (
        <div className="report-empty">No shifts found for {report.date}.</div>
      )}

      {report && report.shifts.length > 0 && (
        <>
          {/* Summary banner */}
          <div className="eod-summary-banner">
            <div className="eod-summary-item">
              <div className="eod-summary-label">Total Net Sales</div>
              <div className="eod-summary-value">{fmt(report.total_net_minor)}</div>
            </div>
            <div className="eod-summary-item">
              <div className="eod-summary-label">Total Cash</div>
              <div className="eod-summary-value">{fmt(report.total_cash_minor)}</div>
            </div>
            <div className="eod-summary-item">
              <div className="eod-summary-label">Total Counted</div>
              <div className="eod-summary-value">{fmtOpt(report.total_counted_minor)}</div>
            </div>
            <div className={`eod-summary-item ${
              report.total_variance_minor == null ? "" :
              report.total_variance_minor < 0 ? "eod-summary-under" :
              report.total_variance_minor > 0 ? "eod-summary-over" : "eod-summary-exact"
            }`}>
              <div className="eod-summary-label">Variance</div>
              <div className="eod-summary-value">
                {report.total_variance_minor == null ? "—" :
                  `${report.total_variance_minor >= 0 ? "+" : ""}${fmt(report.total_variance_minor)}`}
              </div>
            </div>
          </div>

          {/* Per-shift breakdown */}
          <div className="eod-shifts">
            {report.shifts.map(s => (
              <div key={s.shift_id} className="eod-shift-card">
                <div className="eod-shift-header">
                  <span className="eod-shift-cashier">{s.cashier_name}</span>
                  <span className={`eod-shift-status ${s.closed_at ? "eod-closed" : "eod-open"}`}>
                    {s.closed_at ? "Closed" : "Open"}
                  </span>
                  <span className="eod-shift-time">
                    {new Date(s.opened_at).toLocaleTimeString("en-BH", { hour: "2-digit", minute: "2-digit", timeZone: "Asia/Bahrain" })}
                    {s.closed_at
                      ? ` → ${new Date(s.closed_at).toLocaleTimeString("en-BH", { hour: "2-digit", minute: "2-digit", timeZone: "Asia/Bahrain" })}`
                      : " → now"}
                  </span>
                </div>
                <div className="eod-shift-grid">
                  <div className="eod-shift-row">
                    <span>Opening Float</span>
                    <span>+ {fmt(s.opening_minor)}</span>
                  </div>
                  <div className="eod-shift-row">
                    <span>Cash Sales</span>
                    <span>+ {fmt(s.cash_sales_minor)}</span>
                  </div>
                  {s.paid_in_minor > 0 && (
                    <div className="eod-shift-row">
                      <span>Paid In</span>
                      <span>+ {fmt(s.paid_in_minor)}</span>
                    </div>
                  )}
                  {s.paid_out_minor > 0 && (
                    <div className="eod-shift-row eod-row-deduct">
                      <span>Paid Out</span>
                      <span>- {fmt(s.paid_out_minor)}</span>
                    </div>
                  )}
                  {s.safe_drop_minor > 0 && (
                    <div className="eod-shift-row eod-row-deduct">
                      <span>Safe Drop</span>
                      <span>- {fmt(s.safe_drop_minor)}</span>
                    </div>
                  )}
                  <div className="eod-shift-row eod-row-expected">
                    <span>Expected in Drawer</span>
                    <span>{fmt(s.expected_minor)}</span>
                  </div>
                  {s.counted_minor != null && (
                    <>
                      <div className="eod-shift-row">
                        <span>Counted</span>
                        <span>{fmt(s.counted_minor)}</span>
                      </div>
                      <div className={`eod-shift-row eod-row-variance ${
                        s.variance_minor == null ? "" :
                        s.variance_minor < 0 ? "eod-variance-under" :
                        s.variance_minor > 0 ? "eod-variance-over" : "eod-variance-exact"
                      }`}>
                        <span>Variance</span>
                        <span>
                          {s.variance_minor == null ? "—" :
                            `${s.variance_minor >= 0 ? "+" : ""}${fmt(s.variance_minor)}`}
                        </span>
                      </div>
                    </>
                  )}
                  <div className="eod-shift-row eod-row-net">
                    <span>Net Sales</span>
                    <span>{fmt(s.net_sales_minor)}</span>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
