import { useState } from "react";

export interface IdentityDraft {
  storeName: string;
  phone: string;
  currency: string;
  language: "en" | "ar" | "en-ar";
  vatChoice: "standard" | "none" | "custom";
  customRatePercent: string;
}

export const DEFAULT_IDENTITY_DRAFT: IdentityDraft = {
  storeName: "",
  phone: "",
  currency: "BHD",
  language: "en-ar",
  vatChoice: "standard",
  customRatePercent: "10",
};

interface Props {
  draft: IdentityDraft;
  onNext: (draft: IdentityDraft) => void;
}

// Mirrors src-tauri/src/commands/setup_commands.rs::currency_exponent — display
// only, the server is the source of truth for the actual stored exponent.
const THREE_DECIMAL_CURRENCIES = new Set(["BHD", "KWD", "OMR"]);

const CURRENCIES = [
  { code: "BHD", label: "BHD — Bahraini Dinar" },
  { code: "SAR", label: "SAR — Saudi Riyal" },
  { code: "AED", label: "AED — UAE Dirham" },
  { code: "KWD", label: "KWD — Kuwaiti Dinar" },
  { code: "OMR", label: "OMR — Omani Rial" },
  { code: "QAR", label: "QAR — Qatari Riyal" },
  { code: "EGP", label: "EGP — Egyptian Pound" },
  { code: "USD", label: "USD — US Dollar" },
];

const LANGUAGES: Array<{ value: IdentityDraft["language"]; label: string }> = [
  { value: "en-ar", label: "English + العربية" },
  { value: "en", label: "English" },
  { value: "ar", label: "العربية" },
];

/** Step 1 — the only step that can't be skipped: store name, phone, currency,
 * customer language, and a starting VAT rate. Nothing is saved to the database
 * yet — this is bundled with the owner PIN (step 2) into one atomic write. */
export default function StepIdentity({ draft, onNext }: Props) {
  const [form, setForm] = useState<IdentityDraft>(draft);
  const [error, setError] = useState<string | null>(null);

  const decimals = THREE_DECIMAL_CURRENCIES.has(form.currency) ? 3 : 2;

  const handleNext = () => {
    if (!form.storeName.trim()) { setError("Store name is required"); return; }
    if (form.vatChoice === "custom") {
      const pct = parseFloat(form.customRatePercent);
      if (Number.isNaN(pct) || pct < 0 || pct > 100) {
        setError("Enter a valid VAT percentage (0–100)");
        return;
      }
    }
    setError(null);
    onNext(form);
  };

  return (
    <div className="setup-content">
      <h2 className="setup-title">Store Identity</h2>
      <p className="setup-body">Let's get the basics right — you can change any of this later in Settings.</p>

      <label className="field-label">Store Name *</label>
      <input
        className="field-input"
        type="text"
        placeholder="e.g. My Coffee Shop"
        value={form.storeName}
        onChange={e => { setForm(f => ({ ...f, storeName: e.target.value })); setError(null); }}
        // eslint-disable-next-line jsx-a11y/no-autofocus
        autoFocus
        maxLength={60}
      />

      <label className="field-label">Phone Number</label>
      <input
        className="field-input"
        type="tel"
        placeholder="+973 1234 5678"
        value={form.phone}
        onChange={e => setForm(f => ({ ...f, phone: e.target.value }))}
      />

      <label className="field-label">Currency *</label>
      <select
        className="bo-select field-input"
        value={form.currency}
        onChange={e => setForm(f => ({ ...f, currency: e.target.value }))}
      >
        {CURRENCIES.map(c => <option key={c.code} value={c.code}>{c.label}</option>)}
      </select>
      <p className="setup-field-hint">Prices will be shown with {decimals} decimal places.</p>

      <label className="field-label">Customer Language</label>
      <select
        className="bo-select field-input"
        value={form.language}
        onChange={e => setForm(f => ({ ...f, language: e.target.value as IdentityDraft["language"] }))}
      >
        {LANGUAGES.map(l => <option key={l.value} value={l.value}>{l.label}</option>)}
      </select>
      <p className="setup-field-hint">Used for the public storefront and WhatsApp ordering.</p>

      <label className="field-label">VAT</label>
      <div className="setup-vat-choices">
        <label className="bo-checkbox-label">
          <input type="radio" name="vat-choice" checked={form.vatChoice === "standard"}
            onChange={() => setForm(f => ({ ...f, vatChoice: "standard" }))} />
          {" "}Standard 10% VAT
        </label>
        <label className="bo-checkbox-label">
          <input type="radio" name="vat-choice" checked={form.vatChoice === "custom"}
            onChange={() => setForm(f => ({ ...f, vatChoice: "custom" }))} />
          {" "}Custom rate:
          {" "}
          <input
            className="field-input setup-vat-rate-input"
            type="number" min={0} max={100} step="0.01"
            disabled={form.vatChoice !== "custom"}
            value={form.customRatePercent}
            onChange={e => setForm(f => ({ ...f, vatChoice: "custom", customRatePercent: e.target.value }))}
          />
          %
        </label>
        <label className="bo-checkbox-label">
          <input type="radio" name="vat-choice" checked={form.vatChoice === "none"}
            onChange={() => setForm(f => ({ ...f, vatChoice: "none" }))} />
          {" "}Not VAT registered
        </label>
      </div>

      {error && <div className="modal-error">{error}</div>}
      <div className="setup-actions">
        <button className="setup-btn-primary" onClick={handleNext}>Next <span className="icon-directional" aria-hidden="true">→</span></button>
      </div>
    </div>
  );
}
