import { type DragEvent, useMemo, useRef, useState } from "react";
import type { BulkImportResult, BulkCategoryRow, BulkProductRow } from "../tauri/commands";
import * as cmd from "../tauri/commands";
import { useLanguage } from "../hooks/useLanguage";
import { modalTranslator } from "../i18n/modalStrings";
import { detailTranslator } from "../i18n/detailStrings";
import {
  CATEGORY_TEMPLATE_CSV, PRODUCT_TEMPLATE_CSV,
  downloadCsv, parseCSV, rowsToCategories, rowsToProducts,
} from "../csv/productsCsv";
import ModalShell from "./modal/ModalShell";
import { ModalActions, ModalError, ModalSteps } from "./modal/ModalParts";
import type { SessionToken } from "../types";

// ── Types ─────────────────────────────────────────────────────────────────────

type Mode = "products" | "categories";

interface Props {
  mode: Mode;
  sessionToken: SessionToken;
  onClose: () => void;
  onDone: () => void; // refresh parent list after import
}

// ── Component ─────────────────────────────────────────────────────────────────

export default function BulkImportModal({ mode, sessionToken, onClose, onDone }: Props) {
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

  function downloadTemplate() {
    downloadCsv(
      isProducts ? "products_template.csv" : "categories_template.csv",
      isProducts ? PRODUCT_TEMPLATE_CSV : CATEGORY_TEMPLATE_CSV,
    );
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
        res = await cmd.adminBulkImportProducts(previewRows as BulkProductRow[], sessionToken);
      } else {
        res = await cmd.adminBulkImportCategories(previewRows as BulkCategoryRow[], sessionToken);
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

  /* Actions live in the footer, one place, whichever step is showing. They
     used to sit at the bottom of each step's own markup, so Back and Import
     appeared in a different position from Done and moved as the preview table
     grew. */
  const stepIndex = step === "upload" ? 0 : step === "preview" ? 1 : 2;
  const footer =
    step === "upload" ? (
      <ModalActions note="A template with the right columns is one click away.">
        <button type="button" className="btn-secondary" onClick={onClose}>{t("cancel")}</button>
      </ModalActions>
    ) : step === "preview" ? (
      <ModalActions note={previewRows.length === 0 ? t("noValidRows") : `${previewRows.length} ${t("rowsDetected")}`}>
        <button type="button" className="btn-secondary" onClick={() => { setStep("upload"); setError(null); }}>
          {t("back")}
        </button>
        <button
          type="button"
          className="btn-primary"
          onClick={runImport}
          disabled={importing || previewRows.length === 0}
        >
          {importing ? t("importing") : `${t("importAction")} ${previewRows.length}`}
        </button>
      </ModalActions>
    ) : (
      <ModalActions>
        <button
          type="button"
          className="btn-secondary"
          onClick={() => { setStep("upload"); setParsed([]); setResult(null); }}
        >
          Import another file
        </button>
        <button type="button" className="btn-primary" onClick={onClose}>{t("done")}</button>
      </ModalActions>
    );

  return (
    <ModalShell
      kicker={isProducts ? "Catalogue" : "Categories"}
      title={isProducts ? t("bulkImportProducts") : t("bulkImportCategories")}
      subtitle="Bring a spreadsheet in. Nothing is written until you have seen the preview."
      size="lg"
      onClose={onClose}
      footer={footer}
    >
      <ModalSteps steps={["Upload", "Review", "Done"]} current={stepIndex} />
      <div className="bulk-modal">

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
              role="button"
              tabIndex={0}
              onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); fileRef.current?.click(); } }}
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
            <ModalError message={error} />
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

          </div>
        )}
      </div>
    </ModalShell>
  );
}
