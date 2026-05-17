import { useEffect, useState } from "react";
import type { BranchSettings } from "../types";
import {
  settingsGetBranch,
  settingsUpdateBranch,
  appConfigGetTimeout,
  appConfigSetTimeout,
  dbBackup,
} from "../tauri/commands";

const TIMEZONES = [
  "Asia/Bahrain",
  "Asia/Riyadh",
  "Asia/Dubai",
  "Asia/Kuwait",
  "Asia/Muscat",
  "Asia/Qatar",
  "Africa/Cairo",
  "Europe/London",
  "America/New_York",
  "America/Los_Angeles",
  "Asia/Singapore",
];

const TIMEOUT_OPTIONS = [1, 2, 5, 10, 15, 30, 60];

export default function SettingsTab() {
  const [settings, setSettings] = useState<BranchSettings | null>(null);
  const [loading, setLoading]   = useState(true);
  const [saving, setSaving]     = useState(false);
  const [saved, setSaved]       = useState(false);
  const [error, setError]       = useState<string | null>(null);

  // Form state
  const [name, setName]                   = useState("");
  const [timezone, setTimezone]           = useState("Asia/Bahrain");
  const [address, setAddress]             = useState("");
  const [phone, setPhone]                 = useState("");
  const [taxNumber, setTaxNumber]         = useState("");
  const [receiptHeader, setReceiptHeader] = useState("");
  const [receiptFooter, setReceiptFooter] = useState("");

  // Session timeout
  const [timeoutMinutes, setTimeoutMinutes] = useState(5);
  const [savingTimeout, setSavingTimeout]   = useState(false);
  const [savedTimeout, setSavedTimeout]     = useState(false);

  // Backup
  const [backingUp, setBackingUp] = useState(false);
  const [backupMsg, setBackupMsg] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([settingsGetBranch(), appConfigGetTimeout().catch(() => 5)])
      .then(([s, minutes]) => {
        setSettings(s);
        setName(s.name);
        setTimezone(s.timezone);
        setAddress(s.address ?? "");
        setPhone(s.phone ?? "");
        setTaxNumber(s.tax_number ?? "");
        setReceiptHeader(s.receipt_header ?? "");
        setReceiptFooter(s.receipt_footer ?? "");
        setTimeoutMinutes(minutes as number);
      })
      .catch(() => setError("Failed to load settings"))
      .finally(() => setLoading(false));
  }, []);

  const handleSave = async () => {
    if (!name.trim()) { setError("Store name is required"); return; }
    setSaving(true);
    setError(null);
    setSaved(false);
    try {
      const updated = await settingsUpdateBranch({
        name:           name.trim(),
        timezone,
        address:        address.trim() || undefined,
        phone:          phone.trim() || undefined,
        tax_number:     taxNumber.trim() || undefined,
        receipt_header: receiptHeader.trim() || undefined,
        receipt_footer: receiptFooter.trim() || undefined,
      });
      setSettings(updated);
      setSaved(true);
      setTimeout(() => setSaved(false), 3000);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to save settings");
    } finally {
      setSaving(false);
    }
  };

  const handleSaveTimeout = async () => {
    setSavingTimeout(true);
    try {
      await appConfigSetTimeout(timeoutMinutes);
      setSavedTimeout(true);
      setTimeout(() => setSavedTimeout(false), 3000);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to save timeout");
    } finally {
      setSavingTimeout(false);
    }
  };

  const handleBackup = async () => {
    setBackupMsg(null);
    setBackingUp(true);
    try {
      // Pass empty string — Rust will auto-generate a timestamped path in Documents
      const savedTo = await dbBackup("");
      setBackupMsg(`Backup saved to: ${savedTo}`);
    } catch (e: unknown) {
      setBackupMsg(typeof e === "string" ? e : "Backup failed");
    } finally {
      setBackingUp(false);
    }
  };

  if (loading) return <div className="bo-empty">Loading settings…</div>;

  return (
    <div className="settings-layout">
      <div className="settings-header">
        <h2 className="settings-title">Store Settings</h2>
        {settings && (
          <div className="settings-meta">
            Branch ID: <code>{settings.branch_id}</code>
            &nbsp;·&nbsp; Currency: <strong>{settings.currency}</strong>
            &nbsp;·&nbsp; Code: <strong>{settings.branch_code}</strong>
          </div>
        )}
      </div>

      <div className="settings-form">
        <section className="settings-section">
          <h3 className="settings-section-title">Business Information</h3>

          <label className="bo-label">Store Name *</label>
          <input className="bo-input" type="text" value={name} onChange={e => setName(e.target.value)} maxLength={60} />

          <label className="bo-label">Timezone</label>
          <select className="bo-select" value={timezone} onChange={e => setTimezone(e.target.value)}>
            {TIMEZONES.map(tz => <option key={tz} value={tz}>{tz}</option>)}
          </select>

          <label className="bo-label">Address</label>
          <textarea className="bo-input" rows={3} value={address} onChange={e => setAddress(e.target.value)} placeholder="Full address printed on receipts" />

          <label className="bo-label">Phone Number</label>
          <input className="bo-input" type="tel" value={phone} onChange={e => setPhone(e.target.value)} placeholder="+973 1234 5678" />

          <label className="bo-label">Tax / VAT Registration Number</label>
          <input className="bo-input" type="text" value={taxNumber} onChange={e => setTaxNumber(e.target.value)} placeholder="e.g. VAT-1234567890" />

          {/* DB Backup */}
          <div className="settings-backup-row">
            <div>
              <div className="settings-backup-label">Database Backup</div>
              <div className="settings-backup-hint">Copy the database to a safe location.</div>
            </div>
            <button className="btn-secondary settings-backup-btn" onClick={handleBackup} disabled={backingUp}>
              {backingUp ? "Backing up…" : "Backup Database"}
            </button>
          </div>
          {backupMsg && (
            <div className={`settings-backup-msg ${backupMsg.startsWith("Backup saved") ? "settings-backup-ok" : "modal-error"}`}>
              {backupMsg}
            </div>
          )}
        </section>

        <section className="settings-section">
          <h3 className="settings-section-title">Receipt Customisation</h3>
          <p className="settings-hint">
            These lines are printed on every customer receipt.
          </p>

          <label className="bo-label">Receipt Header</label>
          <input
            className="bo-input"
            type="text"
            value={receiptHeader}
            onChange={e => setReceiptHeader(e.target.value)}
            placeholder="Printed above the item list (e.g. tagline)"
            maxLength={120}
          />

          <label className="bo-label">Receipt Footer</label>
          <input
            className="bo-input"
            type="text"
            value={receiptFooter}
            onChange={e => setReceiptFooter(e.target.value)}
            placeholder="Printed below the total (e.g. Thank you!)"
            maxLength={120}
          />

          {/* Live preview */}
          {(receiptHeader || receiptFooter || name) && (
            <div className="settings-receipt-preview">
              <div className="settings-preview-label">Receipt Preview</div>
              <div className="receipt-mini">
                {name && <div className="receipt-mini-biz">{name}</div>}
                {address && <div className="receipt-mini-addr">{address}</div>}
                {phone && <div className="receipt-mini-addr">{phone}</div>}
                {taxNumber && <div className="receipt-mini-addr">Tax: {taxNumber}</div>}
                {receiptHeader && <div className="receipt-mini-header">{receiptHeader}</div>}
                <div className="receipt-mini-divider">- - - - - - - - - -</div>
                <div className="receipt-mini-item">Item name × 1 .............. 1.500</div>
                <div className="receipt-mini-divider">- - - - - - - - - -</div>
                <div className="receipt-mini-total">TOTAL: 1.500</div>
                {receiptFooter && <div className="receipt-mini-footer">{receiptFooter}</div>}
              </div>
            </div>
          )}
        </section>

        <section className="settings-section">
          <h3 className="settings-section-title">Security</h3>

          <label className="bo-label">Session Timeout</label>
          <div className="settings-timeout-row">
            <select
              className="bo-select settings-timeout-select"
              value={timeoutMinutes}
              onChange={e => setTimeoutMinutes(Number(e.target.value))}
            >
              {TIMEOUT_OPTIONS.map(m => (
                <option key={m} value={m}>{m} minute{m !== 1 ? "s" : ""}</option>
              ))}
            </select>
            <button
              className="btn-secondary settings-timeout-btn"
              onClick={handleSaveTimeout}
              disabled={savingTimeout}
            >
              {savingTimeout ? "Saving…" : savedTimeout ? "Saved ✓" : "Save Timeout"}
            </button>
          </div>
          <p className="settings-hint">Lock the screen after this many minutes of inactivity.</p>
        </section>

        {error && <div className="modal-error">{error}</div>}

        <div className="settings-actions">
          {saved && <span className="settings-saved">✓ Saved</span>}
          <button className="btn-primary settings-save-btn" onClick={handleSave} disabled={saving}>
            {saving ? "Saving…" : "Save Changes"}
          </button>
        </div>
      </div>
    </div>
  );
}
