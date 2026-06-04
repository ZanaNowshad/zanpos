import { type DragEvent, useRef, useState } from "react";
import type { BulkImportResult, BulkCategoryRow, BulkProductRow } from "../tauri/commands";
import * as cmd from "../tauri/commands";

// ── Types ─────────────────────────────────────────────────────────────────────

type Mode = "products" | "categories";

interface Props {
  mode: Mode;
  sessionUserId: string;
  onClose: () => void;
  onDone: () => void; // refresh parent list after import
}

// ── CSV helpers ───────────────────────────────────────────────────────────────

function parseCSV(text: string): string[][] {
  const lines = text.replace(/\r\n/g, "\n").replace(/\r/g, "\n").split("\n");
  return lines
    .filter(l => l.trim() !== "")
    .map(line => {
      const cols: string[] = [];
      let cur = "";
      let inQuote = false;
      for (let i = 0; i < line.length; i++) {
        const ch = line[i];
        if (ch === '"') {
          if (inQuote && line[i + 1] === '"') { cur += '"'; i++; }
          else { inQuote = !inQuote; }
        } else if (ch === "," && !inQuote) {
          cols.push(cur); cur = "";
        } else {
          cur += ch;
        }
      }
      cols.push(cur);
      return cols.map(c => c.trim());
    });
}

function rowsToCategories(rows: string[][]): BulkCategoryRow[] {
  if (rows.length < 2) return [];
  const header = rows[0].map(h => h.toLowerCase());
  const nameIdx = header.findIndex(h => h === "name");
  const orderIdx = header.findIndex(h => h.includes("sort") || h.includes("order"));
  if (nameIdx < 0) return [];
  return rows.slice(1).map(r => ({
    name: r[nameIdx] ?? "",
    sort_order: orderIdx >= 0 && r[orderIdx] ? parseInt(r[orderIdx]) || undefined : undefined,
  }));
}

function rowsToProducts(rows: string[][]): BulkProductRow[] {
  if (rows.length < 2) return [];
  const header = rows[0].map(h => h.toLowerCase().replace(/[^a-z_]/g, "_"));
  const col = (names: string[]) => names.reduce((acc, n) => acc >= 0 ? acc : header.findIndex(h => h.includes(n)), -1);

  const nameIdx      = col(["name"]);
  const catIdx       = col(["category"]);
  const priceIdx     = col(["price"]);
  const skuIdx       = col(["sku"]);
  // `barcodes` (plural, pipe-separated) takes priority over single `barcode`
  const barcodesIdx  = header.findIndex(h => h === "barcodes");
  const barcodeIdx   = barcodesIdx >= 0 ? -1 : col(["barcode"]);
  const trackIdx     = col(["track", "inventory"]);
  const taxIdx       = col(["tax"]);

  if (nameIdx < 0 || catIdx < 0 || priceIdx < 0) return [];

  return rows.slice(1).map(r => ({
    name:             r[nameIdx] ?? "",
    category_name:    catIdx >= 0  ? (r[catIdx] ?? "")    : "",
    price:            priceIdx >= 0 ? (r[priceIdx] ?? "0") : "0",
    sku:              skuIdx >= 0   ? (r[skuIdx] || undefined)      : undefined,
    barcode:          barcodeIdx >= 0 ? (r[barcodeIdx] || undefined) : undefined,
    barcodes:         barcodesIdx >= 0 ? (r[barcodesIdx] || undefined) : undefined,
    track_inventory:  trackIdx >= 0
      ? !["false", "0", "no"].includes((r[trackIdx] ?? "").toLowerCase())
      : undefined,
    tax_rule_name:    taxIdx >= 0   ? (r[taxIdx] || undefined)     : undefined,
  }));
}

// ── Component ─────────────────────────────────────────────────────────────────

