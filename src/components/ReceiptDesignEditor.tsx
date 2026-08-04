import { useState } from "react";

export interface ReceiptDesign {
  paper_width: "58mm" | "80mm";
  show_store_name: boolean;
  show_address: boolean;
  show_phone: boolean;
  show_tax_number: boolean;
  show_header: boolean;
  show_footer: boolean;
  font_size: "small" | "medium" | "large";
  alignment: "left" | "center";
  show_dividers: boolean;
  bold_total: boolean;
  default_printer: string;
  print_mode: "thermal" | "windows";
}

const STORAGE_KEY = "zanpos_receipt_design";

export const DEFAULT_DESIGN: ReceiptDesign = {
  paper_width: "80mm",
  show_store_name: true,
  show_address: true,
  show_phone: true,
  show_tax_number: true,
  show_header: true,
  show_footer: true,
  font_size: "medium",
  alignment: "center",
  show_dividers: true,
  bold_total: true,
  default_printer: "",
  print_mode: "thermal",
};

export function loadReceiptDesign(): ReceiptDesign {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    const loaded = raw ? { ...DEFAULT_DESIGN, ...JSON.parse(raw) } : DEFAULT_DESIGN;
    // F-MED-01: coerce any legacy "windows" print_mode to "thermal" (only supported mode)
    if (loaded.print_mode !== "thermal") loaded.print_mode = "thermal";
    return loaded;
  } catch {
    return DEFAULT_DESIGN;
  }
}

interface Props {
  storeName?: string;
  storeAddress?: string;
  storePhone?: string;
  taxNumber?: string;
  crNumber?: string;
  receiptHeader?: string;
  receiptFooter?: string;
}

// ── Toggle row helper ────────────────────────────────────────────────────────
function Toggle({ label, hint, value, onChange }: {
  label: string; hint?: string; value: boolean; onChange: (v: boolean) => void;
}) {
  return (
    <div className="rd-toggle-row">
      <div className="rd-toggle-info">
        <span className="rd-toggle-label">{label}</span>
        {hint && <span className="rd-toggle-hint">{hint}</span>}
      </div>
      <button
        className={`rd-toggle-btn${value ? " rd-toggle-on" : ""}`}
        onClick={() => onChange(!value)}
        role="switch"
        aria-checked={value}
      >
        <span className="rd-toggle-thumb" />
      </button>
    </div>
  );
}

// ── Mini receipt preview ─────────────────────────────────────────────────────
function ReceiptPreview({ d, storeName, storeAddress, storePhone, taxNumber, crNumber, receiptHeader, receiptFooter }: {
  d: ReceiptDesign;
  storeName?: string;
  storeAddress?: string;
  storePhone?: string;
  taxNumber?: string;
  crNumber?: string;
  receiptHeader?: string;
  receiptFooter?: string;
}) {
  const fontScale = d.font_size === "small" ? 0.72 : d.font_size === "large" ? 0.9 : 0.8;
  const align = d.alignment;
  const divider = d.show_dividers ? "─────────────────────" : "";

  return (
    <div
      className={`rd-preview-paper rd-paper-${d.paper_width.replace("mm", "")}`}
      style={{ fontSize: `${fontScale}rem`, fontFamily: "IBM Plex Mono, monospace" }}
    >
      {d.show_store_name && (
        <div className="rd-preview-line" style={{ textAlign: align, fontWeight: 700, fontSize: "1.1em" }}>
          {storeName || "Your Store Name"}
        </div>
      )}
      {d.show_address && storeAddress && (
        <div className="rd-preview-line" style={{ textAlign: align }}>{storeAddress}</div>
      )}
      {d.show_phone && storePhone && (
        <div className="rd-preview-line" style={{ textAlign: align }}>{storePhone}</div>
      )}
      {d.show_tax_number && taxNumber && (
        <div className="rd-preview-line" style={{ textAlign: align }}>TRN: {taxNumber}</div>
      )}
      {d.show_tax_number && crNumber && (
        <div className="rd-preview-line" style={{ textAlign: align }}>CR No: {crNumber}</div>
      )}
      {d.show_header && receiptHeader && (
        <div className="rd-preview-line" style={{ textAlign: align, fontStyle: "italic" }}>{receiptHeader}</div>
      )}
      {divider && <div className="rd-preview-line" style={{ textAlign: "center", opacity: 0.4 }}>{divider}</div>}

      <div className="rd-preview-meta">
        <span>Receipt #00042</span>
        <span>21/05/2026 14:32</span>
      </div>

      {divider && <div className="rd-preview-line" style={{ textAlign: "center", opacity: 0.4 }}>{divider}</div>}

      <div className="rd-preview-item">
        <span>Milk 1L × 2</span>
        <span>0.800</span>
      </div>
      <div className="rd-preview-item">
        <span>Bread Whole Wheat</span>
        <span>0.350</span>
      </div>
      <div className="rd-preview-item">
        <span>Orange Juice</span>
        <span>0.500</span>
      </div>

      {divider && <div className="rd-preview-line" style={{ textAlign: "center", opacity: 0.4 }}>{divider}</div>}

      <div className="rd-preview-item" style={{ fontWeight: d.bold_total ? 700 : 400, fontSize: d.bold_total ? "1.05em" : "1em" }}>
        <span>TOTAL (BHD)</span>
        <span>1.650</span>
      </div>
      <div className="rd-preview-item" style={{ opacity: 0.7 }}>
        <span>Cash Tendered</span>
        <span>2.000</span>
      </div>
      <div className="rd-preview-item" style={{ opacity: 0.7 }}>
        <span>Change</span>
        <span>0.350</span>
      </div>

      {divider && <div className="rd-preview-line" style={{ textAlign: "center", opacity: 0.4 }}>{divider}</div>}

      {d.show_footer && receiptFooter && (
        <div className="rd-preview-line" style={{ textAlign: "center", fontStyle: "italic", opacity: 0.8 }}>{receiptFooter}</div>
      )}
      {!receiptFooter && (
        <div className="rd-preview-line" style={{ textAlign: "center", opacity: 0.5 }}>Thank you for your visit!</div>
      )}
    </div>
  );
}

