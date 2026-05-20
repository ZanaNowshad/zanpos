import { useCallback, useEffect, useState } from "react";
import type { BranchSettings, ThermalConfig, WhatsAppStatus } from "../types";
import {
  settingsGetBranch,
  settingsUpdateBranch,
  appConfigGetTimeout,
  appConfigSetTimeout,
  dbBackup,
  thermalGetConfig,
  thermalSetConfig,
  thermalPrintTest,
  checkForUpdates,
  whatsappStatus,
  whatsappDisconnect,
  whatsappSaveConfig,
  appConfigLoad,
} from "../tauri/commands";
import WhatsAppQRModal from "./WhatsAppQRModal";

function WhatsAppSettingsSection({
  sessionUserId,
  sessionRole,
}: {
  sessionUserId: string;
  sessionRole: string;
}) {
  const [status, setStatus]               = useState<WhatsAppStatus>({ connected: false });
  const [showQR, setShowQR]               = useState(false);
  const [benefitNum, setBenefitNum]       = useState("");
  const [saving, setSaving]               = useState(false);
  const [saved, setSaved]                 = useState(false);
  const [disconnecting, setDisconnecting] = useState(false);

  const isManager = sessionRole === "owner" || sessionRole === "manager";

  const refresh = useCallback(async () => {
    try { setStatus(await whatsappStatus()); } catch { /* ignore */ }
  }, []);

  useEffect(() => {
    refresh();
    appConfigLoad().then(cfg => {
      setBenefitNum(cfg.whatsapp_benefit_number ?? "");
    }).catch(() => {});
  }, [refresh]);

  const handleSave = async () => {
    setSaving(true);
    try {
      await whatsappSaveConfig(benefitNum.trim(), sessionUserId);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e: unknown) {
      alert(typeof e === "string" ? e : "Failed to save");
    } finally {
      setSaving(false);
    }
  };

  const handleDisconnect = async () => {
    if (!confirm("Disconnect WhatsApp? You will need to scan the QR code again.")) return;
    setDisconnecting(true);
    try {
      await whatsappDisconnect(sessionUserId);
      await refresh();
    } catch (e: unknown) {
      alert(typeof e === "string" ? e : "Failed to disconnect");
    } finally {
      setDisconnecting(false);
    }
  };

  return (
    <>
      <h3 className="settings-section-title">📱 WhatsApp</h3>

      <div className="wa-settings-status-row">
        <span className={`wa-settings-badge ${status.connected ? "wa-badge-on" : "wa-badge-off"}`}>
          {status.connected ? "🟢 Connected" : "🔴 Disconnected"}
        </span>
        {!status.connected && isManager && (
          <button className="btn-primary btn-sm" onClick={() => setShowQR(true)}>
            Connect (Scan QR)
          </button>
        )}
        {status.connected && isManager && (
          <button className="btn-secondary btn-sm" onClick={handleDisconnect} disabled={disconnecting}>
            {disconnecting ? "Disconnecting…" : "Disconnect"}
          </button>
        )}
      </div>

      <label className="bo-label">BenefitPay Number</label>
      <p className="settings-hint">Sent in delivery WhatsApp messages so customers can pay you.</p>
      <div className="wa-benefit-row">
        <input
          className="bo-input"
          placeholder="e.g. 33050666"
          value={benefitNum}
          onChange={e => { setBenefitNum(e.target.value); setSaved(false); }}
          maxLength={20}
          disabled={!isManager}
        />
        {isManager && (
          <button className="btn-primary btn-sm" onClick={handleSave} disabled={saving}>
            {saving ? "Saving…" : saved ? "✓ Saved" : "Save"}
          </button>
        )}
      </div>

      {showQR && (
        <WhatsAppQRModal
          onClose={() => setShowQR(false)}
          onConnected={refresh}
        />
      )}
    </>
  );
}

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

interface Props { sessionUserId: string; sessionRole: string; }

