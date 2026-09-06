import { useMemo, useState } from "react";
import type { CatalogImportProposal, CatalogApplyLine, CatalogApplyResult, SessionToken } from "../types";
import { DEVICE } from "../types";
import { catalogImportApply } from "../tauri/commands";
import { useLanguage } from "../hooks/useLanguage";
import { countText, operationsTranslator } from "../i18n/operationsStrings";
import ModalShell from "./modal/ModalShell";
import { ModalActions, ModalError } from "./modal/ModalParts";

interface Props {
  proposal: CatalogImportProposal;
  sessionToken: SessionToken;
  onClose: () => void;
  onApplied: () => void;
}

interface Row {
  line_no: number;
  include: boolean;
  action: "update" | "create";
  product_id: string | null;
  name: string;
  barcode: string | null;
  matched_name: string | null;
  current_price_minor: number | null;
  new_price_major: string;
  current_cost_minor: number | null;
  new_cost_major: string;
  receive_qty: string;
}

const toMajor = (minor: number | null, e: number): string =>
  minor == null ? "" : (minor / Math.pow(10, e)).toFixed(e);
const toMinor = (major: string, e: number): number | null => {
  const t = major.trim();
  if (t === "") return null;
  const v = parseFloat(t);
  return Number.isNaN(v) ? null : Math.round(v * Math.pow(10, e));
};

/**
 * Review-first catalog import. Shows every proposed change extracted from an
 * invoice/price-list photo; nothing is written until the owner presses Apply.
 */
