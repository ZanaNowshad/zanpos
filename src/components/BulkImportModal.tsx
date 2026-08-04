import { type DragEvent, useMemo, useRef, useState } from "react";
import type { BulkImportResult, BulkCategoryRow, BulkProductRow } from "../tauri/commands";
import * as cmd from "../tauri/commands";
import { useLanguage } from "../hooks/useLanguage";
import { modalTranslator } from "../i18n/modalStrings";
import { detailTranslator } from "../i18n/detailStrings";

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
  const { language } = useLanguage();
  const t = useMemo(() => modalTranslator(language), [language]);
  const dt = useMemo(() => detailTranslator(language), [language]);
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
      setError(typeof e === "string" ? e : dt("importFailed"));
    } finally {
      setImporting(false);
    }
  }

  return (
    <div role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}  className="modal-overlay" onClick={e => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="bulk-modal">
        <div className="bulk-modal-header">
          <h2 className="bulk-modal-title">
            {isProducts ? t("bulkImportProducts") : t("bulkImportCategories")}
          </h2>
          <button className="bulk-modal-close" onClick={onClose}>✕</button>
        </div>

        {/* ── Step 1: Upload ── */}
        {step === "upload" && (
          <div className="bulk-upload-area">
            <p className="bulk-upload-hint">
              {t("uploadCsvHint")}
            </p>
            <button className="btn-secondary bulk-template-btn" onClick={downloadTemplate}>
              ⬇ {t("downloadTemplate")}
            </button>
            <div
              className="bulk-dropzone"
              onDrop={handleDrop}
              onDragOver={e => e.preventDefault()}
              onClick={() => fileRef.current?.click()}
            >
              <div className="bulk-dropzone-icon">📂</div>
              <p className="bulk-dropzone-text">{t("dropCsv")}</p>
              <p className="bulk-dropzone-sub">{t("csvFilesOnly")}</p>
            </div>
            <input
              ref={fileRef}
              type="file"
              accept=".csv,text/csv"
              style={{ display: "none" }}
              onChange={e => { const f = e.target.files?.[0]; if (f) handleFile(f); }}
            />

            <div className="bulk-format-box">
              <p className="bulk-format-title">{t("requiredCsvColumns")}</p>
              {isProducts ? (
                <table className="bulk-format-table">
                  <thead><tr><th>{t("column")}</th><th>{t("required")}</th><th>{t("notesOptional")}</th></tr></thead>
                  <tbody>
                    <tr><td><code>name</code></td><td>✅</td><td>{dt("productNameColumn")}</td></tr>
                    <tr><td><code>category_name</code></td><td>✅</td><td>{dt("autoCreateCategory")}</td></tr>
                    <tr><td><code>price</code></td><td>✅</td><td>{dt("fullBhdExample")}</td></tr>
                    <tr><td><code>sku</code></td><td>–</td><td>{dt("stockKeepingUnit")}</td></tr>
                    <tr><td><code>barcodes</code></td><td>–</td><td>{dt("pipeSeparatedBarcodes")}</td></tr>
                    <tr><td><code>track_inventory</code></td><td>–</td><td>{dt("booleanDefaultTrue")}</td></tr>
                    <tr><td><code>tax_rule_name</code></td><td>–</td><td>{dt("existingTaxRule")}</td></tr>
                  </tbody>
                </table>
              ) : (
                <table className="bulk-format-table">
                  <thead><tr><th>{t("column")}</th><th>{t("required")}</th><th>{t("notesOptional")}</th></tr></thead>
                  <tbody>
                    <tr><td><code>name</code></td><td>✅</td><td>{dt("categoryNameColumn")}</td></tr>
                    <tr><td><code>sort_order</code></td><td>–</td><td>{dt("displayOrderNumber")}</td></tr>
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
              {previewRows.length} {t("rowsDetected")}
              {previewRows.length === 0 && ` — ${t("noValidRows")}`}
            </p>
            {error && <div className="bo-form-error">{error}</div>}
            {previewRows.length > 0 && (
              <div className="bulk-preview-scroll">
                <table className="bulk-preview-table">
                  <thead>
                    <tr>
                      {isProducts ? (
                        <>
                          <th>#</th><th>{t("name")}</th><th>{t("category")}</th><th>{t("price")}</th>
                          <th>{t("sku")}</th><th>{t("barcodes")}</th><th>{t("trackInventory")}</th>
                        </>
                      ) : (
                        <><th>#</th><th>{t("name")}</th><th>{t("sortOrder")}</th></>
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
                            <td>{(r as BulkProductRow).track_inventory === false ? dt("no") : dt("yes")}</td>
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
                <span className="icon-directional" aria-hidden="true">←</span> {t("back")}
              </button>
              <button
                className="btn-primary"
                onClick={runImport}
                disabled={importing || previewRows.length === 0}
              >
                {importing ? t("importing") : `${t("importAction")} ${previewRows.length}`}
              </button>
            </div>
          </div>
        )}

        {/* ── Step 3: Result ── */}
        {step === "result" && result && (
          <div className="bulk-result-area">
            <div className={`bulk-result-summary ${result.inserted > 0 ? "bulk-result-ok" : "bulk-result-warn"}`}>
              <span className="bulk-result-stat">
                ✅ <strong>{result.inserted}</strong> {t("inserted")}
              </span>
              {result.skipped > 0 && (
                <span className="bulk-result-stat">
                  ⏭ <strong>{result.skipped}</strong> {t("skipped")}
                </span>
              )}
              {result.errors.length > 0 && (
                <span className="bulk-result-stat bulk-result-err-count">
                  ❌ <strong>{result.errors.length}</strong> {t("errors")}
                </span>
              )}
            </div>

            {result.errors.length > 0 && (
              <div className="bulk-errors-box">
                <p className="bulk-errors-title">{t("errors")}:</p>
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
              <button className="btn-primary" onClick={onClose}>{t("done")}</button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