export default function BulkImportModal({ mode, sessionUserId, onClose, onDone }: Props) {
  const [step, setStep]         = useState<"upload" | "preview" | "result">("upload");
  const [parsed, setParsed]     = useState<string[][]>([]);
  const [importing, setImporting] = useState(false);
  const [result, setResult]     = useState<BulkImportResult | null>(null);
  const [error, setError]       = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  const isProducts = mode === "products";

  const TEMPLATE_PRODUCTS =
    "name,category_name,price,sku,barcodes,track_inventory,tax_rule_name\n" +
    "Coca-Cola 330ml,Drinks,0.400,COLA-330,5449000000996,true,VAT 10%\n" +
    "Pepsi 330ml,Drinks,0.350,PEPS-330,1234567890|9876543210,true,VAT 10%\n" +
    "Water 500ml,Drinks,0.250,WATR-500,,true,Zero Rate\n" +
    "Sandwich,Food,0.800,,,true,";

  const TEMPLATE_CATEGORIES =
    "name,sort_order\n" +
    "Drinks,1\n" +
    "Food,2\n" +
    "Misc,3";

  function downloadTemplate() {
    const content = isProducts ? TEMPLATE_PRODUCTS : TEMPLATE_CATEGORIES;
    const blob = new Blob([content], { type: "text/csv" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = isProducts ? "products_template.csv" : "categories_template.csv";
    a.click();
    URL.revokeObjectURL(url);
  }

  function handleFile(file: File) {
    const reader = new FileReader();
    reader.onload = e => {
      const text = (e.target?.result as string) ?? "";
      const rows = parseCSV(text);
      setParsed(rows);
      setStep("preview");
      setError(null);
    };
    reader.readAsText(file);
  }

  function handleDrop(e: DragEvent) {
    e.preventDefault();
    const file = e.dataTransfer.files[0];
    if (file) handleFile(file);
  }

  const previewRows = isProducts ? rowsToProducts(parsed) : rowsToCategories(parsed);

  async function runImport() {
    setImporting(true); setError(null);
    try {
      let res: BulkImportResult;
      if (isProducts) {
        res = await cmd.adminBulkImportProducts(previewRows as BulkProductRow[], sessionUserId);
      } else {
        res = await cmd.adminBulkImportCategories(previewRows as BulkCategoryRow[], sessionUserId);
      }
      setResult(res);
      setStep("result");
      if (res.inserted > 0) onDone();
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Import failed. Check your file and try again.");
    } finally {
      setImporting(false);
    }
  }

  return (
    <div className="modal-overlay" onClick={e => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="bulk-modal">
        <div className="bulk-modal-header">
          <h2 className="bulk-modal-title">
            {isProducts ? "Bulk Import Products" : "Bulk Import Categories"}
          </h2>
          <button className="bulk-modal-close" onClick={onClose}>✕</button>
        </div>

        {/* ── Step 1: Upload ── */}
        {step === "upload" && (
          <div className="bulk-upload-area">
            <p className="bulk-upload-hint">
              Upload a CSV file to add multiple {isProducts ? "products" : "categories"} at once.
              Existing records with the same name will be skipped.
            </p>
            <button className="btn-secondary bulk-template-btn" onClick={downloadTemplate}>
              ⬇ Download Template
            </button>
            <div
              className="bulk-dropzone"
              onDrop={handleDrop}
              onDragOver={e => e.preventDefault()}
              onClick={() => fileRef.current?.click()}
            >
              <div className="bulk-dropzone-icon">📂</div>
              <p className="bulk-dropzone-text">Drop your CSV here, or click to browse</p>
              <p className="bulk-dropzone-sub">.csv files only</p>
            </div>
            <input
              ref={fileRef}
              type="file"
              accept=".csv,text/csv"
              style={{ display: "none" }}
              onChange={e => { const f = e.target.files?.[0]; if (f) handleFile(f); }}
            />

            <div className="bulk-format-box">
              <p className="bulk-format-title">Required CSV columns:</p>
              {isProducts ? (
                <table className="bulk-format-table">
                  <thead><tr><th>Column</th><th>Required</th><th>Notes</th></tr></thead>
                  <tbody>
                    <tr><td><code>name</code></td><td>✅</td><td>Product name</td></tr>
                    <tr><td><code>category_name</code></td><td>✅</td><td>Created automatically if missing</td></tr>
                    <tr><td><code>price</code></td><td>✅</td><td>Full BHD e.g. <code>1.500</code></td></tr>
                    <tr><td><code>sku</code></td><td>–</td><td>Stock Keeping Unit</td></tr>
                    <tr><td><code>barcodes</code></td><td>–</td><td>Pipe-separated: <code>12345|67890</code> (single barcode also works)</td></tr>
                    <tr><td><code>track_inventory</code></td><td>–</td><td><code>true</code> or <code>false</code> (default true)</td></tr>
                    <tr><td><code>tax_rule_name</code></td><td>–</td><td>Must match an existing tax rule name</td></tr>
                  </tbody>
                </table>
              ) : (
                <table className="bulk-format-table">
                  <thead><tr><th>Column</th><th>Required</th><th>Notes</th></tr></thead>
                  <tbody>
                    <tr><td><code>name</code></td><td>✅</td><td>Category name</td></tr>
                    <tr><td><code>sort_order</code></td><td>–</td><td>Display order number</td></tr>
                  </tbody>
                </table>
              )}
            </div>
          </div>
        )}

        {/* ── Step 2: Preview ── */}
        {step === "preview" && (
          <div className="bulk-preview-area">
            <p className="bulk-preview-count">
              {previewRows.length} row{previewRows.length !== 1 ? "s" : ""} detected
              {previewRows.length === 0 && " — no valid rows found. Check headers match the required columns."}
            </p>
            {error && <div className="bo-form-error">{error}</div>}
            {previewRows.length > 0 && (
              <div className="bulk-preview-scroll">
                <table className="bulk-preview-table">
                  <thead>
                    <tr>
                      {isProducts ? (
                        <>
                          <th>#</th><th>Name</th><th>Category</th><th>Price</th>
                          <th>SKU</th><th>Barcode(s)</th><th>Track</th>
                        </>
                      ) : (
                        <><th>#</th><th>Name</th><th>Sort</th></>
                      )}
                    </tr>
                  </thead>
                  <tbody>
                    {(previewRows as (BulkProductRow | BulkCategoryRow)[]).slice(0, 50).map((r, i) => (
                      <tr key={i}>
                        <td className="bulk-cell-num">{i + 1}</td>
                        <td>{r.name || <span className="bulk-cell-empty">(empty)</span>}</td>
                        {isProducts && (
                          <>
                            <td>{(r as BulkProductRow).category_name}</td>
                            <td>{(r as BulkProductRow).price}</td>
                            <td className="bulk-cell-dim">{(r as BulkProductRow).sku || "–"}</td>
                            <td className="bulk-cell-dim">{(r as BulkProductRow).barcodes || (r as BulkProductRow).barcode || "–"}</td>
                            <td>{(r as BulkProductRow).track_inventory === false ? "No" : "Yes"}</td>
                          </>
                        )}
                        {!isProducts && (
                          <td className="bulk-cell-dim">{(r as BulkCategoryRow).sort_order ?? "–"}</td>
                        )}
                      </tr>
                    ))}
                  </tbody>
                </table>
                {previewRows.length > 50 && (
                  <p className="bulk-preview-more">…and {previewRows.length - 50} more rows (all will be imported)</p>
                )}
              </div>
            )}
            <div className="bulk-preview-actions">
              <button className="btn-secondary" onClick={() => { setStep("upload"); setError(null); }}>
                ← Back
              </button>
              <button
                className="btn-primary"
                onClick={runImport}
                disabled={importing || previewRows.length === 0}
              >
                {importing ? "Importing…" : `Import ${previewRows.length} rows`}
              </button>
            </div>
          </div>
        )}

        {/* ── Step 3: Result ── */}
        {step === "result" && result && (
          <div className="bulk-result-area">
            <div className={`bulk-result-summary ${result.inserted > 0 ? "bulk-result-ok" : "bulk-result-warn"}`}>
              <span className="bulk-result-stat">
                ✅ <strong>{result.inserted}</strong> inserted
              </span>
              {result.skipped > 0 && (
                <span className="bulk-result-stat">
                  ⏭ <strong>{result.skipped}</strong> skipped (already exist)
                </span>
              )}
              {result.errors.length > 0 && (
                <span className="bulk-result-stat bulk-result-err-count">
                  ❌ <strong>{result.errors.length}</strong> errors
                </span>
              )}
            </div>

            {result.errors.length > 0 && (
              <div className="bulk-errors-box">
                <p className="bulk-errors-title">Rows with errors (not imported):</p>
                <div className="bulk-errors-scroll">
                  {result.errors.map((e, i) => (
                    <div key={i} className="bulk-error-row">
                      <span className="bulk-error-num">Row {e.row}</span>
                      <span className="bulk-error-name">{e.name || "(empty)"}</span>
                      <span className="bulk-error-reason">{e.reason}</span>
                    </div>
                  ))}
                </div>
              </div>
            )}

            <div className="bulk-result-actions">
              <button className="btn-secondary" onClick={() => { setStep("upload"); setParsed([]); setResult(null); }}>
                Import another file
              </button>
              <button className="btn-primary" onClick={onClose}>Done</button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
