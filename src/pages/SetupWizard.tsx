import { useRef, useState } from "react";
import type { AppConfig } from "../types";
import {
  setupWizardComplete,
  setupPullCatalog,
  hubEnable,
  hubJoin,
  hubTestConnection,
  setupSaveBenefitNumber,
  adminBulkImportCategories,
  adminBulkImportProducts,
} from "../tauri/commands";
import type { BulkCategoryRow, BulkProductRow, BulkImportResult } from "../tauri/commands";
import type { PullSummary } from "../tauri/commands";
import WhatsAppQRModal from "../components/WhatsAppQRModal";
import OnboardingWizard from "../components/onboarding/OnboardingWizard";

interface Props {
  initialConfig: AppConfig;
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

type JoinChecklistStatus = "ok" | "pending";

export interface JoinSyncChecklistItem {
  label: "Products" | "Prices" | "Barcodes" | "Users" | "Stock" | "Devices" | "Suppliers" | "Settings" | "Pending sync" | "Hub Truth";
  count: number;
  status: JoinChecklistStatus;
}

export function buildJoinSyncChecklist(summary: PullSummary | null): {
  items: JoinSyncChecklistItem[];
  showRetry: boolean;
  canEnterPos: boolean;
  blockingTables: string[];
  summary: string;
} {
  const products = (summary?.products ?? 0) + (summary?.categories ?? 0);
  const prices = summary?.product_prices ?? 0;
  const barcodes = summary?.product_barcodes ?? 0;
  const users = summary?.users ?? 0;
  const stock = summary?.stock_levels ?? 0;
  const devices = summary?.devices ?? 0;
  const suppliers = summary?.suppliers ?? 0;
  const settings = summary?.settings ?? 0;
  const pendingSync = summary?.pending_sync ?? 0;
  const truthScore = summary?.consistency_score ?? 0;
  const hubTruthOk = Boolean(
    summary?.hub_truth_ok
      && summary.schema_match
      && truthScore === 100
      && pendingSync === 0,
  );
  const items: JoinSyncChecklistItem[] = [
    { label: "Products", count: products, status: products > 0 ? "ok" : "pending" },
    { label: "Prices", count: prices, status: prices > 0 ? "ok" : "pending" },
    { label: "Barcodes", count: barcodes, status: barcodes > 0 ? "ok" : "pending" },
    { label: "Users", count: users, status: users > 0 ? "ok" : "pending" },
    { label: "Stock", count: stock, status: stock > 0 ? "ok" : "pending" },
    { label: "Devices", count: devices, status: devices > 0 ? "ok" : "pending" },
    { label: "Suppliers", count: suppliers, status: suppliers > 0 ? "ok" : "pending" },
    { label: "Settings", count: settings, status: settings > 0 ? "ok" : "pending" },
    { label: "Pending sync", count: pendingSync, status: pendingSync === 0 ? "ok" : "pending" },
    { label: "Hub Truth", count: truthScore, status: hubTruthOk ? "ok" : "pending" },
  ];
  const blockingTables = summary?.mismatched_tables ?? [];
  const blockers: string[] = [];
  if (blockingTables.length > 0) blockers.push(`Mismatched: ${blockingTables.join(", ")}`);
  if (summary && !summary.schema_match) blockers.push("Schema version differs from the hub");
  if (pendingSync > 0) blockers.push(`${pendingSync} local change${pendingSync === 1 ? "" : "s"} still pending`);

  return {
    items,
    showRetry: Boolean(summary && !hubTruthOk),
    canEnterPos: hubTruthOk,
    blockingTables,
    summary: !summary
      ? "Waiting for the hub snapshot."
      : hubTruthOk
        ? "This terminal matches the hub truth snapshot."
        : blockers.length > 0
          ? blockers.join(" · ")
          : summary.error || "This terminal has not matched the hub truth snapshot yet.",
  };
}

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
            <button className="setup-btn-skip" type="button" onClick={() => { setParsed([]); setErr(null); }}><span className="icon-directional" aria-hidden="true">←</span> Cancel</button>
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

  // Step 2 — Hub (multi-terminal)
  const [enableHub, setEnableHub] = useState(true);
  const [hubPort, setHubPort] = useState("8923");

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

