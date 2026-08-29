import { useCallback, useEffect, useState } from "react";
import {
  reconciliationPreview, reconciliationRun, terminalRoster,
  type ReconciliationOutcome, type ReconciliationPreview, type TerminalRow,
} from "../../tauri/commands";

/**
 * Which tills are there, whether they are keeping up, and what to do when one
 * is not.
 *
 * The old screen listed live socket connections to the hub, which is a
 * different question and answered "online" for a terminal that had never once
 * contacted it. Every state here is derived from heartbeat evidence, and the
 * advice line comes from the backend rather than being written again in the UI
 * — the meaning of "stale" is defined once, next to the thresholds.
 */

const STATE_LABEL: Record<TerminalRow["state"], string> = {
  online: "Online",
  stale: "Not just now",
  offline: "Offline",
  never_seen: "Never checked in",
  unpaired: "Not bound",
};

/** How long ago, in units somebody reads at a glance. */
function lastSeen(seconds: number | null): string {
  if (seconds === null) return "never";
  if (seconds < 90) return "just now";
  if (seconds < 3_600) return `${Math.round(seconds / 60)}m ago`;
  if (seconds < 172_800) return `${Math.round(seconds / 3_600)}h ago`;
  return `${Math.round(seconds / 86_400)}d ago`;
}

