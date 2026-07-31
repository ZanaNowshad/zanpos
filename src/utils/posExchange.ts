export interface ExchangeBalance {
  creditMinor: number;
  replacementMinor: number;
  appliedCreditMinor: number;
  amountDueMinor: number;
  refundDueMinor: number;
}

export function getExchangeBalance(creditMinor: number, replacementMinor: number): ExchangeBalance {
  const credit = Math.max(0, Math.round(creditMinor));
  const replacement = Math.max(0, Math.round(replacementMinor));
  const applied = Math.min(credit, replacement);
  return {
    creditMinor: credit,
    replacementMinor: replacement,
    appliedCreditMinor: applied,
    amountDueMinor: Math.max(0, replacement - credit),
    refundDueMinor: Math.max(0, credit - replacement),
  };
}

export function buildExchangePayments(
  exchange: { refundId: string; creditMinor: number } | null,
  duePayments: PaymentInput[],
  replacementMinor: number,
): PaymentInput[] {
  if (!exchange) return duePayments;
  const credit = Math.min(
    Math.max(0, Math.round(exchange.creditMinor)),
    Math.max(0, Math.round(replacementMinor)),
  );
  if (credit === 0) return duePayments;
  return [
    {
      method: "exchange_credit",
      amount_minor: credit,
      external_reference: exchange.refundId,
    },
    ...duePayments,
  ];
}
import type { PaymentInput } from "../types";
