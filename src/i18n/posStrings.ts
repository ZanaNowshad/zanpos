import type { Language } from "../hooks/useLanguage";

/**
 * Till-facing strings, English and Arabic.
 *
 * Kept as a plain typed record rather than a library: two languages, one flat
 * namespace, no pluralisation rules in play. `PosStringKey` is derived from the
 * English table, so adding a key without an Arabic counterpart is a type error
 * rather than a silent fallback to English at a real till.
 *
 * Terminology follows Gulf retail usage — "الكاشير" for cashier, "الوردية" for
 * shift — rather than Modern Standard equivalents a shop worker would not use.
 */
const EN = {
  sale: "Sale",
  practice: "Practice",
  orders: "Orders",
  alerts: "Alerts",
  notes: "Notes",
  deliveries: "Deliveries",
  reprint: "Reprint",
  closeShift: "Close shift",
  logout: "Log out",
  lock: "Lock",
  trainingMode: "Training mode",
  trainingNotice: "Sales are not recorded — nothing here reaches your reports or stock.",
  exitTraining: "Exit training",
  scanOrSearch: "Scan or search",
  total: "Total",
  subtotal: "Subtotal",
  tax: "Tax",
  discount: "Discount",
  pay: "Pay",
  cash: "Cash",
  card: "Card",
  change: "Change",
  emptyCart: "Nothing in the cart yet",
  printing: "Printing…",
  printFailed: "Receipt did not print",
  offline: "Offline",
} as const;

export type PosStringKey = keyof typeof EN;

/** Every till string key. Exported so tests can assert the Arabic table is
 *  complete rather than re-listing keys by hand and drifting from the source. */
export const POS_STRING_KEYS = Object.keys(EN) as PosStringKey[];

const AR: Record<PosStringKey, string> = {
  sale: "بيع",
  practice: "تدريب",
  orders: "الطلبات",
  alerts: "التنبيهات",
  notes: "ملاحظات",
  deliveries: "التوصيل",
  reprint: "إعادة طباعة",
  closeShift: "إغلاق الوردية",
  logout: "تسجيل الخروج",
  lock: "قفل",
  trainingMode: "وضع التدريب",
  trainingNotice: "المبيعات هنا غير مسجّلة — لا تظهر في التقارير ولا تؤثر على المخزون.",
  exitTraining: "إنهاء التدريب",
  scanOrSearch: "امسح الباركود أو ابحث",
  total: "الإجمالي",
  subtotal: "المجموع",
  tax: "الضريبة",
  discount: "الخصم",
  pay: "الدفع",
  cash: "نقدًا",
  card: "بطاقة",
  change: "الباقي",
  emptyCart: "لا توجد أصناف في السلة",
  printing: "جارٍ الطباعة…",
  printFailed: "لم تتم طباعة الإيصال",
  offline: "غير متصل",
};

const TABLES: Record<Language, Record<PosStringKey, string>> = { en: EN, ar: AR };

/** Look up one till string. Falls back to English only if a key is somehow
 *  missing at runtime — the types prevent that at build time. */
export function posText(language: Language, key: PosStringKey): string {
  return TABLES[language]?.[key] ?? EN[key];
}

/** Bind the language once at a component boundary: `const t = posTranslator(language)`. */
export const posTranslator = (language: Language) => (key: PosStringKey) =>
  posText(language, key);
