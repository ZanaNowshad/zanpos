import { ChevronRight, MessageCircle, MoreHorizontal, ReceiptText, Truck, Wallet } from "lucide-react";
import type { Cart } from "../../types";
import { DEVICE } from "../../types";
import { formatMoney } from "../../money";
import type { getExchangeBalance } from "../../utils/posExchange";
import type { PaymentJourney } from "../PaymentModal";
import type { ExchangeCredit } from "./posModalState";

interface Props {
  cart: Cart;
  taxTotal: number;
  payableTotal: number;
  exchangeCredit: ExchangeCredit | null;
  exchangeBalance: ReturnType<typeof getExchangeBalance> | null;
  payFastLoading: boolean;
  paymentStarted: boolean;
  onCancelExchange: () => void;
  onCompleteCoveredExchange: () => void;
  onPayFast: () => void;
  onOpenPaymentJourney: (journey: PaymentJourney) => void;
  onOpenMore: () => void;
}

/**
 * Totals and tender, down the right of the till.
 *
 * Replaces the old numpad panel. The dial pad that used to live here was
 * resident — roughly a third of a 1024px screen held for an input a scanner
 * makes unnecessary on most lines. It is now summoned by the field that needs
 * it (quantity, price, cash tendered) and appears in this same column, so the
 * muscle memory survives without the screen being spent on it.
 *
 * Everything that is not the money path moved behind `More` rather than being
 * removed: hold, refund, cash events, deliveries, recent sales, reprint.
 */
export default function PosTotalsPanel({
  cart, taxTotal, payableTotal, exchangeCredit, exchangeBalance,
  payFastLoading, paymentStarted, onCancelExchange, onCompleteCoveredExchange,
  onPayFast, onOpenPaymentJourney, onOpenMore,
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
    <aside className="till-tender" aria-label="Totals and payment">
      <div className="till-totals">
        <div className="till-total-row"><span>Subtotal</span><span>{fmt(grossTotal)}</span></div>
        {totalDiscount > 0
          ? <div className="till-total-row till-discount-active"><span>Discount</span><span>−{fmt(totalDiscount)}</span></div>
          : <div className="till-total-row till-discount-zero"><span>Discount</span><span>{fmt(0)}</span></div>}
        <div className="till-total-row"><span>Tax</span><span>{fmt(taxTotal)}</span></div>
        {exchangeBalance && (
          <>
            <div className="till-total-row till-exchange-credit">
              <span>Exchange credit</span><span>−{fmt(exchangeBalance.appliedCreditMinor)}</span>
            </div>
            {exchangeBalance.refundDueMinor > 0 && (
              <div className="till-total-row till-exchange-refund">
                <span>Refund due</span><span>{fmt(exchangeBalance.refundDueMinor)}</span>
              </div>
            )}
          </>
        )}
        <div className="till-grand" aria-live="polite" aria-atomic="true">
          <span className="till-grand-label">{exchangeBalance ? "DUE" : "TOTAL"}</span>
          <span className="till-grand-value">
            <span className="till-grand-cur">{DEVICE.currency}</span>
            {formatMoney(payableTotal, DEVICE.currency_exponent)}
          </span>
        </div>
      </div>

      {exchangeCredit && (
        <div className="till-exchange-strip">
          <span>Exchange from {exchangeCredit.originalReceipt}</span>
          <button type="button" onClick={onCancelExchange}>Cancel</button>
        </div>
      )}

      {canCompleteExchange ? (
        <button type="button" className="till-fastcash" onClick={onCompleteCoveredExchange}>
          <span className="till-fastcash-icon"><Wallet size={22} aria-hidden="true" /></span>
          <span className="till-fastcash-label">
            {exchangeBalance.refundDueMinor > 0 ? "Refund & Complete" : "Complete Exchange"}
          </span>
          <span className="till-key">
            {exchangeBalance.refundDueMinor > 0 ? fmt(exchangeBalance.refundDueMinor) : fmt(0)}
          </span>
        </button>
      ) : (
        <button
          type="button"
          className="till-fastcash"
          disabled={!canPay}
          onClick={onPayFast}
          title={!canPay ? "Add items to pay" : "Fast Cash · F12"}
        >
          <span className="till-fastcash-icon"><Wallet size={22} aria-hidden="true" /></span>
          <span className="till-fastcash-label">{payFastLoading ? "…" : "Fast Cash"}</span>
          <span className="till-key">F12</span>
        </button>
      )}

      <div className="till-journeys" role="group" aria-label="Checkout type">
        <button
          type="button"
          className="till-journey till-journey-receipt"
          disabled={!canPay}
          onClick={() => onOpenPaymentJourney("receipt")}
        >
          <span className="till-journey-icon"><ReceiptText size={20} aria-hidden="true" /></span>
          <span className="till-journey-text">
            <strong>Receipt</strong>
            <small>Counter sale</small>
          </span>
          <span className="till-key">F6</span>
        </button>
        <button
          type="button"
          className="till-journey till-journey-delivery"
          disabled={!canPay}
          onClick={() => onOpenPaymentJourney("delivery")}
        >
          <span className="till-journey-icon"><Truck size={20} aria-hidden="true" /></span>
          <span className="till-journey-text">
            <strong>Delivery</strong>
            <small>Address</small>
          </span>
          <span className="till-key">F7</span>
        </button>
        <button
          type="button"
          className="till-journey till-journey-digital"
          disabled={!canPay}
          onClick={() => onOpenPaymentJourney("digital")}
        >
          <span className="till-journey-icon"><MessageCircle size={20} aria-hidden="true" /></span>
          <span className="till-journey-text">
            <strong>Digital</strong>
            <small>Card / Mobile / Wallet</small>
          </span>
          <span className="till-key">F8</span>
        </button>
      </div>

      <button type="button" className="till-more" onClick={onOpenMore}>
        <MoreHorizontal size={20} aria-hidden="true" />
        <span>More Options</span>
        <span className="till-key">F9</span>
        <ChevronRight size={18} className="icon-directional" aria-hidden="true" />
      </button>
    </aside>
  );
}
