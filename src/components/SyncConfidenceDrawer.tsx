import { useCallback, useEffect, useMemo, useState } from "react";
import { RefreshCw, Wifi, X } from "lucide-react";
import type { SyncStatus, SyncTableStats, SessionToken } from "../types";
import { syncQueueStats, syncTriggerNow } from "../tauri/commands";

interface Props {
  open: boolean;
  status: SyncStatus | null;
  sessionToken: SessionToken;
  onClose: () => void;
}

export interface SyncStatsSummary {
  pendingSales: number;
  totalPending: number;
  totalFailed: number;
  failedTables: string[];
}

export function summarizeSyncStats(stats: SyncTableStats[]): SyncStatsSummary {
  return stats.reduce<SyncStatsSummary>((summary, row) => {
    const pending = Number(row.pending) || 0;
    const failed = Number(row.failed) || 0;
    return {
      pendingSales: row.table === "sales" ? pending : summary.pendingSales,
      totalPending: summary.totalPending + pending,
      totalFailed: summary.totalFailed + failed,
      failedTables: failed > 0 ? [...summary.failedTables, row.table] : summary.failedTables,
    };
  }, { pendingSales: 0, totalPending: 0, totalFailed: 0, failedTables: [] });
}

const formatTime = (value: string | null | undefined) => {
  if (!value) return "Never";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString([], { dateStyle: "medium", timeStyle: "short" });
};

export default function SyncConfidenceDrawer({ open, status, sessionToken, onClose }: Props) {
  const [stats, setStats] = useState<SyncTableStats[]>([]);
  const [loading, setLoading] = useState(false);
  const [retrying, setRetrying] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setStats(await syncQueueStats(sessionToken));
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Could not load sync queue details.");
    } finally {
      setLoading(false);
    }
  }, [sessionToken]);

  useEffect(() => {
    if (open) void load();
  }, [open, load]);

  const summary = useMemo(() => summarizeSyncStats(stats), [stats]);
  const failedTable = summary.failedTables[0] ?? status?.last_error ?? "None";

  const retry = async () => {
    if (retrying) return;
    setRetrying(true);
    setError(null);
    try {
      await syncTriggerNow(sessionToken);
      await load();
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Retry sync failed.");
    } finally {
      setRetrying(false);
    }
  };

  if (!open) return null;

  return (
    <div role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}  className="sync-confidence-backdrop" onClick={e => e.target === e.currentTarget && onClose()}>
      <aside className="sync-confidence-drawer" aria-label="Sync confidence details">
        <header className="sync-confidence-header">
          <div>
            <span><Wifi size={17} /> Sync confidence</span>
            <strong>{status?.online ? "Online" : "Offline or waiting"}</strong>
          </div>
          <button onClick={onClose} aria-label="Close sync details"><X size={18} /></button>
        </header>

        <div className="sync-confidence-grid">
          <div><span>Pending sales</span><strong>{summary.pendingSales}</strong></div>
          <div><span>Pending rows</span><strong>{summary.totalPending || status?.pending_events || 0}</strong></div>
          <div><span>Failed rows</span><strong>{summary.totalFailed}</strong></div>
          <div><span>Last synced</span><strong>{formatTime(status?.last_successful_sync_at)}</strong></div>
        </div>

        <section className="sync-confidence-section">
          <span className="sync-confidence-label">Failed table</span>
          <p>{failedTable}</p>
        </section>

        {loading ? (
          <div className="sync-confidence-empty">Loading queue details...</div>
        ) : stats.length === 0 ? (
          <div className="sync-confidence-empty">No table-level sync backlog found.</div>
        ) : (
          <div className="sync-confidence-table">
            {stats.map(row => (
              <div key={row.table} className={row.failed > 0 ? "sync-confidence-row sync-confidence-row-failed" : "sync-confidence-row"}>
                <span>{row.table}</span>
                <span>{row.pending} pending</span>
                <span>{row.failed} failed</span>
              </div>
            ))}
          </div>
        )}

        {error && <div className="sync-confidence-error" role="alert">{error}</div>}

        <footer className="sync-confidence-footer">
          <button onClick={retry} disabled={retrying}>
            <RefreshCw size={15} /> {retrying ? "Retrying..." : "Retry sync"}
          </button>
          <span>Sales stay local until the hub accepts them.</span>
        </footer>
      </aside>
    </div>
  );
}