export default function TerminalRosterPanel({
  actorUserId, canRepair,
}: { actorUserId: string; canRepair: boolean }) {
  const [rows, setRows] = useState<TerminalRow[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<ReconciliationPreview | null>(null);
  const [outcomes, setOutcomes] = useState<ReconciliationOutcome[] | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(() => {
    terminalRoster(actorUserId)
      .then(setRows)
      .catch((cause: unknown) =>
        setError(typeof cause === "string" ? cause : "Could not read the terminal list."));
  }, [actorUserId]);

  useEffect(() => {
    load();
    // Heartbeats arrive on their own schedule, so a screen left open goes stale
    // in exactly the way it is meant to be reporting on.
    const timer = setInterval(load, 30_000);
    return () => clearInterval(timer);
  }, [load]);

  const runPreview = (table: string) => {
    setBusy(true);
    setOutcomes(null);
    reconciliationPreview(actorUserId, table)
      .then(setPreview)
      .catch((cause: unknown) =>
        setError(typeof cause === "string" ? cause : "Could not compare with the hub."))
      .finally(() => setBusy(false));
  };

  const runRepair = () => {
    if (!preview) return;
    setBusy(true);
    reconciliationRun(actorUserId, preview.table)
      .then(result => { setOutcomes(result); setPreview(null); load(); })
      .catch((cause: unknown) =>
        setError(typeof cause === "string" ? cause : "The repair could not run."))
      .finally(() => setBusy(false));
  };

  if (error) return <div className="modal-error">{error}</div>;
  if (!rows) return <p className="settings-hint">Reading the terminal list…</p>;
  if (rows.length === 0) return <p className="settings-hint">No terminals are registered yet.</p>;

  return (
    <div className="terminal-roster">
      <h4>Terminals</h4>
      <div className="terminal-grid">
        {rows.map(row => (
          <div key={row.device_id} className={`terminal-tile terminal-${row.state}`}>
            <div className="terminal-tile-head">
              <strong>{row.device_code}</strong>
              <span className={`terminal-state terminal-state-${row.state}`}>
                {STATE_LABEL[row.state]}
              </span>
            </div>
            <div className="terminal-tile-name">{row.name}</div>
            <dl className="terminal-facts">
              <div><dt>Last seen</dt><dd>{lastSeen(row.seconds_since_seen)}</dd></div>
              <div><dt>Address</dt><dd>{row.observed_ip ?? "—"}</dd></div>
              <div><dt>Version</dt><dd>{row.app_version ?? "—"}</dd></div>
            </dl>
            {/* Only when there is something to do about it. Repeating "Serving
                normally" under every healthy till trains people to stop reading
                the line that matters. */}
            {row.state !== "online" && <p className="terminal-advice">{row.advice}</p>}
            {!row.is_active && <p className="terminal-advice">Deactivated in the device roster.</p>}
          </div>
        ))}
      </div>

      <h4>Compare with the hub</h4>
      <p className="settings-hint">
        Names the rows that differ, and says which can be repaired safely. Nothing
        is changed until you ask for it.
      </p>
      <div className="terminal-actions">
        {["products", "product_prices", "sales", "customers"].map(table => (
          <button key={table} className="btn-secondary" disabled={busy}
                  onClick={() => runPreview(table)}>
            {table}
          </button>
        ))}
      </div>

      {preview && <PreviewResult preview={preview} canRepair={canRepair}
                                 busy={busy} onRepair={runRepair} />}
      {outcomes && <RepairResult outcomes={outcomes} />}
    </div>
  );
}

function PreviewResult({
  preview, canRepair, busy, onRepair,
}: {
  preview: ReconciliationPreview; canRepair: boolean; busy: boolean; onRepair: () => void;
}) {
  if (preview.hub_too_old) {
    // Not a clean bill. `settings-action-msg` on its own renders green, which
    // would read as "checked and fine" for the one case where nothing was
    // checked at all.
    return (
      <div className="settings-action-msg settings-action-err">
        The hub is on an older build and cannot compare individual rows yet.
        Update the hub terminal first.
      </div>
    );
  }
  if (preview.diverged === 0) {
    return (
      <div className="settings-action-msg">
        {preview.table} matches the hub exactly.
      </div>
    );
  }

  return (
    <div className="reconcile-preview">
      <p><strong>{preview.diverged}</strong> row(s) in {preview.table} differ from the hub.</p>

      {preview.deliverable.length > 0 && (
        <p className="reconcile-safe">
          {preview.deliverable.length} can be repaired without anyone deciding —
          only one side holds them, so nothing is overwritten.
        </p>
      )}

      {/* The important half. These are rows both terminals hold with different
          contents, and for a sale or a payment the copy that would lose may be
          the only record that a customer handed over money. */}
      {preview.needs_review.length > 0 && (
        <div className="reconcile-review">
          <p>
            <strong>
              {preview.needs_review.length === 1
                ? "1 needs a person to look."
                : `${preview.needs_review.length} need a person to look.`}
            </strong>{" "}
            Both this terminal and the hub hold {preview.needs_review.length === 1 ? "it" : "them"}{" "}
            with different contents, so repairing would discard one of two real edits.
            These are never repaired automatically.
          </p>
          <ul>{preview.needs_review.slice(0, 10).map(pk => <li key={pk}><code>{pk}</code></li>)}</ul>
        </div>
      )}

      {canRepair && preview.deliverable.length > 0 && (
        <button className="btn-primary" onClick={onRepair} disabled={busy}>
          {busy ? "Repairing…" : `Deliver ${preview.deliverable.length} missing row(s)`}
        </button>
      )}
      {!canRepair && preview.deliverable.length > 0 && (
        <p className="settings-hint">A manager can repair these.</p>
      )}
    </div>
  );
}

function RepairResult({ outcomes }: { outcomes: ReconciliationOutcome[] }) {
  return (
    <div className="reconcile-outcome">
      {outcomes.map(outcome => (
        <div key={outcome.table}>
          <p>
            {/* diverged_after is measured after the repair, not predicted before
                it: a repair that quietly did not take is worse than one that
                never ran, because it reports success. */}
            <strong>{outcome.table}</strong> — {outcome.delivered_from_hub} pulled,{" "}
            {outcome.delivered_to_hub} pushed,{" "}
            {outcome.diverged_after === 0
              ? "now matching."
              : outcome.diverged_after === 1
                ? "1 still differs."
                : `${outcome.diverged_after} still differ.`}
          </p>
          {outcome.left_for_review.length > 0 && (
            <p className="reconcile-review">
              {outcome.left_for_review.length === 1
                ? "1 left for review: it was not touched."
                : `${outcome.left_for_review.length} left for review: they were not touched.`}
            </p>
          )}
        </div>
      ))}
    </div>
  );
}