  const [error, setError]         = useState<string | null>(null);
  // Per-button loading states — prevents ALL buttons showing "Setting up…" when one is clicked
  const [finishLoading, setFinishLoading]   = useState(false);
  const [csvLoading, setCsvLoading]         = useState(false);
  const [migrateLoading, setMigrateLoading] = useState(false);
  const anyLoading = finishLoading || csvLoading || migrateLoading;

  const clearError = () => setError(null);

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

  // Shared setup completion — accepts which button's loading setter to use
  const doSetupComplete = async (setLd: (v: boolean) => void): Promise<AppConfig | null> => {
    // Guard: reuse already-completed config (e.g. user navigates back then clicks again)
    if (completedConfig) return completedConfig;
    setLd(true);
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
      setCompletedConfig(cfg);
      return cfg;
    } catch (e: unknown) {
      const msg = typeof e === "string" ? e : (e instanceof Error ? e.message : null);
      setError(msg || "Setup failed — please try again");
      return null;
    } finally {
      setLd(false);
    }
  };

  const handleFinish = async () => {
    const cfg = await doSetupComplete(setFinishLoading);
    if (!cfg) return;
    // If hub was enabled in the wizard, activate it now
    if (enableHub && cfg.owner_user_id) {
      try {
        await hubEnable(cfg.owner_user_id, parseInt(hubPort, 10) || 8923);
      } catch { /* non-blocking — hub can be enabled later in Settings */ }
    }
    onComplete(cfg);
  };

  const handleGoImportCSV = async () => {
    const cfg = await doSetupComplete(setCsvLoading);
    if (!cfg) return;
    if (enableHub && cfg.owner_user_id) {
      try { await hubEnable(cfg.owner_user_id, parseInt(hubPort, 10) || 8923); } catch { /* ignore */ }
    }
    setOwnerUserId(cfg.owner_user_id ?? "");
    setStep(8);
  };

  // ── Render ─────────────────────────────────────────────────────────────────

  const LABELS = ["Hub", "Store Info", "Contact", "Payments", "Owner", "Review"];
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

      {/* ── Step 2: Multi-Terminal Hub ── */}
      {step === 2 && (
        <div className="setup-content">
          <h2 className="setup-title">Multi-Terminal Hub</h2>
          <p className="setup-body">
            Use this on the main computer for the store. It becomes the source
            hub for products, stock, users, settings, sales, payments, and reports
            across your other POS terminals on the same shop WiFi.
          </p>

          <label htmlFor="a11y-input-1" className="field-label">
            <input id="a11y-input-1" type="checkbox" checked={enableHub} onChange={e => setEnableHub(e.target.checked)} />
            {" "}Enable Hub on this device
          </label>

          {enableHub && (
            <div>
              <label htmlFor="a11y-input-2" className="field-label">Hub Port</label>
              <input id="a11y-input-2"
                className="field-input"
                type="number"
                value={hubPort}
                onChange={e => { setHubPort(e.target.value); clearError(); }}
              />
            </div>
          )}

          <p className="setup-hint">
            Set up the first device as the Hub. On every extra till, choose Join Existing Store
            and enter the Hub address plus store token from Back Office → Settings → Hub.
          </p>

          <div className="setup-actions">
            <button className="setup-btn-secondary" onClick={() => { setEnableHub(false); setStep(3); }}>
              Skip for now
            </button>
            <button className="setup-btn-primary" onClick={() => setStep(3)}>
              Continue <span className="icon-directional" aria-hidden="true">→</span>
            </button>
          </div>
        </div>
      )}

      {/* ── Step 3: Store info ── */}
      {step === 3 && (
        <div className="setup-content">
          <h2 className="setup-title">Store Information</h2>

          <label htmlFor="a11y-input-3" className="field-label">Store Name *</label>
          <input id="a11y-input-3"
            className="field-input"
            type="text"
            placeholder="e.g. My Coffee Shop"
            value={storeName}
            onChange={e => { setStoreName(e.target.value); clearError(); }}

            maxLength={60}
          />

          <label htmlFor="a11y-input-4" className="field-label">Currency *</label>
          <select id="a11y-input-4" className="bo-select field-input" value={currency} onChange={e => setCurrency(e.target.value)}>
            {CURRENCIES.map(c => <option key={c.code} value={c.code}>{c.label}</option>)}
          </select>

          <label htmlFor="a11y-input-5" className="field-label">Timezone *</label>
          <select id="a11y-input-5" className="bo-select field-input" value={timezone} onChange={e => setTimezone(e.target.value)}>
            {TIMEZONES.map(tz => <option key={tz} value={tz}>{tz}</option>)}
          </select>

          {error && <div className="modal-error">{error}</div>}
          <div className="setup-actions">
            <button className="setup-btn-secondary" onClick={() => { setError(null); setStep(2); }}><span className="icon-directional" aria-hidden="true">←</span> Back</button>
            <button className="setup-btn-primary" onClick={goNext}>Next <span className="icon-directional" aria-hidden="true">→</span></button>
          </div>
        </div>
      )}

      {/* ── Step 4: Contact + receipt ── */}
      {step === 4 && (
        <div className="setup-content">
          <h2 className="setup-title">Contact & Receipt</h2>
          <p className="setup-body">All fields are optional — update anytime in Settings.</p>

          <label htmlFor="a11y-input-6" className="field-label">Address</label>
          <textarea id="a11y-input-6" className="field-input" rows={2} placeholder="123 Main Street, City" value={address} onChange={e => setAddress(e.target.value)} />

          <label htmlFor="a11y-input-7" className="field-label">Phone Number</label>
          <input id="a11y-input-7" className="field-input" type="tel" placeholder="+973 1234 5678" value={phone} onChange={e => setPhone(e.target.value)} />

          <label htmlFor="a11y-input-8" className="field-label">Tax / VAT Registration Number</label>
          <input id="a11y-input-8" className="field-input" type="text" placeholder="Optional" value={taxNumber} onChange={e => setTaxNumber(e.target.value)} />

          <label htmlFor="a11y-input-9" className="field-label">Commercial Register Number (CR No)</label>
          <input id="a11y-input-9" className="field-input" type="text" placeholder="Optional" value={crNumber} onChange={e => setCrNumber(e.target.value)} />

          <label htmlFor="a11y-input-10" className="field-label">Receipt Header</label>
          <input id="a11y-input-10" className="field-input" type="text" placeholder="e.g. Thank you for visiting!" value={receiptHeader} onChange={e => setReceiptHeader(e.target.value)} />

          <label htmlFor="a11y-input-11" className="field-label">Receipt Footer</label>
          <input id="a11y-input-11" className="field-input" type="text" placeholder="Thank you for your purchase!" value={receiptFooter} onChange={e => setReceiptFooter(e.target.value)} />

          <div className="setup-actions">
            <button className="setup-btn-secondary" onClick={() => { setError(null); setStep(3); }}><span className="icon-directional" aria-hidden="true">←</span> Back</button>
            <button className="setup-btn-primary" onClick={goNext}>Next <span className="icon-directional" aria-hidden="true">→</span></button>
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

          <label htmlFor="a11y-input-12" className="field-label">BenefitPay Number <span style={{ opacity: 0.6, fontSize: "0.82em" }}>(optional)</span></label>
          <p className="setup-field-hint">Customers send delivery payments to this number via BenefitPay. You can add it later in Settings.</p>
          <input id="a11y-input-12"
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
            <button className="setup-btn-secondary" onClick={() => { setError(null); setStep(4); }}><span className="icon-directional" aria-hidden="true">←</span> Back</button>
            <button className="setup-btn-primary" onClick={goNext}>
              Next <span className="icon-directional" aria-hidden="true">→</span>
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

          <label htmlFor="a11y-input-13" className="field-label">Full Name *</label>
 <input id="a11y-input-13" className="field-input" type="text" placeholder="e.g. Mohammed Al-Farsi" value={ownerName} onChange={e => { setOwnerName(e.target.value); clearError(); }} />

          <label htmlFor="a11y-input-14" className="field-label">Username *</label>
          <input id="a11y-input-14" className="field-input" type="text" placeholder="admin" value={ownerUsername} onChange={e => { setOwnerUsername(e.target.value.toLowerCase().replace(/\s/g, "")); clearError(); }} />

          <label htmlFor="a11y-input-15" className="field-label">PIN (4–6 digits) *</label>
          <input id="a11y-input-15" className="field-input setup-pin-input" type="password" inputMode="numeric" pattern="[0-9]*" placeholder="Enter PIN" maxLength={6} value={ownerPin} onChange={e => { setOwnerPin(e.target.value.replace(/\D/g, "")); clearError(); }} />

          <label htmlFor="a11y-input-16" className="field-label">Confirm PIN *</label>
          <input id="a11y-input-16" className="field-input setup-pin-input" type="password" inputMode="numeric" pattern="[0-9]*" placeholder="Re-enter PIN" maxLength={6} value={ownerPinConfirm} onChange={e => { setOwnerPinConfirm(e.target.value.replace(/\D/g, "")); clearError(); }} />

          {error && <div className="modal-error">{error}</div>}
          <div className="setup-actions">
            <button className="setup-btn-secondary" onClick={() => { setError(null); setStep(5); }}><span className="icon-directional" aria-hidden="true">←</span> Back</button>
            <button className="setup-btn-primary" onClick={goNext}>Review <span className="icon-directional" aria-hidden="true">→</span></button>
          </div>
        </div>
      )}

      {/* ── Step 7: Review & launch ── */}
      {step === 7 && (
        <div className="setup-content setup-done">
          <div className="setup-done-icon">🎉</div>
          <h2 className="setup-title">Ready to go!</h2>

          <div className="setup-review">
            {enableHub
              ? <div className="setup-review-row"><span>Hub</span><strong>Hub enabled on this device</strong></div>
              : <div className="setup-review-row setup-review-warn"><span>Hub</span><strong>Not enabled (can enable later in Settings)</strong></div>
            }
            <div className="setup-review-row"><span>Store name</span><strong>{storeName}</strong></div>
            <div className="setup-review-row"><span>Currency</span><strong>{currency}</strong></div>
            <div className="setup-review-row"><span>Timezone</span><strong>{timezone}</strong></div>
            {address    && <div className="setup-review-row"><span>Address</span><strong>{address}</strong></div>}
            {phone      && <div className="setup-review-row"><span>Phone</span><strong>{phone}</strong></div>}
            {taxNumber  && <div className="setup-review-row"><span>Tax / VAT #</span><strong>{taxNumber}</strong></div>}
            {crNumber   && <div className="setup-review-row"><span>CR Number</span><strong>{crNumber}</strong></div>}
            {benefitNumber && <div className="setup-review-row"><span>BenefitPay #</span><strong>{benefitNumber}</strong></div>}
            <div className="setup-review-row"><span>Owner account</span><strong>{ownerName} ({ownerUsername})</strong></div>
          </div>

          <p className="setup-body" style={{ marginTop: 16 }}>
            Change any of this later in <strong>Back Office → Settings</strong>.
          </p>

          {error && <div className="modal-error">{error}</div>}

          <div className="setup-actions setup-actions-back">
            <button className="setup-btn-secondary" onClick={() => { setError(null); setStep(6); }} disabled={anyLoading}><span className="icon-directional" aria-hidden="true">←</span> Back</button>
          </div>

          <div className="setup-launch-options">
            {/* Option 1 — Go straight to POS */}
            <div className="setup-launch-card">
              <div className="setup-launch-card-icon">🚀</div>
              <div className="setup-launch-card-body">
                <div className="setup-launch-card-title">Start Fresh</div>
                <div className="setup-launch-card-desc">Launch the POS right now. Add products and categories manually from Back Office.</div>
              </div>
              <button className="setup-btn-primary setup-btn-finish" onClick={handleFinish} disabled={anyLoading}>
                {finishLoading ? "Setting up…" : "Launch POS"}
              </button>
            </div>

            {/* Option 2 — Upload CSV */}
            <div className="setup-launch-card setup-launch-card-highlight">
              <div className="setup-launch-card-icon">📊</div>
              <div className="setup-launch-card-body">
                <div className="setup-launch-card-title">Import from CSV</div>
                <div className="setup-launch-card-desc">Upload your categories and products now using a spreadsheet. Quick setup in minutes.</div>
              </div>
              <button className="setup-btn-primary" onClick={handleGoImportCSV} disabled={anyLoading}>
                {csvLoading ? "Setting up…" : <>Upload CSV <span className="icon-directional" aria-hidden="true">→</span></>}
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
                const cfg = await doSetupComplete(setMigrateLoading);
                if (!cfg) return;
                if (enableHub && cfg.owner_user_id) {
                  try { await hubEnable(cfg.owner_user_id, parseInt(hubPort, 10) || 8923); } catch { /* ignore */ }
                }
                onMigrate(cfg);
              }} disabled={anyLoading}>
                {migrateLoading ? "Setting up…" : <>Import with AI <span className="icon-directional" aria-hidden="true">→</span></>}
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

function JoinStoreWizard({ onComplete, onBack }: { onComplete: (cfg: AppConfig) => void; onBack?: () => void }) {
  const [step, setStep]           = useState<JoinStep>("creds");
  const [hubUrl, setHubUrl]         = useState("");
  const [token, setToken]         = useState("");
  const [storeName, setStoreName] = useState<string | null>(null);
  const [deviceName, setDeviceName] = useState("");
  const [deviceCode, setDeviceCode] = useState("");
  const [error, setError]         = useState<string | null>(null);
  const [loading, setLoading]     = useState(false);
  const [pullStatus, setPullStatus] = useState<string | null>(null);
  const [joinedConfig, setJoinedConfig] = useState<AppConfig | null>(null);
  const [pullSummary, setPullSummary] = useState<PullSummary | null>(null);

  const clearError = () => setError(null);

  const runInitialPull = async (): Promise<PullSummary | null> => {
    setLoading(true);
    setError(null);
    setPullStatus("Downloading store data to this terminal...");
    try {
      const summary = await setupPullCatalog();
      setPullSummary(summary);
      if (summary.rows_pulled > 0) {
        setPullStatus(`Downloaded ${summary.rows_pulled} synced records`);
      } else if (!summary.ok) {
        setPullStatus("Initial sync incomplete - ZANPOS will retry automatically");
      } else {
        setPullStatus("Connected and ready");
      }
      return summary;
    } catch (e) {
      setPullStatus("Initial sync incomplete - use Retry before opening POS");
      setError(typeof e === "string" ? e : "Could not finish the initial sync.");
      setPullSummary({
        ok: false,
        rows_pulled: 0,
        error: typeof e === "string" ? e : String(e),
        products: 0,
        categories: 0,
        product_barcodes: 0,
        product_prices: 0,
        users: 0,
        devices: 0,
        stock_levels: 0,
        suppliers: 0,
        settings: 0,
        pending_sync: 0,
        consistency_score: 0,
        schema_match: false,
        hub_truth_ok: false,
        mismatched_tables: [],
      });
      return null;
    } finally {
      setLoading(false);
    }
  };

  const handleValidateCreds = async () => {
    if (!hubUrl.trim()) { setError("Hub address is required"); return; }
    if (!token.trim()) { setError("Store token is required"); return; }
    setLoading(true);
    setError(null);
    try {
      const result = await hubTestConnection(hubUrl.trim(), token.trim());
      if (!result.ok) {
        setError(result.error ?? "Connection failed — check address and token.");
        return;
      }
      setStoreName(result.store_name);
      setStep("device");
    } catch (e: unknown) {
      const raw = typeof e === "string" ? e : String(e);
      setError(raw.length < 160 ? raw : "Connection check failed.");
    } finally {
      setLoading(false);
    }
  };

  const handleJoin = async () => {
    if (!deviceName.trim()) { setError("Device name is required"); return; }
    if (!deviceCode.trim()) { setError("Device code is required"); return; }
    setLoading(true);
    setError(null);
    setStep("joining");
    setPullSummary(null);
    setJoinedConfig(null);
    setPullStatus("Connecting to the hub store...");
    try {
      const cfg = await hubJoin({
        hub_url: hubUrl.trim(),
        token: token.trim(),
        device_name: deviceName.trim(),
        device_code: deviceCode.trim().toUpperCase(),
      });
      setJoinedConfig(cfg);

      const summary = await runInitialPull();
      const checklist = buildJoinSyncChecklist(summary);
      if (summary?.ok && !checklist.showRetry) {
        await new Promise(r => setTimeout(r, 800));
        onComplete(cfg);
      }
    } catch (e: unknown) {
      const raw = typeof e === "string" ? e : String(e);
      const friendlyMsg =
        raw.includes("pull_branch") || raw.includes("incomplete")
          ? "No store found on this hub. Make sure Device 1 has been set up as the hub."
          : raw.includes("Wrong store token")
            ? "Invalid store token — check the token from the hub device."
            : raw.includes("connect") || raw.includes("NETWORK")
              ? "Could not reach the hub — check the address and that both devices are on the same WiFi."
              : raw.length < 120 ? raw : "Could not join store — check address and token.";
      setError(friendlyMsg);
      setStep("device");
    } finally {
      setLoading(false);
    }
  };

  const handleRetryPull = async () => {
    const summary = await runInitialPull();
    const checklist = buildJoinSyncChecklist(summary);
    if (joinedConfig && summary?.ok && !checklist.showRetry) {
      await new Promise(r => setTimeout(r, 500));
      onComplete(joinedConfig);
    }
  };

  const joinChecklist = buildJoinSyncChecklist(pullSummary);

  return (
    <div className="setup-panel">
      {step === "creds" && (
        <div className="setup-content">
          <h2 className="setup-title">Join Existing Store</h2>
          <p className="setup-body">
            Enter the hub address and store token shown on the hub device's
            Back Office → Settings → Hub screen. This terminal will download the
            store catalog, stock, users, devices, settings, sales history, payments,
            deliveries, and reports over your shop WiFi.
          </p>

          <label htmlFor="a11y-input-17" className="field-label">Hub Address *</label>
          <input id="a11y-input-17"
            className="field-input"
            placeholder="192.168.1.50"
            value={hubUrl}
            onChange={e => { setHubUrl(e.target.value); clearError(); }}

          />

          <label htmlFor="a11y-input-18" className="field-label">Store Token *</label>
          <input id="a11y-input-18"
            className="field-input"
            type="password"
            placeholder="Paste the token from the hub…"
            value={token}
            onChange={e => { setToken(e.target.value); clearError(); }}
          />

          {error && <div className="modal-error">{error}</div>}
          <div className="setup-actions">
            {onBack && (
              <button className="setup-btn-secondary" onClick={onBack} disabled={loading}><span className="icon-directional" aria-hidden="true">←</span> Back</button>
            )}
            <button className="setup-btn-primary" onClick={handleValidateCreds} disabled={loading}>
              {loading ? "Checking…" : <>Next <span className="icon-directional" aria-hidden="true">→</span></>}
            </button>
          </div>
        </div>
      )}

      {step === "device" && (
        <div className="setup-content">
          <h2 className="setup-title">Register This Terminal</h2>
          {storeName && (
            <p className="setup-body" style={{ color: "var(--success)", fontWeight: 500 }}>
              Found store: {storeName}
            </p>
          )}
          <p className="setup-body">
            Give this terminal a unique name and short code. After joining, this device keeps
            its own local database and syncs changes with the hub automatically.
          </p>

          <label htmlFor="a11y-input-19" className="field-label">Terminal Name *</label>
          <input id="a11y-input-19"
            className="field-input"
            placeholder="e.g. POS Terminal 2"
            value={deviceName}
            onChange={e => { setDeviceName(e.target.value); clearError(); }}

          />

          <label htmlFor="a11y-input-20" className="field-label">Terminal Code * (short, uppercase)</label>
          <input id="a11y-input-20"
            className="field-input"
            placeholder="e.g. POS02"
            value={deviceCode}
            onChange={e => { setDeviceCode(e.target.value.toUpperCase().replace(/\s/g, "")); clearError(); }}
            maxLength={10}
          />

          {error && <div className="modal-error">{error}</div>}
          <div className="setup-actions">
            <button className="setup-btn-secondary" onClick={() => { setError(null); setStep("creds"); }} disabled={loading}><span className="icon-directional" aria-hidden="true">←</span> Back</button>
            <button className="setup-btn-primary" onClick={handleJoin} disabled={loading}>
              {loading ? "Joining…" : "Join Store 🔗"}
            </button>
          </div>
        </div>
      )}

      {step === "joining" && (
        <div className="setup-content" style={{ textAlign: "center", padding: "40px 0" }}>
          {loading && <div className="app-splash-spinner" style={{ margin: "0 auto 20px" }} />}
          <p style={{ marginBottom: 16 }}>{pullStatus ?? "Connecting to store and downloading synced data..."}</p>

          {pullSummary && (
            <div className="setup-sync-review">
              <div className={`setup-sync-summary ${joinChecklist.canEnterPos ? "setup-sync-summary-ok" : "setup-sync-summary-blocked"}`}>
                {joinChecklist.summary}
              </div>
              <div className="setup-sync-checklist">
                {joinChecklist.items.map(item => (
                  <div key={item.label} className={`setup-sync-check ${item.status === "ok" ? "setup-sync-ok" : "setup-sync-pending"}`}>
                    <span className="setup-sync-mark">{item.status === "ok" ? "OK" : "..."}</span>
                    <span>{item.label}</span>
                    <strong>{item.count}</strong>
                  </div>
                ))}
              </div>
              {joinChecklist.blockingTables.length > 0 && (
                <div className="setup-sync-blockers" aria-label="Tables not matching hub">
                  {joinChecklist.blockingTables.map(table => <span key={table}>{table}</span>)}
                </div>
              )}
            </div>
          )}

          {error && <div className="modal-error">{error}</div>}

          {joinChecklist.showRetry ? (
            <div className="setup-actions" style={{ justifyContent: "center" }}>
              <button className="setup-btn-secondary" onClick={() => setStep("device")} disabled={loading}>Back</button>
              <button className="setup-btn-primary" onClick={handleRetryPull} disabled={loading}>
                {loading ? "Retrying..." : "Retry Sync"}
              </button>
            </div>
          ) : loading ? (
            <button className="setup-btn-skip" onClick={() => { setStep("device"); setLoading(false); setPullStatus(null); }}>
              Cancel
            </button>
          ) : joinedConfig ? (
            <button className="setup-btn-primary" onClick={() => onComplete(joinedConfig)}>
              Start POS
            </button>
          ) : null}
        </div>
      )}
    </div>
  );
}

// ─── Root SetupWizard — path selector ────────────────────────────────────────
//
// Three mutually exclusive first-run routes. Only the guided "new store" flow
// lives outside this file; the other two are defined above.

type SetupPath = null | "new" | "join" | "migrate";

export default function SetupWizard({ initialConfig, onComplete, onMigrate }: Props) {
  const [path, setPath] = useState<SetupPath>(null);

  // "new" is the guided six-step flow (store → owner → WhatsApp → products →
  // printer → go live), resumable across restarts. "migrate" keeps the older
  // one-shot flow, which is the only route to the AI import-from-another-POS
  // agent and to enabling the Hub during setup — neither exists in the guided
  // flow, so this path must not be folded away.
  if (path === "new")     return <OnboardingWizard onComplete={onComplete} />;
  if (path === "migrate") return <div className="setup-screen"><NewStoreWizard onComplete={onComplete} onMigrate={onMigrate ?? onComplete} /></div>;
  if (path === "join")    return <div className="setup-screen"><JoinStoreWizard onComplete={onComplete} onBack={() => setPath(null)} /></div>;

  // ── Path selector ──────────────────────────────────────────────────────────
  return (
    <div className="setup-screen">
      <div className="setup-panel">
        <div className="setup-content">
          <div className="setup-logo">ZAN<span>POS</span></div>
          <h1 className="setup-title">Welcome to ZanPOS</h1>
          <p className="setup-body">
            ZanPOS keeps your store data local and syncs across terminals over your
            shop WiFi — no internet required. One device acts as the Hub; other tills
            connect to it for multi-terminal operation.
          </p>
          <p className="setup-body">What would you like to do?</p>
          <div className="setup-source-card" aria-label="Current database source">
            <span>Current data source</span>
            <code>{initialConfig.database_path || "Database path unavailable"}</code>
            <small>
              Setup is {initialConfig.setup_complete ? "complete" : "not complete"} for {initialConfig.branch_name} ({initialConfig.branch_code}) on device {initialConfig.device_id.slice(0, 8)}.
            </small>
          </div>

          <div className="setup-path-cards">
            <button className="setup-path-card" onClick={() => setPath("new")}>
              <span className="setup-path-icon">🏪</span>
              <span className="setup-path-title">New Store</span>
              <span className="setup-path-desc">
                Set up a brand-new store, guided step by step: store details and
                owner account, WhatsApp, your products, a printer test, and your
                public shop. You can stop and pick up where you left off.
              </span>
            </button>

            <button className="setup-path-card" onClick={() => setPath("migrate")}>
              <span className="setup-path-icon">🤖</span>
              <span className="setup-path-title">Move from Another POS</span>
              <span className="setup-path-desc">
                Set up a new store and bring your existing data with you — an
                AI-guided import from SQL Server, SQLite, Access, or MySQL.
                Also the place to enable the Hub during setup.
              </span>
            </button>

            <button className="setup-path-card" onClick={() => setPath("join")}>
              <span className="setup-path-icon">🔗</span>
              <span className="setup-path-title">Join Existing Store</span>
              <span className="setup-path-desc">
                Add this terminal to a store that's already running. Enter the
                hub address and store token, and this device will sync the store's
                catalog and users automatically.
              </span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
