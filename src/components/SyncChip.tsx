import { type MouseEvent, memo, useState } from "react";
import type { SyncStatus } from "../types";
import { syncTriggerNow } from "../tauri/commands";

interface Props { status: SyncStatus | null }

const SyncChip = memo(function SyncChip({ status }: Props) {
  const [retrying, setRetrying] = useState(false);

  const handleRetry = async (e: MouseEvent) => {
    e.stopPropagation();
    if (retrying) return;
    setRetrying(true);
    // R-16: surface retry failures to the console rather than swallowing them.
    try { await syncTriggerNow(); }
    catch (err: unknown) { console.warn("Sync retry failed:", err); }
    finally { setRetrying(false); }
  };

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
    const hasStuckError = !!status.last_error;
    const pending = status.pending_events > 0;
    const consecFails = status.consecutive_failure_count ?? 0;
    const persistentError = hasStuckError && consecFails >= 3;
    const errorTitle = hasStuckError ? `Sync errors: ${status.last_error} (${consecFails} consecutive)` : undefined;
    if (hasStuckError) {
      return (
        <span className="sync-chip sync-has-error" title={errorTitle}>
          <span className="sync-chip-error-marker" aria-label="sync error">!</span>
          {' '}Online{pending ? ` — Syncing (${status.pending_events})` : " (with errors)"}
          {status.last_error && (
            <span className="sync-error-inline" style={{ fontSize: '0.75em', marginLeft: 4, opacity: 0.85 }}>
              — {status.last_error.slice(0, 60)}{status.last_error.length > 60 ? '…' : ''}
            </span>
          )}
          {persistentError && (
            <button
              className="sync-retry-btn"
              onClick={handleRetry}
              disabled={retrying}
              title={`${consecFails} consecutive sync errors — retry now`}
            >
              {retrying ? "…" : "⟳"}
            </button>
          )}
        </span>
      );
    }
    return pending
      ? <span className="sync-chip sync-syncing">⟳  Syncing ({status.pending_events})</span>
      : <span className="sync-chip sync-ok">●  Online</span>;
  }

  // Offline — surface the worker's actual last error so an operator can
  // diagnose without opening DevTools. Renders as a hover title and as a
  // small inline marker. Without this, "Offline" gives no clue whether the
  // cause is bad credentials, DNS, or a rejected event.
  const stale = (status.days_since_last_sync ?? 0) > 3;
  const staleDays = status.days_since_last_sync ?? 0;
  const err = status.last_error ?? "";
  const errorSuffix = err ? (err.length > 80 ? `${err.slice(0, 80)}…` : err) : "";
  const title = [
    `Pending: ${status.pending_events}`,
    stale ? `Offline ${staleDays}d` : "",
    err ? `Last error: ${err}` : "",
  ].filter(Boolean).join(" — ");

  return (
    <span className="sync-offline-wrap">
      <span
        className={`sync-chip ${stale ? "sync-stale" : "sync-offline"}`}
        title={title}
      >
        ○  Offline{stale ? ` — ${staleDays}d` : ""}{status.pending_events > 0 ? ` (${status.pending_events})` : ""}
        {err && <span className="sync-chip-error-marker" aria-label="sync error">!</span>}
        {err && errorSuffix && (
          <span className="sync-error-inline" style={{ fontSize: '0.75em', marginLeft: 4, opacity: 0.85 }}>
            — {errorSuffix}
          </span>
        )}
        {/* Retry button always visible when offline so operator can manually trigger sync */}
        <button
          className="sync-retry-btn"
          onClick={handleRetry}
          disabled={retrying}
          title={errorSuffix ? `Retry sync now — last error: ${err}` : "Retry sync now"}
        >
          {retrying ? "…" : "⟳"}
        </button>
      </span>
      {/* T05: reassure cashier that sales are safe while offline */}
      <span className="sync-offline-hint">Your sales are saved — keep selling</span>
    </span>
  );
});

export default SyncChip;
