import { useState } from "react";
import { flushDiagnosticsNow } from "../../tauri/diagnostics";

interface SystemTabProps {
  appVersion: string;
  backingUp: boolean; backupMsg: string | null;
  checkingUpdate: boolean; updateMsg: string | null;
  handleBackup: () => void; handleCheckUpdate: () => void;
}

export default function SystemTab(props: SystemTabProps) {
  const { appVersion, backingUp, backupMsg, checkingUpdate, updateMsg, handleBackup, handleCheckUpdate } = props;
  const [sendingDiagnostics, setSendingDiagnostics] = useState(false);
  const [diagnosticsMsg, setDiagnosticsMsg] = useState<string | null>(null);

  const handleSendDiagnostics = async () => {
    setSendingDiagnostics(true);
    setDiagnosticsMsg(null);
    try {
      const result = await flushDiagnosticsNow();
      setDiagnosticsMsg(result.message);
    } catch {
      setDiagnosticsMsg("Could not send diagnostics — will retry automatically.");
    } finally {
      setSendingDiagnostics(false);
    }
  };

  return (
    <div className="settings-page">
      <section>
        <h3 className="settings-page-title">Database Backup</h3>
        <div className="settings-backup-row">
          <div>
            <div className="settings-backup-label">Backup Database</div>
            <div className="settings-backup-hint">Copy the database to a safe location in Documents.</div>
          </div>
          <button className="btn-secondary settings-backup-btn" onClick={handleBackup} disabled={backingUp}>
            {backingUp ? "Backing up…" : "Backup Now"}
          </button>
        </div>
        {backupMsg && (
          <div className={`settings-backup-msg ${backupMsg.startsWith("Backup saved") ? "settings-backup-ok" : "modal-error"}`} role="status" aria-live="polite">
            {backupMsg}
          </div>
        )}
      </section>

      <hr className="settings-page-divider" />

      <section>
        <h3 className="settings-page-title">Updates</h3>
        <div className="update-row">
          <div>
            <div className="update-version-label">Application Version</div>
            <div className="update-version-value">{appVersion}</div>
          </div>
          <button className="btn-secondary" onClick={handleCheckUpdate} disabled={checkingUpdate}>
            {checkingUpdate ? "Checking…" : "Check for Updates"}
          </button>
        </div>
        {updateMsg && (
          <div className={`update-msg ${updateMsg.includes("available") ? "update-msg-available" : "update-msg-ok"}`} role="status" aria-live="polite">
            {updateMsg}
          </div>
        )}
        <div className="update-row" style={{ marginTop: 12 }}>
          <div>
            <div className="update-version-label">Diagnostics</div>
            <div className="update-version-value">Send any queued crash and error reports now.</div>
          </div>
          <button className="btn-secondary" onClick={handleSendDiagnostics} disabled={sendingDiagnostics}>
            {sendingDiagnostics ? "Sending…" : "Send Diagnostics"}
          </button>
        </div>
        {diagnosticsMsg && (
          <div className="update-msg update-msg-ok" role="status" aria-live="polite">
            {diagnosticsMsg}
          </div>
        )}
      </section>
    </div>
  );
}
