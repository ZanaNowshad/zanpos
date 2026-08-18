import type { PurchaseOrderDetail } from "../../../types";
import { operationsTranslator, poStatusText } from "../../../i18n/operationsStrings";

type OperationsTranslator = ReturnType<typeof operationsTranslator>;
import type { Language } from "../../../hooks/useLanguage";
import { purchasingMoney } from "../../../officeai/purchasingPresentation";
import {
  PO_PIPELINE, PO_STATUS, isOpen, outstanding, poStatus, primaryActionFor,
} from "./poLifecycle";
import "./purchasing.css";

/**
 * Selected purchase order — the work area of the Purchasing workspace.
 *
 * Deliberately not a stack of cards: summary facts sit in a definition list and
 * lines in a plain table, so the detail reads as one object rather than four
 * nested panels. Receiving is NOT edited here yet; `onReceive` re-uses the
 * existing flow until the per-line editor lands in the next slice.
 */
interface Props {
  detail: PurchaseOrderDetail;
  language: Language;
  t: OperationsTranslator;
  currencyExp: number;
  busy: boolean;
  onReceive: (poId: string) => void;
  onCancel: (poId: string) => void;
  onClose: () => void;
}

export default function PoDetail({
  detail, language, t, currencyExp, busy, onReceive, onCancel, onClose,
}: Props) {
  const { order, lines } = detail;
  const status = poStatus(order.status);
  const meta = PO_STATUS[status];
  const primary = primaryActionFor(order.status);
  const open = isOpen(order.status);

  const orderedUnits = lines.reduce((n, l) => n + Number(l.ordered_qty), 0);
  const receivedUnits = lines.reduce((n, l) => n + Number(l.received_qty), 0);

  return (
    <aside className="zp-po-detail" aria-label={t("poDetail")}>
      <header className="zp-po-detail-head">
        <div className="zp-po-detail-ident">
          <span className="zp-po-detail-id">{order.po_id.slice(0, 8)}</span>
          <span className={`zp-status zp-status-${meta.tone}`}>
            {poStatusText(language, order.status)}
            {meta.step >= 0 && (
              <small className="zp-po-step">{meta.step + 1}/{PO_PIPELINE.length}</small>
            )}
          </span>
        </div>
        <button type="button" className="zp-po-detail-close" onClick={onClose}>
          {t("close")}
        </button>
      </header>

      <p className="zp-po-detail-supplier">{order.supplier_name ?? t("noSupplier")}</p>

      {/* Facts, not cards. */}
      <dl className="zp-po-facts">
        <div><dt>{t("expected")}</dt><dd>{order.expected_date ? new Date(order.expected_date).toLocaleDateString() : "—"}</dd></div>
        <div><dt>{t("lines")}</dt><dd className="zp-numeric">{order.line_count}</dd></div>
        <div><dt>{t("value")}</dt><dd className="zp-numeric">{purchasingMoney(order.ordered_total_minor ?? 0, currencyExp)}</dd></div>
        <div><dt>{t("received")}</dt><dd className="zp-numeric">{purchasingMoney(order.received_total_minor ?? 0, currencyExp)}</dd></div>
        <div><dt>{t("units")}</dt><dd className="zp-numeric">{receivedUnits} / {orderedUnits}</dd></div>
      </dl>

      {order.notes && <p className="zp-po-notes">{order.notes}</p>}

      <h3 className="zp-po-lines-title">{t("lineItems")}</h3>
      {lines.length === 0 ? (
        <p className="zp-po-empty">{t("noLines")}</p>
      ) : (
        <table className="zp-po-lines">
          <thead>
            <tr>
              <th scope="col">{t("productName")}</th>
              <th scope="col" className="zp-align-end">{t("ordered")}</th>
              <th scope="col" className="zp-align-end">{t("received")}</th>
              <th scope="col" className="zp-align-end">{t("remaining")}</th>
              <th scope="col" className="zp-align-end">{t("unitCost")}</th>
            </tr>
          </thead>
          <tbody>
            {lines.map(line => {
              const left = outstanding(line);
              return (
                <tr key={line.po_line_id}>
                  <td>{line.product_name}</td>
                  <td className="zp-align-end zp-numeric">{line.ordered_qty}</td>
                  <td className="zp-align-end zp-numeric">{line.received_qty}</td>
                  <td className={`zp-align-end zp-numeric${left > 0 ? " zp-status-warn" : ""}`}>{left}</td>
                  <td className="zp-align-end zp-numeric">{purchasingMoney(line.unit_cost_minor, currencyExp)}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}

      {/* Terminal orders are read-only: no Receive, no Cancel. */}
      {(primary === "receive" || open) && (
        <div className="zp-po-detail-actions">
          {primary === "receive" && (
            <button
              type="button"
              className="oa-primary-mini"
              onClick={() => onReceive(order.po_id)}
              disabled={busy || lines.length === 0}
            >
              {t("receive")}
            </button>
          )}
          {open && (
            <button type="button" className="oa-tool-btn" onClick={() => onCancel(order.po_id)} disabled={busy}>
              {t("cancel")}
            </button>
          )}
        </div>
      )}
    </aside>
  );
}
