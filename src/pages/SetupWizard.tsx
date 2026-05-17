import { useState } from "react";
import type { AppConfig } from "../types";
import {
  setupWizardComplete,
  setupJoinStore,
  adminSetupSupabase,
  adminSetupSupabaseCredsOnly,
} from "../tauri/commands";

interface Props {
  onComplete: (config: AppConfig) => void;
}

// ─── Constants ────────────────────────────────────────────────────────────────

const CURRENCIES = [
  { code: "BHD", label: "BHD — Bahraini Dinar" },
  { code: "SAR", label: "SAR — Saudi Riyal" },
  { code: "AED", label: "AED — UAE Dirham" },
  { code: "KWD", label: "KWD — Kuwaiti Dinar" },
  { code: "OMR", label: "OMR — Omani Rial" },
  { code: "QAR", label: "QAR — Qatari Riyal" },
  { code: "EGP", label: "EGP — Egyptian Pound" },
  { code: "USD", label: "USD — US Dollar" },
  { code: "EUR", label: "EUR — Euro" },
  { code: "GBP", label: "GBP — British Pound" },
];

const TIMEZONES = [
  "Asia/Bahrain", "Asia/Riyadh", "Asia/Dubai", "Asia/Kuwait",
  "Asia/Muscat", "Asia/Qatar", "Africa/Cairo",
  "Europe/London", "America/New_York", "America/Los_Angeles", "Asia/Singapore",
];

// ─── Path: New Store ──────────────────────────────────────────────────────────
// Steps: 1=Welcome/path 2=Cloud(optional) 3=StoreInfo 4=Contact 5=Owner 6=Review

type NewStep = 1 | 2 | 3 | 4 | 5 | 6;

