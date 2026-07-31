import type { Cart } from "../../types";
import { DEVICE } from "../../types";
import { formatMoney } from "../../money";
import type { getExchangeBalance } from "../../utils/posExchange";
import Dialpad from "../Dialpad";
import type { ExchangeCredit } from "./posModalState";

interface Props {
  cart: Cart;
  numpadValue: string;
  recentLineId: string | null;
  taxTotal: number;
  payableTotal: number;
  exchangeCredit: ExchangeCredit | null;
  exchangeBalance: ReturnType<typeof getExchangeBalance> | null;
  payFastLoading: boolean;
  paymentStarted: boolean;
  onNumpadKey: (key: string) => void;
  onCancelExchange: () => void;
  onCompleteCoveredExchange: () => void;
  onPayFast: () => void;
  onPayDirect: (method: "cash" | "card" | "wallet") => void;
  onPaySplit: () => void;
}

export default function PosNumpadPanel({
  cart, numpadValue, recentLineId, taxTotal, payableTotal, exchangeCredit,
  exchangeBalance, payFastLoading, paymentStarted, onNumpadKey,
  onCancelExchange, onCompleteCoveredExchange, onPayFast, onPayDirect, onPaySplit,
}: Props) {
  const activeLines = cart.lines.filter(line => !line.voided);
  const grossTotal = activeLines.reduce((sum, line) => sum + line.line_total_minor, 0);
  const totalDiscount = cart.bill_discount_minor
    + activeLines.reduce((sum, line) => sum + line.line_discount_minor, 0);
  const fmt = (minor: number) =>
    `${DEVICE.currency} ${formatMoney(minor, DEVICE.currency_exponent)}`;
  const canPay = activeLines.length > 0 && payableTotal > 0
    && !(payFastLoading || paymentStarted);
  const canCompleteExchange = activeLines.length > 0
    && exchangeBalance?.amountDueMinor === 0
    && !payFastLoading;

  return (
    <div className="numpad-panel">
      <div className="numpad-top">
        <div className="numpad-display">
          <span key={numpadValue} className="numpad-multiplier">× {numpadValue}</span>
          {recentLineId && (
            <span className="numpad-recent-name">
              {cart.lines.find(line =>
                line.cart_line_id === recentLineId && !line.voided)?.product_name}
            </span>
          )}
        </div>
        <Dialpad onKey={onNumpadKey} />
      </div>

      <div className="np-pay-section">
        <div className="np-totals">
          <div className="np-total-row"><span>Subtotal</span><span>{fmt(grossTotal)}</span></div>
          {totalDiscount > 0
            ? <div className="np-total-row np-discount-active"><span>Discount</span><span>−{fmt(totalDiscount)}</span></div>
            : <div className="np-total-row np-discount-zero"><span>Discount</span><span>{fmt(0)}</span></div>
          }
          <div className="np-total-row"><span>Tax</span><span>{fmt(taxTotal)}</span></div>
          {exchangeBalance && (
            <>
              <div className="np-total-row np-exchange-credit"><span>Exchange credit</span><span>−{fmt(exchangeBalance.appliedCreditMinor)}</span></div>
              {exchangeBalance.refundDueMinor > 0 && (
                <div className="np-total-row np-exchange-refund"><span>Refund due</span><span>{fmt(exchangeBalance.refundDueMinor)}</span></div>
              )}
            </>
          )}
          <div className="np-total-row np-grand" aria-live="polite" aria-atomic="true">
            <span>{exchangeBalance ? "DUE" : "TOTAL"}</span><span>{fmt(payableTotal)}</span>
          </div>
        </div>

        {exchangeCredit && (
          <div className="pos-exchange-strip">
            <span>Exchange from {exchangeCredit.originalReceipt}</span>
            <button onClick={onCancelExchange}>Cancel</button>
          </div>
        )}

        {canCompleteExchange ? (
          <button className="np-fast-cash-btn" onClick={onCompleteCoveredExchange}>
            <span>{exchangeBalance.refundDueMinor > 0
              ? "Refund difference & complete"
              : "Complete Exchange"}</span>
            <span className="np-fast-total">{exchangeBalance.refundDueMinor > 0
              ? fmt(exchangeBalance.refundDueMinor)
              : fmt(0)}</span>
          </button>
        ) : (
          <button
            className="np-fast-cash-btn"
            disabled={!canPay || payFastLoading}
            onClick={onPayFast}
            title={!canPay ? "Add items to pay" : "Fast Cash · F12"}
          >
            {payFastLoading
              ? "…"
              : <><span>Fast Cash <kbd>F12</kbd></span><span className="np-fast-total">{fmt(payableTotal)}</span></>}
          </button>
        )}

        <div className="np-methods">
          <button className="np-method-btn" disabled={!canPay} onClick={() => onPayDirect("cash")}>Cash</button>
          <button className="np-method-btn" disabled={!canPay} onClick={() => onPayDirect("card")}>Card</button>
          <button className="np-method-btn" disabled={!canPay} onClick={() => onPayDirect("wallet")}>Wallet</button>
          <button className="np-method-btn np-split-btn" disabled={!canPay} onClick={onPaySplit}>Split</button>
        </div>
      </div>
    </div>
  );
}
