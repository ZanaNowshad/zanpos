import { Check, X } from "lucide-react";
import { DEVICE, type PaymentInput } from "../types";
import type { DetailStringKey } from "../i18n/detailStrings";
import type { ActiveField, PaymentLine } from "./paymentLines";

/**
 * The two blocks of the payment modal that only exist when money is being
 * counted at the counter: the tender/change group, and the split lines.
 *
 * Lifted out of PaymentModal, which had grown past the 500-line limit the ship
 * gate enforces. They are the right pieces to move because both are gated on
 * conditions nothing else in the modal shares — cash taken now, and a split in
 * progress — so neither is on the path of a card, delivery or digital sale.
 */

interface CashProps {
  mainLine: PaymentLine;
  activeField: ActiveField;
  onFocusTendered: () => void;
  change: number;
  quickAmts: number[];
  onQuick: (amount: number) => void;
  netTotal: number;
  fmt: (minor: number) => string;
  dt: (key: DetailStringKey) => string;
}

/** Cash received, change due, and the quick-tender buttons. */
export function PaymentCashTender({
  mainLine, activeField, onFocusTendered, change, quickAmts, onQuick, netTotal, fmt, dt,
}: CashProps) {
  return (
    <>
      <div
        role="button"
        tabIndex={0}
        onKeyDown={e => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}
        className={`pm-amount-box pm-tendered-box${activeField?.kind === "tendered" ? " pm-field-active" : ""}`}
        onClick={onFocusTendered}
      >
        <span className="pm-amount-label">{dt("cashReceived")}</span>
        <span className="pm-amount-value">
          {mainLine.tenderedStr || mainLine.amountStr || "0"}
          {activeField?.kind === "tendered" && <span className="pm-cursor">|</span>}
        </span>
      </div>

      {/* Change is the number the cashier reads aloud and counts back, so it
          carries the same weight as the total rather than sitting in a pill. */}
      <div className={`pm-change${change > 0 ? " pm-change-live" : ""}`} aria-live="polite" aria-atomic="true">
        <span className="pm-change-label"><Check size={15} aria-hidden="true" /> {dt("changeDue")}</span>
        <span className="pm-change-value">
          <span className="pm-due-cur">{DEVICE.currency}</span>{fmt(change)}
        </span>
      </div>

      <div className="pm-quick">
        <button className="pm-quick-btn pm-quick-exact" onClick={() => onQuick(netTotal)}>
          {dt("exact")} <kbd>E</kbd>
        </button>
        {quickAmts.slice(0, 3).map(a => (
          <button key={a} className="pm-quick-btn" onClick={() => onQuick(a)}>
            {DEVICE.currency} {fmt(a)}
          </button>
        ))}
      </div>
    </>
  );
}

interface SplitProps {
  lines: PaymentLine[];
  activeField: ActiveField;
  remainingMinor: number;
  onChangeMethod: (id: number, method: PaymentInput["method"]) => void;
  onFocusAmount: (id: number) => void;
  onRemove: (id: number) => void;
  fmt: (minor: number) => string;
  dt: (key: DetailStringKey) => string;
}

/** The additional tenders of a split, and what is still owed across them. */
export function PaymentSplitLines({
  lines, activeField, remainingMinor, onChangeMethod, onFocusAmount, onRemove, fmt, dt,
}: SplitProps) {
  return (
    <>
      {lines.length > 1 && (
        <div className="pm-split-section">
          {lines.slice(1).map(line => (
            <div key={line.id} className="pm-split-line">
              <select
                className="pm-split-method"
                value={line.method}
                onChange={e => onChangeMethod(line.id, e.target.value as PaymentInput["method"])}
              >
                <option value="card">{dt("card")}</option>
                <option value="cash">{dt("cash")}</option>
                <option value="wallet">{dt("wallet")}</option>
              </select>
              <div
                role="button"
                tabIndex={0}
                onKeyDown={e => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}
                className={`pm-amount-box pm-split-amount${activeField?.kind === "amount" && activeField.lineId === line.id ? " pm-field-active" : ""}`}
                onClick={() => onFocusAmount(line.id)}
              >
                <span className="pm-amount-value" style={{ fontSize: "1rem" }}>
                  {line.amountStr || "0"}
                  {activeField?.kind === "amount" && activeField.lineId === line.id && <span className="pm-cursor">|</span>}
                </span>
              </div>
              <button className="pm-split-remove" onClick={() => onRemove(line.id)} aria-label={dt("removeSplitPayment")}>
                <X size={15} />
              </button>
            </div>
          ))}
          <div className="pm-split-remaining">
            {dt("remaining")}: {DEVICE.currency} {fmt(remainingMinor)}
          </div>
        </div>
      )}

      <div className={`pm-remaining${remainingMinor < 0 ? " pm-remaining-change" : remainingMinor === 0 ? " pm-remaining-ok" : ""}`}>
        {remainingMinor > 0 ? `${dt("due")}: ${DEVICE.currency} ${fmt(remainingMinor)}` :
         remainingMinor < 0 ? `${dt("changeDue")}: ${DEVICE.currency} ${fmt(-remainingMinor)}` :
         `✓ ${dt("fullyPaid")}`}
      </div>
    </>
  );
}
