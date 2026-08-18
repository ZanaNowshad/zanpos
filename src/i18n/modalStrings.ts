import type { Language } from "../hooks/useLanguage";
import { EN, type ModalStringKey } from "./modalStrings.en";
import { AR } from "./modalStrings.ar";

export type { ModalStringKey };
export const MODAL_STRING_KEYS = Object.keys(EN) as ModalStringKey[];
const TABLES: Record<Language, Record<ModalStringKey, string>> = { en: EN, ar: AR };

export const modalText = (language: Language, key: ModalStringKey): string =>
  TABLES[language][key];

export const modalTranslator = (language: Language) => (key: ModalStringKey): string =>
  modalText(language, key);

const OWNED_LABELS: Record<string, ModalStringKey> = {
  completed: "statusCompleted",
  voided: "statusVoided",
  refunded: "statusRefunded",
  pending: "statusPending",
  failed: "statusFailed",
  conflict: "statusConflict",
  new: "newStatus",
  reviewed: "reviewed",
  fulfilled: "fulfilled",
  cancelled: "cancelledStatus",
  customer_return: "reasonCustomerReturn",
  defective: "reasonDefective",
  wrong_item: "reasonWrongItem",
  exchange: "exchange",
  other: "reasonOther",
};

export function ownedModalLabel(language: Language, value: string): string {
  const key = OWNED_LABELS[value.trim().toLowerCase()];
  return key ? modalText(language, key) : value;
}
