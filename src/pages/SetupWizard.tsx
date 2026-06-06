import { useRef, useState } from "react";
import type { AppConfig } from "../types";
import {
  setupWizardComplete,
  setupJoinStore,
  adminSetupSupabase,
  setupSaveBenefitNumber,
  adminBulkImportCategories,
  adminBulkImportProducts,
  adminListUsersAll,
} from "../tauri/commands";
import type { BulkCategoryRow, BulkProductRow, BulkImportResult } from "../tauri/commands";
import WhatsAppQRModal from "../components/WhatsAppQRModal";

interface Props {
  onComplete: (config: AppConfig) => void;
  onMigrate?: (config: AppConfig) => void;
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

type NewStep = 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8;

// ── CSV helpers (shared with step 8) ─────────────────────────────────────────

function parseCSVWizard(text: string): string[][] {
  const lines = text.replace(/\r\n/g, "\n").replace(/\r/g, "\n").split("\n");
  return lines.filter(l => l.trim() !== "").map(line => {
    const cols: string[] = [];
    let cur = ""; let inQuote = false;
    for (let i = 0; i < line.length; i++) {
      const ch = line[i];
      if (ch === '"') { if (inQuote && line[i + 1] === '"') { cur += '"'; i++; } else { inQuote = !inQuote; } }
      else if (ch === "," && !inQuote) { cols.push(cur); cur = ""; }
      else { cur += ch; }
    }
    cols.push(cur);
    return cols.map(c => c.trim());
  });
}

function csvToCategoryRows(rows: string[][]): BulkCategoryRow[] {
  if (rows.length < 2) return [];
  const h = rows[0].map(x => x.toLowerCase());
  const ni = h.findIndex(x => x === "name");
  const oi = h.findIndex(x => x.includes("sort") || x.includes("order"));
  if (ni < 0) return [];
  return rows.slice(1).map(r => ({ name: r[ni] ?? "", sort_order: oi >= 0 && r[oi] ? parseInt(r[oi]) || undefined : undefined }));
}

function csvToProductRows(rows: string[][]): BulkProductRow[] {
  if (rows.length < 2) return [];
  const h = rows[0].map(x => x.toLowerCase().replace(/[^a-z_]/g, "_"));
  const col = (terms: string[]) => terms.reduce((a, t) => a >= 0 ? a : h.findIndex(x => x.includes(t)), -1);
  const ni = col(["name"]); const ci = col(["category"]); const pi = col(["price"]);
  const si = col(["sku"]);
  // `barcodes` (plural, pipe-separated) takes priority over single `barcode`
  const bsi = h.findIndex(x => x === "barcodes");
  const bi = bsi >= 0 ? -1 : col(["barcode"]);
  const ti = col(["track"]); const xi = col(["tax"]);
  if (ni < 0 || ci < 0 || pi < 0) return [];
  return rows.slice(1).map(r => ({
    name: r[ni] ?? "", category_name: r[ci] ?? "", price: r[pi] ?? "0",
    sku: si >= 0 ? (r[si] || undefined) : undefined,
    barcode: bi >= 0 ? (r[bi] || undefined) : undefined,
    barcodes: bsi >= 0 ? (r[bsi] || undefined) : undefined,
    track_inventory: ti >= 0 ? !["false","0","no"].includes((r[ti] ?? "").toLowerCase()) : undefined,
    tax_rule_name: xi >= 0 ? (r[xi] || undefined) : undefined,
  }));
}

const CAT_TEMPLATE = "name,sort_order\nDrinks,1\nFood,2\nMisc,3\n";
const PROD_TEMPLATE = "name,category_name,price,sku,barcodes,track_inventory,tax_rule_name\nCoca-Cola 330ml,Drinks,0.400,COLA-330,5449000000996,true,VAT 10%\nPepsi 330ml,Drinks,0.350,PEPS-330,1234567890|9876543210,true,VAT 10%\nWater 500ml,Drinks,0.250,WATR-500,,true,Zero Rate\n";

function downloadWizardTemplate(mode: "categories" | "products") {
  const content = mode === "categories" ? CAT_TEMPLATE : PROD_TEMPLATE;
  const blob = new Blob([content], { type: "text/csv" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url; a.download = mode === "categories" ? "categories_template.csv" : "products_template.csv";
  a.click(); URL.revokeObjectURL(url);
}

// ── Inline CSV section used on step 8 ────────────────────────────────────────

interface CsvSectionProps {
  title: string;
  mode: "categories" | "products";
  userId: string;
  onImported: () => void;
}

function CsvSection({ title, mode, userId, onImported }: CsvSectionProps) {
  const [parsed, setParsed]     = useState<string[][]>([]);
  const [result, setResult]     = useState<BulkImportResult | null>(null);
  const [importing, setImporting] = useState(false);
  const [err, setErr]           = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  const rows = mode === "categories" ? csvToCategoryRows(parsed) : csvToProductRows(parsed);

  function handleFile(file: File) {
    const reader = new FileReader();
    reader.onload = e => {
      const text = (e.target?.result as string) ?? "";
      setParsed(parseCSVWizard(text));
      setResult(null); setErr(null);
    };
    reader.readAsText(file);
  }

  async function runImport() {
    setImporting(true); setErr(null);
    try {
      const res = mode === "categories"
        ? await adminBulkImportCategories(rows as BulkCategoryRow[], userId)
        : await adminBulkImportProducts(rows as BulkProductRow[], userId);
      setResult(res);
      if (res.inserted > 0) onImported();
    } catch (e: unknown) {
      setErr(typeof e === "string" ? e : "Import failed");
    } finally {
      setImporting(false);
    }
  }

  return (
    <div className="wiz-csv-section">
      <div className="wiz-csv-section-header">
        <span className="wiz-csv-section-title">{title}</span>
        <button className="setup-btn-skip wiz-template-btn" type="button" onClick={() => downloadWizardTemplate(mode)}>
          ⬇ Template
        </button>
      </div>

      {result ? (
        <div className="wiz-csv-result">
          <span className="wiz-csv-ok">✅ {result.inserted} inserted</span>
          {result.skipped > 0 && <span className="wiz-csv-skip">⏭ {result.skipped} skipped</span>}
          {result.errors.length > 0 && <span className="wiz-csv-err">❌ {result.errors.length} errors</span>}
          <button className="setup-btn-skip" type="button" onClick={() => { setParsed([]); setResult(null); }}>Import another</button>
        </div>
      ) : parsed.length === 0 ? (
        <div
          className="wiz-dropzone"
          onDrop={e => { e.preventDefault(); const f = e.dataTransfer.files[0]; if (f) handleFile(f); }}
          onDragOver={e => e.preventDefault()}
          onClick={() => fileRef.current?.click()}
        >
          <span className="wiz-dropzone-icon">📂</span>
          <span className="wiz-dropzone-text">Drop CSV here or click to browse</span>
        </div>
      ) : (
        <div className="wiz-csv-preview">
          <div className="wiz-csv-preview-count">{rows.length} row{rows.length !== 1 ? "s" : ""} ready</div>
          <div className="wiz-csv-preview-scroll">
            <table className="wiz-csv-table">
              <thead>
                <tr>
                  {mode === "categories"
                    ? <><th>#</th><th>Name</th><th>Sort</th></>
                    : <><th>#</th><th>Name</th><th>Category</th><th>Price</th><th>SKU</th></>}
                </tr>
              </thead>
              <tbody>
                {(rows as (BulkCategoryRow | BulkProductRow)[]).slice(0, 8).map((r, i) => (
                  <tr key={i}>
                    <td className="wiz-cell-num">{i + 1}</td>
                    <td>{r.name || <em>empty</em>}</td>
                    {mode === "categories"
                      ? <td>{(r as BulkCategoryRow).sort_order ?? "–"}</td>
                      : <>
                          <td>{(r as BulkProductRow).category_name}</td>
                          <td>{(r as BulkProductRow).price}</td>
                          <td className="wiz-cell-dim">{(r as BulkProductRow).sku || "–"}</td>
                        </>}
                  </tr>
                ))}
              </tbody>
            </table>
            {rows.length > 8 && <div className="wiz-csv-more">…and {rows.length - 8} more</div>}
          </div>
          {err && <div className="modal-error">{err}</div>}
          <div className="wiz-csv-preview-actions">
            <button className="setup-btn-skip" type="button" onClick={() => { setParsed([]); setErr(null); }}>← Cancel</button>
            <button className="setup-btn-primary" type="button" onClick={runImport} disabled={importing || rows.length === 0}>
              {importing ? "Importing…" : `Import ${rows.length} ${mode}`}
            </button>
          </div>
        </div>
      )}
      <input ref={fileRef} type="file" accept=".csv,text/csv" style={{ display: "none" }}
        onChange={e => { const f = e.target.files?.[0]; if (f) handleFile(f); e.target.value = ""; }} />
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────

function NewStoreWizard({ onComplete, onMigrate }: { onComplete: (cfg: AppConfig) => void; onMigrate: (cfg: AppConfig) => void }) {
  const [step, setStep] = useState<NewStep>(2); // skip path-selector inside sub-wizard
  const [completedConfig, setCompletedConfig] = useState<AppConfig | null>(null); // set when step 8 is reached
  const [ownerUserId, setOwnerUserId] = useState(""); // resolved after setup completes

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
  const [crNumber, setCrNumber]           = useState("");
  const [receiptHeader, setReceiptHeader] = useState("");
  const [receiptFooter, setReceiptFooter] = useState("Thank you for your purchase!");

  // Step 5 — Payments & WhatsApp
  const [benefitNumber, setBenefitNumber] = useState("");
  const [showWaQR, setShowWaQR]           = useState(false);

  // Step 6 — admin account
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
    if (!sbPat.trim()) { setError("Personal Access Token is required for new store setup — it runs the one-time schema migration that enables multi-terminal sync"); return; }
    setSbValidating(true);
    setError(null);
    try {
      await adminSetupSupabase(sbUrl.trim(), sbKey.trim(), sbPat.trim());
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
    if (step === 6) {
      if (!ownerName.trim())     { setError("Owner name is required"); return; }
      if (!ownerUsername.trim()) { setError("Username is required"); return; }
      if (ownerPin.length < 4)   { setError("PIN must be at least 4 digits"); return; }
      if (ownerPin !== ownerPinConfirm) { setError("PINs do not match"); return; }
    }
    setStep(s => (s + 1) as NewStep);
  };

  // Shared setup completion — returns cfg or null on error
  const doSetupComplete = async (): Promise<AppConfig | null> => {
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
        cr_number:          crNumber.trim()        || undefined,
        currency,
        timezone,
        owner_display_name: ownerName.trim(),
        owner_username:     ownerUsername.trim(),
        owner_pin:          ownerPin,
      });
      if (benefitNumber.trim()) {
        try { await setupSaveBenefitNumber(benefitNumber.trim()); } catch { /* ignore */ }
      }
      return cfg;
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Setup failed — please try again");
      setStep(6);
      return null;
    } finally {
      setLoading(false);
    }
  };

  const handleFinish = async () => {
    const cfg = await doSetupComplete();
    if (cfg) onComplete(cfg);
  };

  const handleGoImportCSV = async () => {
    const cfg = await doSetupComplete();
    if (!cfg) return;
    setCompletedConfig(cfg);
    // Resolve the newly-created owner user_id so CsvSection can call RBAC-gated commands.
    // Match by username first; fall back to the first ACTIVE owner-role user (never a
    // deactivated row) so CSV import always runs as a valid owner.
    try {
      const users = await adminListUsersAll("");
      const owner =
        users.find(u => u.username === ownerUsername.trim() && u.is_active) ??
        users.find(u => u.is_active && u.role_name === "owner") ??
        users.find(u => u.is_active);
      setOwnerUserId(owner?.user_id ?? "");
    } catch { /* non-critical — imports will fail gracefully if blank */ }
    setStep(8);
  };

  // ── Render ─────────────────────────────────────────────────────────────────

  const LABELS = ["Cloud", "Store Info", "Contact", "Payments", "Owner", "Review"];
  const stepIdx = Math.min(step - 2, LABELS.length - 1); // 0-based for progress bar (cap at Review for step 8)

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
            Enter your Supabase project URL, service role key, and Personal Access Token.
            The PAT runs the one-time schema setup on your Supabase project. It is required
            for new store setup — without it, multi-terminal sync will not work.
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

          <label className="field-label">Personal Access Token *</label>
          <input
            className="field-input"
            type="password"
            placeholder="sbp_…"
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

          <label className="field-label">Commercial Register Number (CR No)</label>
          <input className="field-input" type="text" placeholder="Optional" value={crNumber} onChange={e => setCrNumber(e.target.value)} />

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

      {/* ── Step 5: Payments & WhatsApp ── */}
      {step === 5 && (
        <div className="setup-content">
          <h2 className="setup-title">Payments & WhatsApp</h2>
          <p className="setup-body">
            Set your BenefitPay number so it appears in delivery WhatsApp messages,
            and optionally connect WhatsApp now.
          </p>

          <label className="field-label">BenefitPay Number <span className="setup-required">*</span></label>
          <p className="setup-field-hint">Customers send delivery payments to this number via BenefitPay.</p>
          <input
            className="field-input"
            type="text"
            placeholder="e.g. 33050666"
            value={benefitNumber}
            onChange={e => { setBenefitNumber(e.target.value); clearError(); }}
            maxLength={20}
          />

          <div className="setup-wa-section">
            <div className="setup-wa-title">📱 Connect WhatsApp (optional)</div>
            <p className="setup-field-hint">
              ZANPOS will automatically send order confirmations to customers via WhatsApp.
              You can skip this and connect later from Back Office → Settings.
            </p>
            <button
              className="setup-btn-secondary"
              type="button"
              onClick={() => setShowWaQR(true)}
            >
              Connect WhatsApp (Scan QR)
            </button>
          </div>

          {error && <div className="modal-error">{error}</div>}

          <div className="setup-actions">
            <button className="setup-btn-secondary" onClick={() => { setError(null); setStep(4); }}>← Back</button>
            <button
              className="setup-btn-primary"
              onClick={() => {
                if (!benefitNumber.trim()) { setError("BenefitPay number is required"); return; }
                goNext();
              }}
            >
              Next →
            </button>
          </div>

          <p className="setup-hint">You can update all these settings anytime in Back Office → Settings.</p>

          {showWaQR && (
            <WhatsAppQRModal
              onClose={() => setShowWaQR(false)}
            />
          )}
        </div>
      )}

      {/* ── Step 6: Owner account ── */}
      {step === 6 && (
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
            <button className="setup-btn-secondary" onClick={() => { setError(null); setStep(5); }}>← Back</button>
            <button className="setup-btn-primary" onClick={goNext}>Review →</button>
          </div>
        </div>
      )}

      {/* ── Step 7: Review & launch ── */}
      {step === 7 && (
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

          <div className="setup-actions setup-actions-back">
            <button className="setup-btn-secondary" onClick={() => { setError(null); setStep(6); }} disabled={loading}>← Back</button>
          </div>

          <div className="setup-launch-options">
            {/* Option 1 — Go straight to POS */}
            <div className="setup-launch-card">
              <div className="setup-launch-card-icon">🚀</div>
              <div className="setup-launch-card-body">
                <div className="setup-launch-card-title">Start Fresh</div>
                <div className="setup-launch-card-desc">Launch the POS right now. Add products and categories manually from Back Office.</div>
              </div>
              <button className="setup-btn-primary setup-btn-finish" onClick={handleFinish} disabled={loading}>
                {loading ? "Setting up…" : "Launch POS"}
              </button>
            </div>

            {/* Option 2 — Upload CSV */}
            <div className="setup-launch-card setup-launch-card-highlight">
              <div className="setup-launch-card-icon">📊</div>
              <div className="setup-launch-card-body">
                <div className="setup-launch-card-title">Import from CSV</div>
                <div className="setup-launch-card-desc">Upload your categories and products now using a spreadsheet. Quick setup in minutes.</div>
              </div>
              <button className="setup-btn-primary" onClick={handleGoImportCSV} disabled={loading}>
                {loading ? "Setting up…" : "Upload CSV →"}
              </button>
            </div>

            {/* Option 3 — AI migration agent */}
            <div className="setup-launch-card">
              <div className="setup-launch-card-icon">🤖</div>
              <div className="setup-launch-card-body">
                <div className="setup-launch-card-title">Migrate from Existing POS</div>
                <div className="setup-launch-card-desc">AI-guided import from SQL Server, SQLite, Access, MySQL, or any database file.</div>
              </div>
              <button className="setup-btn-migrate-inline" onClick={async () => {
                const cfg = await doSetupComplete();
                if (cfg) onMigrate(cfg);
              }} disabled={loading}>
                Import with AI →
              </button>
            </div>
          </div>
        </div>
      )}

      {/* ── Step 8: Import Catalog from CSV ── */}
      {step === 8 && completedConfig && (
        <div className="setup-content setup-import-catalog">
          <div className="setup-import-header">
            <div className="setup-import-header-text">
              <h2 className="setup-title" style={{ marginBottom: 4 }}>Import Your Catalog</h2>
              <p className="setup-body" style={{ margin: 0 }}>
                Upload categories first, then products. Both are optional — skip any section and add data manually later in Back Office.
              </p>
            </div>
          </div>

          <CsvSection
            title="1. Categories"
            mode="categories"
            userId={ownerUserId}
            onImported={() => {}}
          />

          <CsvSection
            title="2. Products"
            mode="products"
            userId={ownerUserId}
            onImported={() => {}}
          />

          <div className="setup-import-footer">
            <p className="setup-hint">You can always add more products in <strong>Back Office → Products</strong> anytime.</p>
            <button className="setup-btn-primary setup-btn-finish" onClick={() => onComplete(completedConfig)}>
              Launch POS 🚀
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
      // Clean up raw Supabase JSON errors for the cashier
      const raw = typeof e === "string" ? e : String(e);
      const friendlyMsg =
        raw.includes("No active store") || raw.includes("pull_branch") || raw.includes("branches")
          ? "No store found in this Supabase project. Make sure Device 1 has been set up and connected to the internet at least once."
          : raw.includes("UNAUTHORIZED") || raw.includes("FORBIDDEN") || raw.includes("Invalid")
            ? "Invalid Supabase credentials — check the URL and service role key."
            : raw.includes("connect") || raw.includes("network") || raw.includes("NETWORK")
              ? "Could not reach Supabase — check your internet connection."
              : raw.length < 120 ? raw : "Could not join store — check credentials and network.";
      setError(friendlyMsg);
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

export default function SetupWizard({ onComplete, onMigrate }: Props) {
  const [path, setPath] = useState<SetupPath>(null);

  if (path === "new")  return <div className="setup-screen"><NewStoreWizard onComplete={onComplete} onMigrate={onMigrate ?? onComplete} /></div>;
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