// ── Main editor ──────────────────────────────────────────────────────────────
export default function ReceiptDesignEditor({ storeName, storeAddress, storePhone, taxNumber, crNumber, receiptHeader, receiptFooter }: Props) {
  const [design, setDesign] = useState<ReceiptDesign>(loadReceiptDesign);
  const [saved, setSaved]   = useState(false);

  const update = (patch: Partial<ReceiptDesign>) => {
    setDesign(prev => ({ ...prev, ...patch }));
    setSaved(false);
  };

  const handleSave = () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(design));
    setSaved(true);
    setTimeout(() => setSaved(false), 2500);
  };

  const [resetConfirm, setResetConfirm] = useState(false);

  const handleReset = () => {
    if (!resetConfirm) {
      setResetConfirm(true);
      return;
    }
    setResetConfirm(false);
    setDesign(DEFAULT_DESIGN);
    localStorage.removeItem(STORAGE_KEY);
    setSaved(false);
  };

  return (
    <div className="rd-editor">

      {/* ── Left: Controls ── */}
      <div className="rd-controls">

        {/* Paper & Layout */}
        <div className="rd-section">
          <div className="rd-section-title">Paper & Layout</div>

          <div className="rd-field">
            <span className="bo-label">Paper Width</span>
            <div className="rd-radio-group">
              {(["58mm", "80mm"] as const).map(w => (
                <button
                  key={w}
                  className={`rd-radio-btn${design.paper_width === w ? " active" : ""}`}
                  onClick={() => update({ paper_width: w })}
                >{w}</button>
              ))}
            </div>
          </div>

          <div className="rd-field">
            <span className="bo-label">Text Alignment</span>
            <div className="rd-radio-group">
              {(["left", "center"] as const).map(a => (
                <button
                  key={a}
                  className={`rd-radio-btn${design.alignment === a ? " active" : ""}`}
                  onClick={() => update({ alignment: a })}
                >{a.charAt(0).toUpperCase() + a.slice(1)}</button>
              ))}
            </div>
          </div>

          <div className="rd-field">
            <span className="bo-label">Font Size</span>
            <div className="rd-radio-group">
              {(["small", "medium", "large"] as const).map(s => (
                <button
                  key={s}
                  className={`rd-radio-btn${design.font_size === s ? " active" : ""}`}
                  onClick={() => update({ font_size: s })}
                >{s.charAt(0).toUpperCase() + s.slice(1)}</button>
              ))}
            </div>
          </div>
        </div>

        {/* Visibility */}
        <div className="rd-section">
          <div className="rd-section-title">Show on Receipt</div>
          <Toggle label="Store Name"     value={design.show_store_name}  onChange={v => update({ show_store_name: v })} />
          <Toggle label="Address"        value={design.show_address}     onChange={v => update({ show_address: v })} />
          <Toggle label="Phone"          value={design.show_phone}       onChange={v => update({ show_phone: v })} />
          <Toggle label="Tax / TRN"      value={design.show_tax_number}  onChange={v => update({ show_tax_number: v })} />
          <Toggle label="Header Text"    value={design.show_header}      onChange={v => update({ show_header: v })} hint="From Receipt Customisation" />
          <Toggle label="Footer Text"    value={design.show_footer}      onChange={v => update({ show_footer: v })} hint="From Receipt Customisation" />
          <Toggle label="Divider Lines"  value={design.show_dividers}    onChange={v => update({ show_dividers: v })} />
          <Toggle label="Bold Total"     value={design.bold_total}       onChange={v => update({ bold_total: v })} />
        </div>

        {/* Printer */}
        <div className="rd-section">
          <div className="rd-section-title">Default Printer</div>

          <div className="rd-field">
            <span className="bo-label">Print Mode</span>
            <div className="rd-radio-group">
              <button
                className={`rd-radio-btn${design.print_mode === "thermal" ? " active" : ""}`}
                onClick={() => update({ print_mode: "thermal" })}
              >Thermal (ESC/POS)</button>
            </div>
          </div>

          {/* F-MED-01: Windows GDI mode removed — thermal ESC/POS is the only supported mode.
              The thermal hint always shows since it's the sole print path. */}
          <p className="settings-hint">
            Using the ESC/POS serial port configured in the <strong>Printers</strong> section.
          </p>
        </div>

        {/* Actions */}
        <div className="rd-actions">
          <button className={`btn-secondary rd-reset-btn${resetConfirm ? " rd-reset-btn-warn" : ""}`} onClick={handleReset}>
            {resetConfirm ? "Confirm Reset?" : "Reset to Defaults"}
          </button>
          <button className="btn-primary" onClick={handleSave}>
            {saved ? "✓ Saved" : "Save Design"}
          </button>
        </div>
      </div>

      {/* ── Right: Live Preview ── */}
      <div className="rd-preview-panel">
        <div className="rd-preview-label">Live Preview</div>
        <ReceiptPreview
          d={design}
          storeName={storeName}
          storeAddress={storeAddress}
          storePhone={storePhone}
          taxNumber={taxNumber}
          crNumber={crNumber}
          receiptHeader={receiptHeader}
          receiptFooter={receiptFooter}
        />
      </div>

    </div>
  );
}
