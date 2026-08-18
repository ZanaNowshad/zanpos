import type { Language } from "../hooks/useLanguage";
import { EN, type OperationsStringKey } from "./operationsStrings.en";
import { AR } from "./operationsStrings.ar";

export type { OperationsStringKey };
export const OPERATIONS_STRING_KEYS = Object.keys(EN) as OperationsStringKey[];
const TABLES: Record<Language, Record<OperationsStringKey, string>> = { en: EN, ar: AR };
export function operationsText(language: Language, key: OperationsStringKey): string {
  return TABLES[language][key];
}
export const operationsTranslator = (language: Language) => (key: OperationsStringKey) =>
  operationsText(language, key);
const PO_STATUS_KEYS: Record<string, OperationsStringKey> = {
  draft: "poStatusDraft",
  ordered: "poStatusOrdered",
  partial: "poStatusPartial",
  received: "poStatusReceived",
  cancelled: "poStatusCancelled",
} as const;
export function poStatusText(language: Language, status: string): string {
  const key = PO_STATUS_KEYS[status];
  if (key) return operationsText(language, key);
  return status;
}
const ROLE_KEYS: Record<string, OperationsStringKey> = {
  owner: "roleOwner",
  manager: "roleManager",
  cashier: "roleCashier",
  accountant: "roleAccountant",
};
export function roleText(language: Language, role: string): string {
  const key = ROLE_KEYS[role];
  if (key) return operationsText(language, key);
  return role;
}
export function deviceStatusText(language: Language, active: boolean): string {
  return operationsText(language, active ? "active" : "inactive");
}
const HUB_TRUTH_STATUS_KEYS: Record<string, OperationsStringKey> = {
  match: "match",
  missing_on_hub: "missingOnHub",
  count_mismatch: "countMismatch",
  checksum_mismatch: "checksumMismatch",
};
export function hubTruthStatusText(language: Language, status: string): string {
  const key = HUB_TRUTH_STATUS_KEYS[status];
  if (key) return operationsText(language, key);
  return status;
}
type CountKind = "points" | "lines" | "items" | "suppliers" | "products" | "pos" | "units" | "changes" | "costUpdates" | "customers";
const EN_COUNTS: Record<CountKind, [string, string]> = {
  points: ["point", "points"], lines: ["line", "lines"], items: ["item", "items"],
  suppliers: ["supplier", "suppliers"],
  products: ["product", "products"], pos: ["PO", "POs"], units: ["unit", "units"],
  changes: ["change", "changes"], costUpdates: ["cost update", "cost updates"],
  customers: ["customer", "customers"],
};
const AR_COUNTS: Record<CountKind, [string, string, string, string]> = {
  points: ["نقطة", "نقطة واحدة", "نقطتان", "نقاط"], lines: ["بند", "بند واحد", "بندان", "بنود"],
  items: ["صنف", "صنف واحد", "صنفان", "أصناف"],
  suppliers: ["مورد", "مورد واحد", "موردان", "موردين"], products: ["منتج", "منتج واحد", "منتجان", "منتجات"],
  pos: ["أمر شراء", "أمر شراء واحد", "أمرا شراء", "أوامر شراء"], units: ["وحدة", "وحدة واحدة", "وحدتان", "وحدات"],
  changes: ["تغيير", "تغيير واحد", "تغييران", "تغييرات"],
  costUpdates: ["تحديث تكلفة", "تحديث تكلفة واحد", "تحديثا تكلفة", "تحديثات تكلفة"],
  customers: ["عميل", "عميل واحد", "عميلان", "عملاء"],
};
export function countText(language: Language, kind: CountKind, count: number): string {
  if (language === "en") return `${count} ${EN_COUNTS[kind][count === 1 ? 0 : 1]}`;
  const [singular, one, two, plural] = AR_COUNTS[kind];
  if (count === 1) return one;
  if (count === 2) return two;
  const formatted = new Intl.NumberFormat("ar-BH", { useGrouping: false }).format(count);
  if (count === 0 || count > 10) return `${formatted} ${singular}`;
  return `${formatted} ${plural}`;
}
