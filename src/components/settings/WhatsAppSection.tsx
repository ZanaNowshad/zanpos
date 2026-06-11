import { useCallback, useEffect, useState } from "react";
import type { ImportContactsResult, WhatsAppStatus } from "../../types";
import {
  appConfigLoad,
  whatsappDisconnect,
  whatsappImportContacts,
  whatsappSaveConfig,
  whatsappStatus,
} from "../../tauri/commands";
import WhatsAppQRModal from "../WhatsAppQRModal";

export default function WhatsAppSection({
  sessionUserId,
  sessionRole,
  registerTimer,
}: {
  sessionUserId: string;
  sessionRole: string;
  registerTimer: (id: ReturnType<typeof setTimeout>) => void;
}) {
  const [status, setStatus]               = useState<WhatsAppStatus>({ connected: false });
  const [showQR, setShowQR]               = useState(false);
  const [benefitNum, setBenefitNum]       = useState("");
  const [saving, setSaving]               = useState(false);
  const [saved, setSaved]                 = useState(false);
  const [saveError, setSaveError]         = useState<string | null>(null);
  const [disconnecting, setDisconnecting] = useState(false);
  const [confirmDisconnect, setConfirmDisconnect] = useState(false);
  const [importing, setImporting]         = useState(false);
  const [importResult, setImportResult]   = useState<ImportContactsResult | null>(null);
  const [importError, setImportError]     = useState<string | null>(null);

  const isManager = sessionRole === "owner" || sessionRole === "manager";

  const refresh = useCallback(async () => {
    // R-17: log polling errors instead of silently swallowing — a persistently
    // unreachable sidecar should leave a diagnostic trail.
    try { setStatus(await whatsappStatus(sessionUserId)); }
    catch (e: unknown) { console.warn("WhatsApp status poll failed:", e); }
  }, []);

  useEffect(() => {
    refresh();
    appConfigLoad().then(cfg => {
      setBenefitNum(cfg.whatsapp_benefit_number ?? "");
    }).catch(() => {});
  }, [refresh]);

  // BUG-WA-PHONE-VALIDATION: validate phone number before calling the Tauri command.
  const validateBenefitNum = (val: string): string | null => {
    const v = val.trim();
    if (v === "") return null; // empty = clear setting, allowed
    if (!v.startsWith("+"))
      return "Phone number must start with '+' followed by the country code (e.g. +97333050666)";
    const afterPlus = v.slice(1);
    if (afterPlus.length === 0 || !/^\d+$/.test(afterPlus))
      return "Phone number must contain only digits after '+'";
    if (v.length < 8 || v.length > 16)
      return "Phone number must be 8–16 characters including the '+' prefix";
    return null;
  };

  const handleSave = async () => {
    setSaveError(null);
    const validationError = validateBenefitNum(benefitNum);
    if (validationError) {
      setSaveError(validationError);
      return;
    }
    setSaving(true);
    try {
      await whatsappSaveConfig(benefitNum.trim(), sessionUserId);
      setSaved(true);
      registerTimer(setTimeout(() => setSaved(false), 2000));
    } catch (e: unknown) {
      setSaveError(typeof e === "string" ? e : "Failed to save");
    } finally {
      setSaving(false);
    }
  };

  const handleDisconnect = async () => {
    setConfirmDisconnect(false);
    setDisconnecting(true);
    try {
      await whatsappDisconnect(sessionUserId);
      await refresh();
    } catch (e: unknown) {
      setSaveError(typeof e === "string" ? e : "Failed to disconnect");
    } finally {
      setDisconnecting(false);
    }
  };

  const handleImportContacts = useCallback(async () => {
    setImporting(true);
    setImportResult(null);
    setImportError(null);
    try {
      const result = await whatsappImportContacts(sessionUserId);
      setImportResult(result);
      registerTimer(setTimeout(() => setImportResult(null), 8_000));
    } catch (e: unknown) {
      setImportError(typeof e === "string" ? e : "Failed to import contacts");
      registerTimer(setTimeout(() => setImportError(null), 6_000));
    } finally {
      setImporting(false);
    }
  }, [sessionUserId, registerTimer]);

  const handleConnected = useCallback(async () => {
    await refresh();
    // Baileys fires contacts.set (the full contact list) several seconds AFTER the
    // connection.update "open" event.  12 s gives WhatsApp time to finish the
    // initial contact-sync before we pull from the sidecar.
    const t = setTimeout(handleImportContacts, 12_000);
    registerTimer(t);
  }, [refresh, handleImportContacts, registerTimer]);

  return (
    <>
      <div className="wa-settings-status-row">
        <span className={`wa-settings-badge ${status.connected ? "wa-badge-on" : "wa-badge-off"}`}>
          {status.connected ? "● Connected" : "● Disconnected"}
        </span>
        {!status.connected && isManager && (
          <button className="btn-primary btn-sm" onClick={() => setShowQR(true)}>
            Connect (Scan QR)
          </button>
        )}
        {status.connected && isManager && (
          <>
            <button className="btn-secondary btn-sm" onClick={() => setConfirmDisconnect(true)} disabled={disconnecting}>
              {disconnecting ? "Disconnecting…" : "Disconnect"}
            </button>
            {confirmDisconnect && (
              <div className="settings-confirm-overlay">
                <div className="settings-confirm-box">
                  <div className="settings-confirm-text">Disconnect WhatsApp? You will need to scan the QR code again.</div>
                  <div className="settings-confirm-actions">
                    <button className="btn-secondary btn-sm" onClick={() => setConfirmDisconnect(false)}>Cancel</button>
                    <button className="btn-danger btn-sm" onClick={handleDisconnect}>Disconnect</button>
                  </div>
                </div>
              </div>
            )}
          </>
        )}
      </div>

      {isManager && (
        <div className="wa-import-row">
          <div className="wa-import-info">
            <span className="wa-import-label">Contact Import</span>
            <span className="wa-import-hint">
              Save all WhatsApp contacts into the POS customer list.
            </span>
          </div>
          <button className="btn-secondary btn-sm" onClick={handleImportContacts} disabled={importing}>
            {importing ? "Importing…" : "Import Contacts"}
          </button>
        </div>
      )}
      {importResult !== null && (
        <div className="wa-import-result wa-import-result-ok" role="status" aria-live="polite">
          ✅ {importResult.imported} contact{importResult.imported !== 1 ? "s" : ""} imported
          {importResult.skipped > 0 ? ` — ${importResult.skipped} already existed` : ""}
        </div>
      )}
      {importError !== null && (
        <div className="wa-import-result wa-import-result-err" role="alert">{importError}</div>
      )}
      {saveError && (
        <div className="wa-import-result wa-import-result-err" role="alert">{saveError}</div>
      )}

      <label className="bo-label" style={{ marginTop: "16px" }}>BenefitPay Number</label>
      <p className="settings-hint">Sent in delivery messages so customers can pay you.</p>
      <div className="wa-benefit-row">
        <input
          className="bo-input"
          placeholder="e.g. 33050666"
          value={benefitNum}
          onChange={e => { setBenefitNum(e.target.value); setSaved(false); }}
          maxLength={20}
          disabled={!isManager}
          aria-label="BenefitPay Number"
        />
        {isManager && (
          <button className="btn-primary btn-sm" onClick={handleSave} disabled={saving}>
            {saving ? "Saving…" : saved ? "✓ Saved" : "Save"}
          </button>
        )}
      </div>

      {showQR && (
        <WhatsAppQRModal onClose={() => setShowQR(false)} onConnected={handleConnected} sessionUserId={sessionUserId} />
      )}
    </>
  );
}
