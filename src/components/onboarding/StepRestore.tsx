import { useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { backupRestoreFile } from "../../tauri/backup";
import Button from "../ui/Button";

interface Props {
  /** Actor id. On a fresh machine there is no owner yet, so the backend only
   *  enforces the owner check once the store HAS users — see backup.rs. */
  ownerUserId: string;
  onRestored: (path: string) => void;
  onSkip: () => void;
}

/**
 * Step 0 — "Restoring from a backup?"
 *
 * Offered before a store sets itself up, because the person standing in front
 * of a dead till does not want to type their whole catalogue again. This is
 * the recovery path for the failure the off-site backup exists to survive.
 *
 * It decrypts to a NEW file and will not overwrite anything. That refusal is
 * deliberate and surfaced to the operator: a restore that silently clobbers a
 * working store because the wrong file was picked is a worse outage than the
 * one being recovered from. Swapping the restored file into place is a
 * separate, conscious step taken with the app closed.
 */
export default function StepRestore({ ownerUserId, onRestored, onSkip }: Props) {
  const [licenseKey, setLicenseKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [restoredTo, setRestoredTo] = useState<string | null>(null);

  const run = async () => {
    setError(null);
    const encrypted = await open({
      title: "Choose your backup file",
      multiple: false,
      filters: [{ name: "ZANPOS backup", extensions: ["enc"] }],
    });
    if (typeof encrypted !== "string") return;

    const destination = await save({
      title: "Save the restored database as…",
      defaultPath: "zanpos-restored.db",
      filters: [{ name: "SQLite database", extensions: ["db"] }],
    });
    if (!destination) return;

    setBusy(true);
    try {
      const written = await backupRestoreFile(
        ownerUserId,
        encrypted,
        destination,
        licenseKey.trim() || undefined,
      );
      setRestoredTo(written);
      onRestored(written);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  if (restoredTo) {
    return (
      <div className="setup-content">
        <div className="setup-title">Backup restored</div>
        <p className="setup-body">
          Your data was decrypted to:<br />
          <code>{restoredTo}</code>
        </p>
        <p className="setup-body">
          Close ZANPOS, replace your database file with this one, then reopen.
          Nothing has been overwritten yet — that step is yours.
        </p>
        <div className="setup-actions">
          <Button role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}  variant="secondary" onClick={onSkip}>Continue setup instead</Button>
        </div>
      </div>
    );
  }

  return (
    <div className="setup-content">
      <div className="setup-title">Restoring from a backup?</div>
      <p className="setup-body">
        If this store has run ZANPOS before and you have its backup file, restore
        it now instead of setting up again.
      </p>
      <label htmlFor="a11y-input-1">
        Licence key <small>(only if this is a new machine)</small>
        <input id="a11y-input-1"
          value={licenseKey}
          onChange={event => setLicenseKey(event.target.value)}
          placeholder="Leave blank to use this machine's own key"
          autoComplete="off"
        />
      </label>
      {error && <p className="setup-body" role="alert">{error}</p>}
      <div className="setup-actions">
        <Button role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}  variant="primary" busy={busy} onClick={run}>Choose backup file</Button>
        <Button role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}  variant="ghost" onClick={onSkip}>No, set up a new store</Button>
      </div>
    </div>
  );
}
