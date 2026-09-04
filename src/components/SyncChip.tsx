import { type KeyboardEvent, type MouseEvent, memo, useMemo, useState } from "react";
import { AlertTriangle, CheckCircle2, RefreshCw, WifiOff } from "lucide-react";
import type { SyncStatus } from "../types";
import { syncTriggerNow } from "../tauri/commands";

interface Props { status: SyncStatus | null; sessionToken?: string; onOpenDetails?: () => void }

/** Minutes since the last successful sync, or null if it has never succeeded. */
export function syncAgeMinutes(lastSuccess: string | null): number | null {
  if (!lastSuccess) return null;
  const then = Date.parse(lastSuccess);
  if (!Number.isFinite(then)) return null;
  return Math.max(0, Math.floor((Date.now() - then) / 60_000));
}

/**
 * Sync age in words. Minutes below an hour, then hours, then days — an
 * operator deciding whether to keep selling needs "12m" and "3d" to read
 * differently at a glance, and "180m" reads as neither.
 */
export function formatSyncAge(minutes: number | null, fallbackDays = 0): string {
  if (minutes === null) return fallbackDays > 0 ? `${fallbackDays}d ago` : "never";
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

const SyncChip = memo(function SyncChip({ status, sessionToken = "", onOpenDetails }: Props) {
  const [retrying, setRetrying] = useState(false);
  const openProps = useMemo(() => {
    if (!onOpenDetails) return {};
    return {
      role: "button",
      tabIndex: 0,
      onClick: onOpenDetails,
      onKeyDown: (e: KeyboardEvent<HTMLElement>) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onOpenDetails();
        }
      },
    };
  }, [onOpenDetails]);

  const handleRetry = async (e: MouseEvent) => {
    e.stopPropagation();
    if (retrying) return;
    setRetrying(true);
    // R-16: surface retry failures to the console rather than swallowing them.
    try { await syncTriggerNow(sessionToken); }
    catch (err: unknown) { console.warn("Sync retry failed:", err); }
    finally { setRetrying(false); }
  };

  if (!status) return <span className="sync-chip sync-unknown" {...openProps}><RefreshCw size={14} /> Connecting...</span>;

  // Not configured — no hub connection
  if (!status.hub_configured) {
    return (
      <span className="sync-chip sync-not-configured" title="Open Back Office → Settings → Hub to connect" {...openProps}>
        <AlertTriangle size={14} /> No Hub
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
        <span className="sync-chip sync-has-error" title={errorTitle} {...openProps}>
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
              {retrying ? "..." : <RefreshCw size={12} />}
            </button>
          )}
        </span>
      );
    }
    return pending
      ? <span className="sync-chip sync-syncing" {...openProps}><RefreshCw size={14} /> Syncing ({status.pending_events})</span>
      : <span className="sync-chip sync-ok" {...openProps}><CheckCircle2 size={14} /> Online</span>;
  }

  // Offline — surface the worker's actual last error so an operator can
  // diagnose without opening DevTools. Renders as a hover title and as a
  // small inline marker. Without this, "Offline" gives no clue whether the
  // cause is bad credentials, DNS, or a rejected event.
  // Sync AGE, not just "offline" (spec item 55). Days-since was the only
  // signal here and it only tripped after 3 days — but two tills diverge in
  // minutes, not days, and by the third day the damage is a stock count
  // nobody trusts. Minutes are what an operator can still act on.
  const ageMinutes = syncAgeMinutes(status.last_successful_sync_at);
  const stale = ageMinutes === null || ageMinutes >= 60;
  const staleDays = status.days_since_last_sync ?? 0;
  const ageLabel = formatSyncAge(ageMinutes, staleDays);
  const err = status.last_error ?? "";
  const errorSuffix = err ? (err.length > 80 ? `${err.slice(0, 80)}…` : err) : "";
  const title = [
    `Pending: ${status.pending_events}`,
    `Last sync: ${ageLabel}`,
    err ? `Last error: ${err}` : "",
  ].filter(Boolean).join(" — ");

  return (
    <span className="sync-offline-wrap" {...openProps}>
      <span
        className={`sync-chip ${stale ? "sync-stale" : "sync-offline"}`}
        title={title}
      >
        <WifiOff size={14} /> Offline — {ageLabel}{status.pending_events > 0 ? ` (${status.pending_events})` : ""}
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
          {retrying ? "..." : <RefreshCw size={12} />}
        </button>
      </span>
      {/* T05: reassure cashier that sales are safe while offline */}
      <span className="sync-offline-hint">Sales saved locally — sync resumes automatically</span>
    </span>
  );
});

export default SyncChip;
