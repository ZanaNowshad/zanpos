import type { PaymentInput } from "../../types";

export type ActiveModal =
  | { kind: "none" }
  | { kind: "payment"; method?: PaymentInput["method"]; split: boolean }
  | { kind: "shiftClose" }
  | { kind: "hold" }
  | { kind: "refund" }
  | { kind: "report" }
  | { kind: "discount" }
  | { kind: "lineDiscount"; lineId: string }
  | { kind: "customItem" }
  | { kind: "cashEvent" }
  | { kind: "xReport" }
  | { kind: "clearConfirm" }
  | { kind: "help" }
  | { kind: "recent" }
  | { kind: "priceInput"; mode: "setExisting"; lineId: string; productName: string }
  | { kind: "priceInput"; mode: "addNew"; itemName: string; quantity: string };

export interface ExchangeCredit {
  refundId: string;
  creditMinor: number;
  refundReceipt: string;
  originalReceipt: string;
}
