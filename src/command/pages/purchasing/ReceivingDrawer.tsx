import { useMemo, useState } from "react";
import { AlertTriangle } from "lucide-react";
import type { PurchaseOrderDetail, ReceivePurchaseOrderResult } from "../../../types";
import { operationsTranslator } from "../../../i18n/operationsStrings";
import { Drawer } from "../../../components/templates";
import {
  fillAllRemaining, hasReceivableInput, outstanding, receivePayload, validateReceiveQty,
  type QtyError,
} from "./poLifecycle";
import "./purchasing.css";

type Tr = ReturnType<typeof operationsTranslator>;

/**
 * Per-line receiving editor.
 *
 * Every rule here mirrors `po_receive_inner`; see poLifecycle.ts for the traced
 * contract. Notably zero means "not on this receipt" rather than an error, so
 * a partially-arrived delivery is entered by zeroing the lines that did not
 * turn up rather than by fighting a validator.
 *
 * Client validation is a courtesy: the backend re-validates everything, and its
 * errors are surfaced verbatim.
 */
interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  detail: PurchaseOrderDetail;
  t: Tr;
  submitting: boolean;
  error: string | null;
  onSubmit: (lines: { po_line_id: string; received_qty: string }[]) => Promise<ReceivePurchaseOrderResult | null>;
}

const ERROR_KEY: Record<QtyError, string> = {
  "not-a-number": "qtyNotNumber",
  negative: "qtyNegative",
  "exceeds-remaining": "qtyExceedsRemaining",
};

export default function ReceivingDrawer({
  open, onOpenChange, detail, t, submitting, error, onSubmit,
}: Props) {
  const { lines } = detail;
  // Prefilled with everything outstanding — the common case is "it all arrived".
  const [entries, setEntries] = useState<Record<string, string>>(() => fillAllRemaining(lines));

  const errors = useMemo(() => {
    const out: Record<string, QtyError> = {};
    for (const line of lines) {
      const e = validateReceiveQty(entries[line.po_line_id] ?? "", line);
      if (e) out[line.po_line_id] = e;
    }
    return out;
  }, [entries, lines]);

  const hasErrors = Object.keys(errors).length > 0;
  const canSubmit = !submitting && !hasErrors && hasReceivableInput(entries, lines);
  const totalUnits = receivePayload(entries, lines)
    .reduce((n, l) => n + Number(l.received_qty), 0);

  async function submit() {
    const result = await onSubmit(receivePayload(entries, lines));
    // Only close on a confirmed backend success; a failure keeps the entered
    // quantities so the user does not retype a whole delivery.
    if (result) onOpenChange(false);
  }

  return (
    <Drawer
      open={open}
      onOpenChange={onOpenChange}
      title={t("receiveStock")}
      description={t("receiveStockHint")}
      width="lg"
      footer={
        <>
          <button className="oa-tool-btn" onClick={() => onOpenChange(false)} disabled={submitting}>
            {t("cancel")}
          </button>
          <button className="oa-primary-mini" onClick={submit} disabled={!canSubmit}>
            {submitting ? t("receiving") : t("receiveStock")}
          </button>
        </>
      }
    >
      {error && (
        <div className="zp-po-exceptions" role="alert">
          <AlertTriangle size={15} aria-hidden="true" />
          <span>{error}</span>
        </div>
      )}

      <div className="zp-receive-tools">
        <button className="oa-tool-btn" onClick={() => setEntries(fillAllRemaining(lines))} disabled={submitting}>
          {t("receiveAllRemaining")}
        </button>
        <button
          className="oa-tool-btn"
          onClick={() => setEntries(Object.fromEntries(lines.map(l => [l.po_line_id, "0"])))}
          disabled={submitting}
        >
          {t("clearQuantities")}
        </button>
      </div>

      <table className="zp-receive-table">
        <thead>
          <tr>
            <th scope="col">{t("productName")}</th>
            <th scope="col" className="zp-align-end">{t("ordered")}</th>
            <th scope="col" className="zp-align-end">{t("received")}</th>
            <th scope="col" className="zp-align-end">{t("remaining")}</th>
            <th scope="col" className="zp-align-end">{t("receiveNow")}</th>
          </tr>
        </thead>
        <tbody>
          {lines.map(line => {
            const left = outstanding(line);
            const err = errors[line.po_line_id];
            const inputId = `recv-${line.po_line_id}`;
            return (
              <tr key={line.po_line_id} className={err ? "has-error" : undefined}>
                <td>
                  <label htmlFor={inputId} className="zp-receive-name">{line.product_name}</label>
                  {/* The message sits with the field that caused it. */}
                  {err && (
                    <span className="zp-receive-error" id={`${inputId}-err`}>{t(ERROR_KEY[err] as never)}</span>
                  )}
                </td>
                <td className="zp-align-end zp-numeric">{line.ordered_qty}</td>
                <td className="zp-align-end zp-numeric">{line.received_qty}</td>
                <td className={`zp-align-end zp-numeric${left > 0 ? " zp-status-warn" : ""}`}>{left}</td>
                <td className="zp-align-end">
                  <input
                    id={inputId}
                    className="zp-receive-input"
                    inputMode="decimal"
                    value={entries[line.po_line_id] ?? ""}
                    disabled={submitting || left <= 0}
                    aria-invalid={err ? true : undefined}
                    aria-describedby={err ? `${inputId}-err` : undefined}
                    onChange={e => setEntries(prev => ({ ...prev, [line.po_line_id]: e.target.value }))}
                  />
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>

      <p className="zp-receive-summary">
        {t("receivingNowTotal")} <strong className="zp-numeric">{totalUnits}</strong>
        {" · "}{t("receiveUpdatesStock")}
      </p>
    </Drawer>
  );
}
