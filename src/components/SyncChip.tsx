import type { SyncStatus } from "../types";

interface Props { status: SyncStatus | null }

export default function SyncChip({ status }: Props) {
  if (!status) return <span className="sync-chip sync-unknown">●  Connecting…</span>;

  // Not configured — Supabase credentials absent
  if (!status.supabase_configured) {
    return (
      <span className="sync-chip sync-not-configured" title="Open Back Office → Sync to connect">
        ⚠  No Cloud
      </span>
    );
  }

  if (status.online) {
    return status.pending_events > 0
      ? <span className="sync-chip sync-syncing">⟳  Syncing ({status.pending_events})</span>
      : <span className="sync-chip sync-ok">●  Online</span>;
  }

  // Stale: offline > 3 days is worth escalating in the chip itself
  const stale = (status.days_since_last_sync ?? 0) > 3;
  return (
    <span
      className={`sync-chip ${stale ? "sync-stale" : "sync-offline"}`}
      title={`Pending: ${status.pending_events}${stale ? ` — offline ${status.days_since_last_sync}d` : ""}`}
    >
      ○  Offline{status.pending_events > 0 ? ` (${status.pending_events} pending)` : ""}
    </span>
  );
}