function NewStoreWizard({ onComplete }: { onComplete: (cfg: AppConfig) => void }) {
  const [step, setStep] = useState<NewStep>(2); // skip path-selector inside sub-wizard

  // Step 2 — Cloud (soft-required)
  const [sbUrl, setSbUrl]         = useState("");
  const [sbKey, setSbKey]         = useState("");
  const [sbPat, setSbPat]         = useState("");
  const [sbSkipped, setSbSkipped] = useState(false);
  const [sbValidating, setSbValidating] = useState(false);

  // Step 3 — store info
  const [storeName, setStoreName] = useState("");
  const [currency, setCurrency]   = useState("BHD");
  const [timezone, setTimezone]   = useState("Asia/Bahrain");

  // Step 4 — contact + receipt
  const [address, setAddress]             = useState("");
  const [phone, setPhone]                 = useState("");
  const [taxNumber, setTaxNumber]         = useState("");
  const [receiptHeader, setReceiptHeader] = useState("");
  const [receiptFooter, setReceiptFooter] = useState("Thank you for your purchase!");

  // Step 5 — admin account
  const [ownerName, setOwnerName]           = useState("");
  const [ownerUsername, setOwnerUsername]   = useState("admin");
  const [ownerPin, setOwnerPin]             = useState("");
  const [ownerPinConfirm, setOwnerPinConfirm] = useState("");

  const [error, setError]   = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const clearError = () => setError(null);

  // ── Cloud step handlers ────────────────────────────────────────────────────

  const handleConnectCloud = async () => {
    if (!sbUrl.trim()) { setError("Supabase URL is required"); return; }
    if (!sbKey.trim()) { setError("Service role key is required"); return; }
    setSbValidating(true);
    setError(null);
    try {
      if (sbPat.trim()) {
        // Full setup: validate + run central schema migration
        await adminSetupSupabase(sbUrl.trim(), sbKey.trim(), sbPat.trim());
      } else {
        // Credentials-only: validate connection, store creds, defer schema migration
        await adminSetupSupabaseCredsOnly(sbUrl.trim(), sbKey.trim());
      }
      setSbSkipped(false);
      setStep(3);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Could not connect to Supabase — check URL and service role key");
    } finally {
      setSbValidating(false);
    }
  };

  const handleSkipCloud = () => {
    setSbSkipped(true);
    setError(null);
    setStep(3);
  };

  // ── Step validation ────────────────────────────────────────────────────────

  const goNext = () => {
    setError(null);
    if (step === 3 && !storeName.trim()) { setError("Store name is required"); return; }
    if (step === 5) {
      if (!ownerName.trim())     { setError("Owner name is required"); return; }
      if (!ownerUsername.trim()) { setError("Username is required"); return; }
      if (ownerPin.length < 4)   { setError("PIN must be at least 4 digits"); return; }
      if (ownerPin !== ownerPinConfirm) { setError("PINs do not match"); return; }
    }
    setStep(s => (s + 1) as NewStep);
  };

  const handleFinish = async () => {
    setLoading(true);
    setError(null);
    try {
      const cfg = await setupWizardComplete({
        store_name:         storeName.trim(),
        store_address:      address.trim()        || undefined,
        store_phone:        phone.trim()           || undefined,
        receipt_header:     receiptHeader.trim()   || undefined,
        receipt_footer:     receiptFooter.trim()   || undefined,
        tax_number:         taxNumber.trim()       || undefined,
        currency,
        timezone,
        owner_display_name: ownerName.trim(),
        owner_username:     ownerUsername.trim(),
        owner_pin:          ownerPin,
      });
      onComplete(cfg);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Setup failed — please try again");
      setStep(5);
    } finally {
      setLoading(false);
    }
  };

  // ── Render ─────────────────────────────────────────────────────────────────

  const LABELS = ["Cloud", "Store Info", "Contact", "Owner", "Review"];
  const stepIdx = step - 2; // 0-based for progress bar

  return (
    <div className="setup-panel">
      <div className="setup-progress">
        {LABELS.map((label, i) => (
          <div key={i} className={`setup-step-dot ${i <= stepIdx ? "setup-step-done" : ""} ${i === stepIdx ? "setup-step-active" : ""}`}>
            <div className="setup-dot-circle">{i < stepIdx ? "✓" : i + 1}</div>
            <div className="setup-dot-label">{label}</div>
          </div>
        ))}
      </div>

      {/* ── Step 2: Cloud connection ── */}
      {step === 2 && (
        <div className="setup-content">
          <h2 className="setup-title">Connect to Cloud</h2>
          <p className="setup-body">
            ZanPOS uses Supabase as its central database — the authoritative source for all
            your store data, multi-terminal sync, and backups.
          </p>
          <p className="setup-body setup-body-dim">
            Enter your Supabase project URL and service role key. The Personal Access Token
            is only needed to run the one-time schema setup; you can skip it and run it later
            from <strong>Back Office → Sync</strong>.
          </p>

          <label className="field-label">Supabase Project URL *</label>
          <input
            className="field-input"
            placeholder="https://yourproject.supabase.co"
            value={sbUrl}
            onChange={e => { setSbUrl(e.target.value); clearError(); }}
            autoFocus
          />

          <label className="field-label">Service Role Key *</label>
          <input
            className="field-input"
            type="password"
            placeholder="eyJh…"
            value={sbKey}
            onChange={e => { setSbKey(e.target.value); clearError(); }}
          />

          <label className="field-label">Personal Access Token (optional — for one-time schema setup)</label>
          <input
            className="field-input"
            type="password"
            placeholder="sbp_… (leave blank to configure schema later)"
            value={sbPat}
            onChange={e => { setSbPat(e.target.value); clearError(); }}
          />

          {error && <div className="modal-error">{error}</div>}

          <div className="setup-actions">
            <button className="setup-btn-skip" onClick={handleSkipCloud} disabled={sbValidating}>
              Skip for now (7-day grace)
            </button>
            <button className="setup-btn-primary" onClick={handleConnectCloud} disabled={sbValidating}>
              {sbValidating ? "Connecting…" : "Connect & Continue →"}
            </button>
          </div>

          <p className="setup-hint">
            ⚠ Skipping cloud connection means sales are stored locally only. You have 7 days
            before a persistent warning appears on every screen.
          </p>
        </div>
      )}

      {/* ── Step 3: Store info ── */}
      {step === 3 && (
        <div className="setup-content">
          <h2 className="setup-title">Store Information</h2>
          {sbSkipped && (
            <div className="setup-warn-banner">
              ⚠ Cloud not connected — you can add Supabase credentials later in Back Office → Sync.
            </div>
          )}
          <label className="field-label">Store Name *</label>
          <input
            className="field-input"
            type="text"
            placeholder="e.g. My Coffee Shop"
            value={storeName}
            onChange={e => { setStoreName(e.target.value); clearError(); }}
            autoFocus
            maxLength={60}
          />

          <label className="field-label">Currency *</label>
          <select className="bo-select field-input" value={currency} onChange={e => setCurrency(e.target.value)}>
            {CURRENCIES.map(c => <option key={c.code} value={c.code}>{c.label}</option>)}
          </select>

          <label className="field-label">Timezone *</label>
          <select className="bo-select field-input" value={timezone} onChange={e => setTimezone(e.target.value)}>
            {TIMEZONES.map(tz => <option key={tz} value={tz}>{tz}</option>)}
          </select>

          {error && <div className="modal-error">{error}</div>}
          <div className="setup-actions">
            <button className="setup-btn-secondary" onClick={() => { setError(null); setStep(2); }}>← Back</button>
            <button className="setup-btn-primary" onClick={goNext}>Next →</button>
          </div>
        </div>
      )}

      {/* ── Step 4: Contact + receipt ── */}
      {step === 4 && (
        <div className="setup-content">
          <h2 className="setup-title">Contact & Receipt</h2>
          <p className="setup-body">All fields are optional — update anytime in Settings.</p>

          <label className="field-label">Address</label>
          <textarea className="field-input" rows={2} placeholder="123 Main Street, City" value={address} onChange={e => setAddress(e.target.value)} />

          <label className="field-label">Phone Number</label>
          <input className="field-input" type="tel" placeholder="+973 1234 5678" value={phone} onChange={e => setPhone(e.target.value)} />

          <label className="field-label">Tax / VAT Registration Number</label>
          <input className="field-input" type="text" placeholder="Optional" value={taxNumber} onChange={e => setTaxNumber(e.target.value)} />

          <label className="field-label">Receipt Header</label>
          <input className="field-input" type="text" placeholder="e.g. Thank you for visiting!" value={receiptHeader} onChange={e => setReceiptHeader(e.target.value)} />

          <label className="field-label">Receipt Footer</label>
          <input className="field-input" type="text" placeholder="Thank you for your purchase!" value={receiptFooter} onChange={e => setReceiptFooter(e.target.value)} />

          <div className="setup-actions">
            <button className="setup-btn-secondary" onClick={() => { setError(null); setStep(3); }}>← Back</button>
            <button className="setup-btn-primary" onClick={goNext}>Next →</button>
          </div>
        </div>
      )}

      {/* ── Step 5: Owner account ── */}
      {step === 5 && (
        <div className="setup-content">
          <h2 className="setup-title">Owner Account</h2>
          <p className="setup-body">This account has full access. Keep your PIN secure.</p>

          <label className="field-label">Full Name *</label>
          <input className="field-input" type="text" placeholder="e.g. Mohammed Al-Farsi" value={ownerName} onChange={e => { setOwnerName(e.target.value); clearError(); }} autoFocus />

          <label className="field-label">Username *</label>
          <input className="field-input" type="text" placeholder="admin" value={ownerUsername} onChange={e => { setOwnerUsername(e.target.value.toLowerCase().replace(/\s/g, "")); clearError(); }} />

          <label className="field-label">PIN (4–6 digits) *</label>
          <input className="field-input setup-pin-input" type="password" inputMode="numeric" pattern="[0-9]*" placeholder="Enter PIN" maxLength={6} value={ownerPin} onChange={e => { setOwnerPin(e.target.value.replace(/\D/g, "")); clearError(); }} />

          <label className="field-label">Confirm PIN *</label>
          <input className="field-input setup-pin-input" type="password" inputMode="numeric" pattern="[0-9]*" placeholder="Re-enter PIN" maxLength={6} value={ownerPinConfirm} onChange={e => { setOwnerPinConfirm(e.target.value.replace(/\D/g, "")); clearError(); }} />

          {error && <div className="modal-error">{error}</div>}
          <div className="setup-actions">
            <button className="setup-btn-secondary" onClick={() => { setError(null); setStep(4); }}>← Back</button>
            <button className="setup-btn-primary" onClick={goNext}>Review →</button>
          </div>
        </div>
      )}

      {/* ── Step 6: Review & launch ── */}
      {step === 6 && (
        <div className="setup-content setup-done">
          <div className="setup-done-icon">🎉</div>
          <h2 className="setup-title">Ready to go!</h2>

          <div className="setup-review">
            {sbSkipped
              ? <div className="setup-review-row setup-review-warn"><span>Cloud</span><strong>⚠ Not connected (7-day grace)</strong></div>
              : <div className="setup-review-row"><span>Cloud</span><strong>✓ Supabase connected</strong></div>
            }
            <div className="setup-review-row"><span>Store name</span><strong>{storeName}</strong></div>
            <div className="setup-review-row"><span>Currency</span><strong>{currency}</strong></div>
            <div className="setup-review-row"><span>Timezone</span><strong>{timezone}</strong></div>
            {address && <div className="setup-review-row"><span>Address</span><strong>{address}</strong></div>}
            <div className="setup-review-row"><span>Owner account</span><strong>{ownerName} ({ownerUsername})</strong></div>
          </div>

          <p className="setup-body" style={{ marginTop: 16 }}>
            Change any of this later in <strong>Back Office → Settings</strong>.
          </p>

          {error && <div className="modal-error">{error}</div>}
          <div className="setup-actions">
            <button className="setup-btn-secondary" onClick={() => { setError(null); setStep(5); }} disabled={loading}>← Back</button>
            <button className="setup-btn-primary setup-btn-finish" onClick={handleFinish} disabled={loading}>
              {loading ? "Setting up…" : "Launch POS 🚀"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

// ─── Path: Join Existing Store ────────────────────────────────────────────────
// Steps: cloud-creds → device-info → pulling → done

type JoinStep = "creds" | "device" | "joining" | "done";

function JoinStoreWizard({ onComplete }: { onComplete: (cfg: AppConfig) => void }) {
  const [step, setStep]       = useState<JoinStep>("creds");
  const [sbUrl, setSbUrl]     = useState("");
  const [sbKey, setSbKey]     = useState("");
  const [deviceName, setDeviceName] = useState("POS Terminal 2");
  const [deviceCode, setDeviceCode] = useState("POS02");
  const [error, setError]     = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const clearError = () => setError(null);

  const handleValidateCreds = () => {
    if (!sbUrl.trim()) { setError("Supabase URL is required"); return; }
    if (!sbKey.trim()) { setError("Service role key is required"); return; }
    setError(null);
    setStep("device");
  };

  const handleJoin = async () => {
    if (!deviceName.trim()) { setError("Device name is required"); return; }
    if (!deviceCode.trim()) { setError("Device code is required"); return; }
    setLoading(true);
    setError(null);
    setStep("joining");
    try {
      const cfg = await setupJoinStore({
        supabase_url: sbUrl.trim(),
        supabase_key: sbKey.trim(),
        device_name:  deviceName.trim(),
        device_code:  deviceCode.trim().toUpperCase(),
      });
      onComplete(cfg);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Could not join store — check credentials and network");
      setStep("device");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="setup-panel">
      {step === "creds" && (
        <div className="setup-content">
          <h2 className="setup-title">Join Existing Store</h2>
          <p className="setup-body">
            Enter the same Supabase project credentials used when the store was first set up.
            This terminal will pull the store's catalog, users, and configuration automatically.
          </p>

          <label className="field-label">Supabase Project URL *</label>
          <input
            className="field-input"
            placeholder="https://yourproject.supabase.co"
            value={sbUrl}
            onChange={e => { setSbUrl(e.target.value); clearError(); }}
            autoFocus
          />

          <label className="field-label">Service Role Key *</label>
          <input
            className="field-input"
            type="password"
            placeholder="eyJh…"
            value={sbKey}
            onChange={e => { setSbKey(e.target.value); clearError(); }}
          />

          {error && <div className="modal-error">{error}</div>}
          <div className="setup-actions">
            <button className="setup-btn-primary" onClick={handleValidateCreds}>
              Next →
            </button>
          </div>
        </div>
      )}

      {(step === "device" || step === "joining") && (
        <div className="setup-content">
          <h2 className="setup-title">Register This Terminal</h2>
          <p className="setup-body">
            Give this terminal a unique name and short code. The code appears on receipts.
          </p>

          <label className="field-label">Terminal Name *</label>
          <input
            className="field-input"
            placeholder="e.g. POS Terminal 2"
            value={deviceName}
            onChange={e => { setDeviceName(e.target.value); clearError(); }}
            autoFocus
          />

          <label className="field-label">Terminal Code * (short, uppercase)</label>
          <input
            className="field-input"
            placeholder="e.g. POS02"
            value={deviceCode}
            onChange={e => { setDeviceCode(e.target.value.toUpperCase().replace(/\s/g, "")); clearError(); }}
            maxLength={10}
          />

          {error && <div className="modal-error">{error}</div>}
          <div className="setup-actions">
            <button className="setup-btn-secondary" onClick={() => { setError(null); setStep("creds"); }} disabled={loading}>← Back</button>
            <button className="setup-btn-primary" onClick={handleJoin} disabled={loading}>
              {loading ? "Joining…" : "Join Store 🔗"}
            </button>
          </div>
        </div>
      )}

      {step === "joining" && (
        <div className="setup-content" style={{ textAlign: "center", padding: "40px 0" }}>
          <div className="app-splash-spinner" style={{ margin: "0 auto 20px" }} />
          <p>Connecting to store and downloading catalog…</p>
        </div>
      )}
    </div>
  );
}

// ─── Root SetupWizard — path selector ────────────────────────────────────────

type SetupPath = null | "new" | "join";

export default function SetupWizard({ onComplete }: Props) {
  const [path, setPath] = useState<SetupPath>(null);

  if (path === "new")  return <div className="setup-screen"><NewStoreWizard onComplete={onComplete} /></div>;
  if (path === "join") return <div className="setup-screen"><JoinStoreWizard onComplete={onComplete} /></div>;

  // ── Path selector ──────────────────────────────────────────────────────────
  return (
    <div className="setup-screen">
      <div className="setup-panel">
        <div className="setup-content">
          <div className="setup-logo">ZAN<span>POS</span></div>
          <h1 className="setup-title">Welcome to ZanPOS</h1>
          <p className="setup-body">
            ZanPOS is a cloud-authoritative point-of-sale system. Your central store data
            lives in Supabase; this terminal uses a local cache for fast, offline-resilient
            operation and syncs automatically.
          </p>
          <p className="setup-body">What would you like to do?</p>

          <div className="setup-path-cards">
            <button className="setup-path-card" onClick={() => setPath("new")}>
              <span className="setup-path-icon">🏪</span>
              <span className="setup-path-title">New Store</span>
              <span className="setup-path-desc">
                Set up a brand-new store. You'll configure your store details,
                connect Supabase, and create your owner account.
              </span>
            </button>

            <button className="setup-path-card" onClick={() => setPath("join")}>
              <span className="setup-path-icon">🔗</span>
              <span className="setup-path-title">Join Existing Store</span>
              <span className="setup-path-desc">
                Add this terminal to a store that's already running. Enter your
                Supabase credentials and this device will sync the store's catalog
                and users automatically.
              </span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
