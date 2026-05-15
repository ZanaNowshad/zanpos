import type { SyncStatus } from "../types";

interface Props { status: SyncStatus | null }

export default function SyncChip({ status }: Props) {
  if (!status) return <span className="sync-chip sync-unknown">●  Connecting…</span>;

  if (status.online) {
    return status.pending_events > 0
      ? <span className="sync-chip sync-syncing">⟳  Syncing ({status.pending_events})</span>
      : <span className="sync-chip sync-ok">●  Online</span>;
  }

  return (
    <span className="sync-chip sync-offline" title={`Pending: ${status.pending_events}`}>
      ○  Offline{status.pending_events > 0 ? ` (${status.pending_events} pending)` : ""}
    </span>
  );
}
