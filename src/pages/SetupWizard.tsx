import { useState } from "react";
import type { AppConfig } from "../types";
import { setupWizardComplete } from "../tauri/commands";

interface Props {
  onComplete: (config: AppConfig) => void;
}

type Step = 1 | 2 | 3 | 4 | 5;

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

const STEP_LABELS = [
  "Welcome",
  "Store Info",
  "Contact & Receipt",
  "Admin Account",
  "Done",
];

export default function SetupWizard({ onComplete }: Props) {
  const [step, setStep] = useState<Step>(1);

  // Step 2 — store info
  const [storeName, setStoreName]   = useState("");
  const [currency, setCurrency]     = useState("BHD");
  const [timezone, setTimezone]     = useState("Asia/Bahrain");

  // Step 3 — contact + receipt
  const [address, setAddress]               = useState("");
  const [phone, setPhone]                   = useState("");
  const [taxNumber, setTaxNumber]           = useState("");
  const [receiptHeader, setReceiptHeader]   = useState("");
  const [receiptFooter, setReceiptFooter]   = useState("Thank you for your purchase!");

  // Step 4 — admin account
  const [ownerName, setOwnerName]         = useState("");
  const [ownerUsername, setOwnerUsername] = useState("admin");
  const [ownerPin, setOwnerPin]           = useState("");
  const [ownerPinConfirm, setOwnerPinConfirm] = useState("");

  const [error, setError]     = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const clearError = () => setError(null);

  const goNext = () => {
    setError(null);
    if (step === 2) {
      if (!storeName.trim()) { setError("Store name is required"); return; }
    }
    if (step === 4) {
      if (!ownerName.trim())     { setError("Owner name is required"); return; }
      if (!ownerUsername.trim()) { setError("Username is required"); return; }
      if (ownerPin.length < 4)   { setError("PIN must be at least 4 digits"); return; }
      if (ownerPin !== ownerPinConfirm) { setError("PINs do not match"); return; }
    }
    setStep(s => Math.min(5, s + 1) as Step);
  };

  const handleFinish = async () => {
    setLoading(true);
    setError(null);
    try {
      const config = await setupWizardComplete({
        store_name:      storeName.trim(),
        store_address:   address.trim() || undefined,
        store_phone:     phone.trim() || undefined,
        receipt_header:  receiptHeader.trim() || undefined,
        receipt_footer:  receiptFooter.trim() || undefined,
        tax_number:      taxNumber.trim() || undefined,
        currency,
        timezone,
        owner_display_name: ownerName.trim(),
        owner_username:     ownerUsername.trim(),
        owner_pin:          ownerPin,
      });
      onComplete(config);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Setup failed — please try again");
      setStep(4);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="setup-screen">
      {/* Progress bar */}
      <div className="setup-progress">
        {STEP_LABELS.map((label, i) => (
          <div key={i} className={`setup-step-dot ${i + 1 <= step ? "setup-step-done" : ""} ${i + 1 === step ? "setup-step-active" : ""}`}>
            <div className="setup-dot-circle">{i + 1 < step ? "✓" : i + 1}</div>
            <div className="setup-dot-label">{label}</div>
          </div>
        ))}
        <div className="setup-progress-line" style={{ width: `${(step - 1) * 25}%` }} />
      </div>

      <div className="setup-panel">
        {/* ── Step 1: Welcome ── */}
        {step === 1 && (
          <div className="setup-content">
            <div className="setup-logo">ZAN<span>POS</span></div>
            <h1 className="setup-title">Welcome to ZanPOS</h1>
            <p className="setup-body">
              Let's get your store set up in a few quick steps.
              You'll configure your store name, currency, and create
              your first owner account.
            </p>
            <p className="setup-body">
              This only takes about 2 minutes.
            </p>
            <div className="setup-actions">
              <button className="setup-btn-primary" onClick={goNext}>
                Get Started →
              </button>
            </div>
          </div>
        )}

        {/* ── Step 2: Store info ── */}
        {step === 2 && (
          <div className="setup-content">
            <h2 className="setup-title">Store Information</h2>
            <p className="setup-body">What's the name of your business?</p>

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
              {CURRENCIES.map(c => (
                <option key={c.code} value={c.code}>{c.label}</option>
              ))}
            </select>

            <label className="field-label">Timezone *</label>
            <select className="bo-select field-input" value={timezone} onChange={e => setTimezone(e.target.value)}>
              {TIMEZONES.map(tz => (
                <option key={tz} value={tz}>{tz}</option>
              ))}
            </select>

            {error && <div className="modal-error">{error}</div>}
            <div className="setup-actions">
              <button className="setup-btn-secondary" onClick={() => setStep(1)}>← Back</button>
              <button className="setup-btn-primary" onClick={goNext}>Next →</button>
            </div>
          </div>
        )}

        {/* ── Step 3: Contact + receipt ── */}
        {step === 3 && (
          <div className="setup-content">
            <h2 className="setup-title">Contact & Receipt</h2>
            <p className="setup-body">
              This information appears on your customer receipts.
              All fields are optional — you can update them later in Settings.
            </p>

            <label className="field-label">Address</label>
            <textarea
              className="field-input"
              rows={2}
              placeholder="123 Main Street, City"
              value={address}
              onChange={e => setAddress(e.target.value)}
            />

            <label className="field-label">Phone Number</label>
            <input className="field-input" type="tel" placeholder="+973 1234 5678" value={phone} onChange={e => setPhone(e.target.value)} />

            <label className="field-label">Tax / VAT Registration Number</label>
            <input className="field-input" type="text" placeholder="Optional" value={taxNumber} onChange={e => setTaxNumber(e.target.value)} />

            <label className="field-label">Receipt Header (printed above items)</label>
            <input className="field-input" type="text" placeholder="e.g. Thank you for visiting!" value={receiptHeader} onChange={e => setReceiptHeader(e.target.value)} />

            <label className="field-label">Receipt Footer (printed below total)</label>
            <input className="field-input" type="text" placeholder="Thank you for your purchase!" value={receiptFooter} onChange={e => setReceiptFooter(e.target.value)} />

            <div className="setup-actions">
              <button className="setup-btn-secondary" onClick={() => setStep(2)}>← Back</button>
              <button className="setup-btn-primary" onClick={goNext}>Next →</button>
            </div>
          </div>
        )}

        {/* ── Step 4: Admin account ── */}
        {step === 4 && (
          <div className="setup-content">
            <h2 className="setup-title">Owner Account</h2>
            <p className="setup-body">
              Create your owner/admin account. This account has full access
              to all settings. Keep your PIN secure — it can be changed later.
            </p>

            <label className="field-label">Full Name *</label>
            <input
              className="field-input"
              type="text"
              placeholder="e.g. Mohammed Al-Farsi"
              value={ownerName}
              onChange={e => { setOwnerName(e.target.value); clearError(); }}
              autoFocus
            />

            <label className="field-label">Username *</label>
            <input
              className="field-input"
              type="text"
              placeholder="admin"
              value={ownerUsername}
              onChange={e => { setOwnerUsername(e.target.value.toLowerCase().replace(/\s/g, "")); clearError(); }}
            />

            <label className="field-label">PIN (4-6 digits) *</label>
            <input
              className="field-input setup-pin-input"
              type="password"
              inputMode="numeric"
              pattern="[0-9]*"
              placeholder="Enter PIN"
              maxLength={6}
              value={ownerPin}
              onChange={e => { setOwnerPin(e.target.value.replace(/\D/g, "")); clearError(); }}
            />

            <label className="field-label">Confirm PIN *</label>
            <input
              className="field-input setup-pin-input"
              type="password"
              inputMode="numeric"
              pattern="[0-9]*"
              placeholder="Re-enter PIN"
              maxLength={6}
              value={ownerPinConfirm}
              onChange={e => { setOwnerPinConfirm(e.target.value.replace(/\D/g, "")); clearError(); }}
            />

            {error && <div className="modal-error">{error}</div>}
            <div className="setup-actions">
              <button className="setup-btn-secondary" onClick={() => setStep(3)}>← Back</button>
              <button className="setup-btn-primary" onClick={goNext}>Review →</button>
            </div>
          </div>
        )}

        {/* ── Step 5: Done ── */}
        {step === 5 && (
          <div className="setup-content setup-done">
            <div className="setup-done-icon">🎉</div>
            <h2 className="setup-title">Ready to go!</h2>

            <div className="setup-review">
              <div className="setup-review-row">
                <span>Store name</span>
                <strong>{storeName}</strong>
              </div>
              <div className="setup-review-row">
                <span>Currency</span>
                <strong>{currency}</strong>
              </div>
              <div className="setup-review-row">
                <span>Timezone</span>
                <strong>{timezone}</strong>
              </div>
              {address && (
                <div className="setup-review-row">
                  <span>Address</span>
                  <strong>{address}</strong>
                </div>
              )}
              <div className="setup-review-row">
                <span>Owner account</span>
                <strong>{ownerName} ({ownerUsername})</strong>
              </div>
            </div>

            <p className="setup-body" style={{ marginTop: 16 }}>
              You can change any of this later in <strong>Back Office → Settings</strong>.
            </p>

            {error && <div className="modal-error">{error}</div>}
            <div className="setup-actions">
              <button className="setup-btn-secondary" onClick={() => setStep(4)} disabled={loading}>
                ← Back
              </button>
              <button className="setup-btn-primary setup-btn-finish" onClick={handleFinish} disabled={loading}>
                {loading ? "Setting up…" : "Launch POS 🚀"}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
