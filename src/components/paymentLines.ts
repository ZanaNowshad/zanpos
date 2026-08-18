import type { PaymentInput } from "../types";

/**
 * The modal's own model of a tender line, and where the caret is.
 *
 * `amountStr` and `tenderedStr` stay strings the whole way through: they are
 * what the cashier typed, and parsing them to minor units happens once, at the
 * boundary, in paymentValidation. Rounding a partially-typed amount on every
 * keystroke is how "2.0" becomes 2.000 before the 5 arrives.
 */
export interface PaymentLine {
  id: number;
  method: PaymentInput["method"];
  amountStr: string;
  tenderedStr: string;
}

export type ActiveField =
  | { kind: "amount";   lineId: number }
  | { kind: "tendered"; lineId: number }
  | { kind: "phone" }
  | null;

let lineIdCounter = 200;
export const mkLine = (method: PaymentInput["method"] = "cash"): PaymentLine =>
  ({ id: lineIdCounter++, method, amountStr: "", tenderedStr: "" });
