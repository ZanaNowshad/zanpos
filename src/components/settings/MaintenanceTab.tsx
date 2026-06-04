import { useState, useEffect } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { authVerifyOwnerPin, checkForUpdates, downloadAndInstallUpdate } from "../../tauri/commands";

/**
 * Password-protected Maintenance page (Settings → Maintenance).
 *
 * Gated behind a re-entry of an OWNER PIN. The unlock is session-scoped: it
 * lasts only while this component is mounted (navigating away re-locks it).
 *
 * Contents: Application Update (check + download & install).
 */
export default function MaintenanceTab({ sessionUserId }: { sessionUserId: string }) {
  // ── Gate state ────────────────────────────────────────────────────────────
  const [unlocked, setUnlocked] = useState(false);
  const [pin, setPin] = useState("");
  const [verifying, setVerifying] = useState(false);
  const [gateError, setGateError] = useState<string | null>(null);

  const handleUnlock = async () => {
    if (!pin.trim() || verifying) return;
    setVerifying(true);
    setGateError(null);
    try {
      const ok = await authVerifyOwnerPin(pin.trim());
      if (ok) {
        setUnlocked(true);
        setPin("");
      } else {
        setGateError("Incorrect owner PIN.");
        setPin("");
      }
    } catch (e) {
      setGateError(typeof e === "string" ? e : "Verification failed. Try again.");
    } finally {
      setVerifying(false);
    }
  };

  // ── Update state ──────────────────────────────────────────────────────────
  const [appVersion, setAppVersion] = useState("");
  const [checking, setChecking] = useState(false);
  const [available, setAvailable] = useState<string | null>(null); // new version string
  const [updateMsg, setUpdateMsg] = useState<string | null>(null);
  const [installing, setInstalling] = useState(false);

  useEffect(() => {
    if (unlocked) getVersion().then(setAppVersion).catch(() => {});
  }, [unlocked]);

  const handleCheck = async () => {
    setChecking(true);
    setUpdateMsg(null);
    setAvailable(null);
    try {
      const version = await checkForUpdates();
      if (version) {
        setAvailable(version);
        setUpdateMsg(`Update available: v${version}`);
      } else {
        setUpdateMsg("You're on the latest version.");
      }
    } catch {
      setUpdateMsg("Update check failed. Check your connection and try again.");
    } finally {
      setChecking(false);
    }
  };

  const handleInstall = async () => {
    if (installing) return;
    setInstalling(true);
    setUpdateMsg("Downloading and installing… the app will restart automatically.");
    try {
      const did = await downloadAndInstallUpdate(sessionUserId);
      // If we get here, no restart happened (e.g. no update found mid-flight).
      if (!did) {
        setUpdateMsg("No update was available to install.");
        setAvailable(null);
      }
    } catch (e) {
      setUpdateMsg(`Install failed: ${typeof e === "string" ? e : "unknown error"}`);
    } finally {
      setInstalling(false);
    }
  };

  // ── Locked view: owner PIN entry ──────────────────────────────────────────
  if (!unlocked) {
    return (
      <div className="settings-page">
        <section>
          <h3 className="settings-page-title">🔒 Maintenance — Locked</h3>
          <p className="settings-backup-hint" style={{ marginBottom: 16 }}>
            This page is protected. Enter an <strong>owner PIN</strong> to continue.
          </p>
          <label className="setup-label" style={{ maxWidth: 280 }}>
            Owner PIN
            <input
              type="password"
              inputMode="numeric"
              autoComplete="off"
              className="setup-input"
              placeholder="••••"
              value={pin}
              onChange={(e) => setPin(e.target.value.replace(/\D/g, ""))}
              onKeyDown={(e) => e.key === "Enter" && handleUnlock()}
              disabled={verifying}
              autoFocus
            />
          </label>
          {gateError && <p className="setup-error">{gateError}</p>}
          <div style={{ marginTop: 14 }}>
            <button className="btn-primary" onClick={handleUnlock} disabled={verifying || !pin.trim()}>
              {verifying ? "Verifying…" : "Unlock"}
            </button>
          </div>
        </section>
      </div>
    );
  }

  // ── Unlocked view: Application Update ──────────────────────────────────────
  return (
    <div className="settings-page">
      <section>
        <h3 className="settings-page-title">Application Update</h3>
        <div className="update-row">
          <div>
            <div className="update-version-label">Current Version</div>
            <div className="update-version-value">{appVersion || "…"}</div>
          </div>
          <button className="btn-secondary" onClick={handleCheck} disabled={checking || installing}>
            {checking ? "Checking…" : "Check for Updates"}
          </button>
        </div>

        {available && (
          <div className="update-row" style={{ marginTop: 12 }}>
            <div>
              <div className="update-version-label">New Version Available</div>
              <div className="update-version-value">v{available}</div>
            </div>
            <button className="btn-primary" onClick={handleInstall} disabled={installing}>
              {installing ? "Installing…" : "Download & Install"}
            </button>
          </div>
        )}

        {updateMsg && (
          <div
            className={`update-msg ${available ? "update-msg-available" : "update-msg-ok"}`}
            role="status"
            aria-live="polite"
            style={{ marginTop: 12 }}
          >
            {updateMsg}
          </div>
        )}

        <p className="settings-backup-hint" style={{ marginTop: 16 }}>
          Installing downloads the new version, applies it, and restarts the app automatically.
          Make sure no sale is in progress before installing.
        </p>
      </section>
    </div>
  );
}
