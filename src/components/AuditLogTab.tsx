import { useCallback, useState } from "react";
import { auditLogList } from "../tauri/commands";

interface AuditRow {
  audit_log_id: string;
  event_type: string;
  entity_type: string;
  entity_id: string | null;
  actor_user_id: string | null;
  created_at: string;
}

function isoDate(d: Date) { return d.toISOString().slice(0, 10); }

function defaultDates() {
  const now = new Date();
  const today = isoDate(now);
  const weekAgo = isoDate(new Date(now.getTime() - 7 * 24 * 60 * 60 * 1000));
  return { today, weekAgo };
}

export default function AuditLogTab() {
  const { today, weekAgo } = defaultDates();

  const [from, setFrom]     = useState(weekAgo);
  const [to, setTo]         = useState(today);
  const [page, setPage]     = useState(0);
  const [rows, setRows]     = useState<AuditRow[]>([]);
  const [loading, setLoading] = useState(false);
  const [loaded, setLoaded] = useState(false);

  const load = useCallback(async (p: number = 0) => {
    if (!from || !to) return;
    setLoading(true);
    try {
      const result = await auditLogList(from, to, p);
      setRows(result as AuditRow[]);
      setPage(p);
      setLoaded(true);
    } catch (e: unknown) {
      alert(typeof e === "string" ? e : "Failed to load audit log");
    } finally {
      setLoading(false);
    }
  }, [from, to]);

  return (
    <div className="audit-layout">
      <div className="audit-toolbar">
        <span className="audit-title">Audit Log</span>
        <input
          type="date"
          className="rpt-date-input"
          value={from}
          onChange={e => { setFrom(e.target.value); setLoaded(false); }}
        />
        <span className="rpt-date-sep">→</span>
        <input
          type="date"
          className="rpt-date-input"
          value={to}
          onChange={e => { setTo(e.target.value); setLoaded(false); }}
        />
        <button className="btn-primary" onClick={() => load(0)} disabled={loading}>
          {loading ? "…" : "Load"}
        </button>
      </div>

      {!loaded ? (
        <div className="bo-empty">Set a date range and click Load.</div>
      ) : rows.length === 0 ? (
        <div className="bo-empty">No audit events in this period.</div>
      ) : (
        <>
          <div className="rpt-table-wrap audit-table-wrap">
            <table className="rpt-table">
              <thead>
                <tr>
                  <th>Time</th>
                  <th>Event</th>
                  <th>Entity</th>
                  <th>Entity ID</th>
                  <th>Actor</th>
                </tr>
              </thead>
              <tbody>
                {rows.map(r => (
                  <tr key={r.audit_log_id}>
                    <td className="rpt-date">{new Date(r.created_at).toLocaleString([], {
                      month: "short", day: "numeric",
                      hour: "2-digit", minute: "2-digit", second: "2-digit"
                    })}</td>
                    <td><span className="audit-event-badge">{r.event_type}</span></td>
                    <td className="rpt-method">{r.entity_type}</td>
                    <td className="rpt-receipt">{r.entity_id ?? "—"}</td>
                    <td className="rpt-dim">{r.actor_user_id ?? "system"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          {/* Pagination */}
          <div className="audit-pagination">
            <button
              className="btn-secondary"
              onClick={() => load(page - 1)}
              disabled={page === 0 || loading}
            >
              ← Prev
            </button>
            <span className="audit-page-info">Page {page + 1}</span>
            <button
              className="btn-secondary"
              onClick={() => load(page + 1)}
              disabled={rows.length < 50 || loading}
            >
              Next →
            </button>
          </div>
        </>
      )}
    </div>
  );
}
