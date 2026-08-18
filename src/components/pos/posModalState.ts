import type { PaymentInput } from "../../types";
import type { PaymentJourney } from "../PaymentModal";

export type ActiveModal =
  | { kind: "none" }
  | { kind: "payment"; journey: PaymentJourney; method?: PaymentInput["method"]; split: boolean }
  | { kind: "shiftClose" }
  | { kind: "hold" }
  | { kind: "refund" }
  | { kind: "report" }
  | { kind: "discount"; lineId?: string }
  | { kind: "customItem" }
  | { kind: "cashEvent" }
  | { kind: "clearConfirm" }
  | { kind: "help" }
  | { kind: "recent" }
  | { kind: "priceInput"; mode: "setExisting"; lineId: string; productName: string; currentPriceMinor: number }
  | { kind: "priceInput"; mode: "addNew"; itemName: string; quantity: string };

export interface ExchangeCredit {
  refundId: string;
  creditMinor: number;
  refundReceipt: string;
  originalReceipt: string;
}