export default function CatalogImportModal({ proposal, sessionToken, onClose, onApplied }: Props) {
  const { language } = useLanguage();
  const t = operationsTranslator(language);
  const exp = proposal.currency_exponent;
  const cur = DEVICE.currency;

  const [rows, setRows] = useState<Row[]>(() =>
    proposal.lines.map(l => ({
      line_no: l.line_no,
      include: true,
      action: l.action,
      product_id: l.matched_product_id,
      name: l.name,
      barcode: l.barcode,
      matched_name: l.matched_name,
      current_price_minor: l.current_price_minor,
      new_price_major: toMajor(l.new_price_minor, exp),
      current_cost_minor: l.current_cost_minor,
      new_cost_major: toMajor(l.new_cost_minor, exp),
      receive_qty: l.quantity != null ? String(l.quantity) : "",
    })),
  );
  const [applying, setApplying] = useState(false);
  const [result, setResult]     = useState<CatalogApplyResult | null>(null);
  const [error, setError]       = useState<string | null>(null);

  const set = (i: number, patch: Partial<Row>) =>
    setRows(prev => prev.map((r, idx) => (idx === i ? { ...r, ...patch } : r)));

  const includedCount = useMemo(() => rows.filter(r => r.include).length, [rows]);

  const apply = async () => {
    setApplying(true);
    setError(null);
    try {
      const lines: CatalogApplyLine[] = rows
        .filter(r => r.include)
        .map(r => ({
          action: r.action,
          product_id: r.product_id,
          name: r.name,
          barcode: r.barcode,
          new_price_minor: toMinor(r.new_price_major, exp),
          new_cost_minor: toMinor(r.new_cost_major, exp),
          receive_qty: r.receive_qty.trim() === "" ? null : r.receive_qty.trim(),
        }));
      const res = await catalogImportApply(
        { currency_exponent: exp, supplier_name: proposal.supplier_name, supplier_id: proposal.supplier_id, lines },
        sessionToken,
      );
      setResult(res);
      onApplied();
    } catch (e) {
      setError(typeof e === "string" ? e : t("catalogApplyFailed"));
    } finally {
      setApplying(false);
    }
  };

  /* One footer for both states. The result screen used to put Done inside its
     own block and the review screen put Apply inside another, so the button
     moved down the dialog as the table grew. */
  const footer = result ? (
    <ModalActions note={t("inactiveDraftNote")}>
      <button type="button" className="btn-primary" onClick={onClose}>{t("done")}</button>
    </ModalActions>
  ) : (
    <ModalActions
      note={proposal.supplier_name
        ? `${t("supplier")}: ${proposal.supplier_name}${proposal.supplier_id ? "" : ` (${t("newSupplier")})`}`
        : undefined}
    >
      <button type="button" className="btn-secondary" onClick={onClose} disabled={applying}>
        {t("cancel")}
      </button>
      <button
        type="button"
        className="btn-primary"
        onClick={apply}
        disabled={applying || includedCount === 0}
      >
        {applying ? t("applying") : `${t("apply")} ${countText(language, "changes", includedCount)}`}
      </button>
    </ModalActions>
  );

  return (
    <ModalShell
      kicker="Purchasing"
      title={t("catalogReviewTitle")}
      subtitle={result ? undefined : `${countText(language, "lines", rows.length)} ${t("found")}`}
      size="xl"
      className="catalog-import-modal"
      onClose={onClose}
      footer={footer}
    >
      <>

        {result ? (
          <div className="catalog-import-result">
            <p className="ci-result-line">✅ {t("applied")}: <strong>{result.updated}</strong> {t("updated")} · <strong>{result.created}</strong> {t("newDraft")} · <strong>{result.received}</strong> {t("received")}</p>
            {result.supplier_id && <p className="ci-result-sub">{t("supplierLinked")}</p>}
            {result.errors.length > 0 && (
              <div className="ci-result-errors">
                <p>{countText(language, "lines", result.errors.length)} {t("issues")}:</p>
                <ul>{result.errors.map((er, i) => <li key={i}>{er}</li>)}</ul>
              </div>
            )}
          </div>
        ) : (
          <>
            <ModalError message={error} />

            <div className="catalog-import-body">
              <table className="ci-table">
                <thead>
                  <tr>
                    <th></th><th>{t("item")}</th><th>{t("status")}</th>
                    <th>{t("price")} ({cur})</th><th>{t("cost")} ({cur})</th><th>{t("receiveQty")}</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((r, i) => (
                    <tr key={r.line_no} className={r.include ? "" : "ci-row-excluded"}>
                      <td>
                        <input type="checkbox" checked={r.include} onChange={e => set(i, { include: e.target.checked })} aria-label={t("includeLine")} />
                      </td>
                      <td>
                        <div className="ci-item-name">{r.name || "—"}</div>
                        {r.matched_name && r.matched_name !== r.name && (
                          <div className="ci-item-match">↳ {r.matched_name}</div>
                        )}
                        {r.barcode && <div className="ci-item-barcode">#{r.barcode}</div>}
                      </td>
                      <td>
                        {r.action === "create"
                          ? <span className="ci-badge ci-badge-new">{t("newBadge")}</span>
                          : <span className="ci-badge ci-badge-match">{t("matchBadge")}</span>}
                      </td>
                      <td>
                        {r.current_price_minor != null && (
                          <span className="ci-old numeric-ltr">
                            {toMajor(r.current_price_minor, exp)} <span className="icon-directional" aria-hidden="true">→</span>
                          </span>
                        )}
                        <input className="ci-input" inputMode="decimal" value={r.new_price_major}
                          onChange={e => set(i, { new_price_major: e.target.value })} placeholder="—" />
                      </td>
                      <td>
                        {r.current_cost_minor != null && (
                          <span className="ci-old numeric-ltr">
                            {toMajor(r.current_cost_minor, exp)} <span className="icon-directional" aria-hidden="true">→</span>
                          </span>
                        )}
                        <input className="ci-input" inputMode="decimal" value={r.new_cost_major}
                          onChange={e => set(i, { new_cost_major: e.target.value })} placeholder="—" />
                      </td>
                      <td>
                        <input className="ci-input" inputMode="decimal" value={r.receive_qty}
                          onChange={e => set(i, { receive_qty: e.target.value })} placeholder="0" />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>

          </>
        )}
      </>
    </ModalShell>
  );
}