export default function SettingsTab({ sessionUserId, sessionRole }: Props) {
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

  // Thermal printer
  const [thermal, setThermal]           = useState<ThermalConfig>({ enabled: false, port: "", baud: "9600" });
  const [savingThermal, setSavingThermal] = useState(false);
  const [savedThermal, setSavedThermal]   = useState(false);
  const [testingPrint, setTestingPrint]   = useState(false);
  const [printTestMsg, setPrintTestMsg]   = useState<string | null>(null);

  // Auto-updater
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [updateMsg, setUpdateMsg]           = useState<string | null>(null);

  useEffect(() => {
    Promise.all([
      settingsGetBranch(),
      appConfigGetTimeout().catch(() => 5),
      thermalGetConfig().catch(() => ({ enabled: false, port: "", baud: "9600" })),
    ])
      .then(([s, minutes, tc]) => {
        setSettings(s as BranchSettings);
        setName((s as BranchSettings).name);
        setTimezone((s as BranchSettings).timezone);
        setAddress((s as BranchSettings).address ?? "");
        setPhone((s as BranchSettings).phone ?? "");
        setTaxNumber((s as BranchSettings).tax_number ?? "");
        setReceiptHeader((s as BranchSettings).receipt_header ?? "");
        setReceiptFooter((s as BranchSettings).receipt_footer ?? "");
        setTimeoutMinutes(minutes as number);
        setThermal(tc as ThermalConfig);
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
        actor_user_id:  sessionUserId,
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
      await appConfigSetTimeout(timeoutMinutes, sessionUserId);
      setSavedTimeout(true);
      setTimeout(() => setSavedTimeout(false), 3000);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to save timeout");
    } finally {
      setSavingTimeout(false);
    }
  };

  const handleSaveThermal = async () => {
    setSavingThermal(true);
    setPrintTestMsg(null);
    try {
      await thermalSetConfig(thermal);
      setSavedThermal(true);
      setTimeout(() => setSavedThermal(false), 3000);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to save printer settings");
    } finally {
      setSavingThermal(false);
    }
  };

  const handleTestPrint = async () => {
    setTestingPrint(true);
    setPrintTestMsg(null);
    try {
      const msg = await thermalPrintTest();
      setPrintTestMsg(msg);
    } catch (e: unknown) {
      setPrintTestMsg(typeof e === "string" ? e : "Test failed");
    } finally {
      setTestingPrint(false);
    }
  };

  const handleCheckUpdate = async () => {
    setCheckingUpdate(true);
    setUpdateMsg(null);
    try {
      const version = await checkForUpdates();
      if (version) {
        setUpdateMsg(`Version ${version} is available — restart to install.`);
      } else {
        setUpdateMsg("You are up to date.");
      }
    } catch (e: unknown) {
      setUpdateMsg(typeof e === "string" ? e : "Update check failed");
    } finally {
      setCheckingUpdate(false);
    }
  };

  const handleBackup = async () => {
    setBackupMsg(null);
    setBackingUp(true);
    try {
      // Pass empty string — Rust will auto-generate a timestamped path in Documents
      const savedTo = await dbBackup("", sessionUserId);
      setBackupMsg(`Backup saved to: ${savedTo}`);
    } catch (e: unknown) {
      setBackupMsg(typeof e === "string" ? e : "Backup failed");
    } finally {
      setBackingUp(false);
    }
  };

  const scrollTo = (id: string) => {
    document.getElementById(id)?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  const SUB_NAV = [
    { id: "s-business",  label: "Store Settings" },
    { id: "s-receipt",   label: "Receipt Customisation" },
    { id: "s-security",  label: "Security" },
    { id: "s-printer",   label: "Printers" },
    { id: "s-whatsapp",  label: "WhatsApp" },
    { id: "s-app",       label: "Application" },
  ];

  if (loading) return <div className="bo-empty">Loading settings…</div>;

  return (
    <div className="settings-layout">
      <nav className="settings-subnav">
        <div className="settings-subnav-label">Settings</div>
        {SUB_NAV.map(n => (
          <button key={n.id} className="settings-subnav-item" onClick={() => scrollTo(n.id)}>
            {n.label}
          </button>
        ))}
      </nav>

      <div className="settings-scroll">
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
        <section id="s-business" className="settings-section">
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

        <section id="s-receipt" className="settings-section">
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
                {taxNumber && <div className="receipt-mini-addr">TRN: {taxNumber}</div>}
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

        <section id="s-security" className="settings-section">
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

        {/* ── Thermal Printer ── */}
        <section id="s-printer" className="settings-section">
          <h3 className="settings-section-title">Receipt Printer (ESC/POS)</h3>
          <p className="settings-hint">
            Requires <code>tauri-plugin-serialport</code> for full hardware integration.
            Configure the port and baud rate, then use "Test Print" to verify.
          </p>

          <div className="thermal-row">
            <label className="bo-checkbox-label">
              <input
                type="checkbox"
                checked={thermal.enabled}
                onChange={e => setThermal(t => ({ ...t, enabled: e.target.checked }))}
              />
              Enable thermal printing
            </label>
          </div>

          <div className="bo-row-two">
            <div>
              <label className="bo-label">Serial Port</label>
              <input
                className="bo-input"
                value={thermal.port}
                onChange={e => setThermal(t => ({ ...t, port: e.target.value }))}
                placeholder="COM3 or /dev/ttyUSB0"
                disabled={!thermal.enabled}
              />
            </div>
            <div>
              <label className="bo-label">Baud Rate</label>
              <select
                className="bo-select"
                value={thermal.baud}
                onChange={e => setThermal(t => ({ ...t, baud: e.target.value }))}
                disabled={!thermal.enabled}
              >
                <option value="9600">9600</option>
                <option value="19200">19200</option>
                <option value="38400">38400</option>
                <option value="115200">115200</option>
              </select>
            </div>
          </div>

          <div className="thermal-actions">
            <button
              className="btn-secondary"
              onClick={handleSaveThermal}
              disabled={savingThermal}
            >
              {savingThermal ? "Saving…" : savedThermal ? "Saved ✓" : "Save Printer Settings"}
            </button>
            <button
              className="btn-secondary"
              onClick={handleTestPrint}
              disabled={testingPrint || !thermal.enabled}
            >
              {testingPrint ? "Testing…" : "Test Print"}
            </button>
          </div>
          {printTestMsg && (
            <pre className="thermal-test-msg">{printTestMsg}</pre>
          )}
        </section>

        {/* ── WhatsApp ── */}
        <section id="s-whatsapp" className="settings-section">
          <WhatsAppSettingsSection
            sessionUserId={sessionUserId}
            sessionRole={sessionRole}
          />
        </section>

        {/* ── Application / Updater ── */}
        <section id="s-app" className="settings-section">
          <h3 className="settings-section-title">Application</h3>
          <div className="update-row">
            <div>
              <div className="update-version-label">Current Version</div>
              <div className="update-version-value">0.1.0</div>
            </div>
            <button
              className="btn-secondary"
              onClick={handleCheckUpdate}
              disabled={checkingUpdate}
            >
              {checkingUpdate ? "Checking…" : "Check for Updates"}
            </button>
          </div>
          {updateMsg && (
            <div className={`update-msg ${updateMsg.includes("available") ? "update-msg-available" : "update-msg-ok"}`}>
              {updateMsg}
            </div>
          )}
        </section>

        {error && <div className="modal-error">{error}</div>}

        <div className="settings-actions">
          {saved && <span className="settings-saved">✓ Saved</span>}
          <button className="btn-primary settings-save-btn" onClick={handleSave} disabled={saving}>
            {saving ? "Saving…" : "Save Changes"}
          </button>
        </div>
      </div>
      </div>{/* settings-scroll */}
    </div>
  );
}
