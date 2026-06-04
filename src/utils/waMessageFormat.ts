// ── WhatsApp Message Format — types, defaults, storage, builder ───────────────

export type WaLang = "en" | "ar";
export type WaLineType = "text" | "items" | "blank";

export interface WaLine {
  id: string;
  type: WaLineType;
  enabled: boolean;
  template: string; // used by "text" type; empty for blank/items
}

export interface WaFormat {
  language: WaLang;
  en_lines: WaLine[];
  ar_lines: WaLine[];
}

export interface WaVars {
  customer_name?: string;
  receipt_number?: string;
  date?: string;
  amount?: string;
  address?: string;
  house_number?: string;
  area?: string;
  delivery_note?: string;
  method?: string;
  benefit_number?: string;
  store_name?: string;
  store_phone?: string;
}

// ── Variable reference metadata ───────────────────────────────────────────────

export const WA_VARIABLES: { key: keyof WaVars; label_en: string; label_ar: string }[] = [
  { key: "customer_name",  label_en: "Customer Name",    label_ar: "اسم العميل" },
  { key: "receipt_number", label_en: "Order #",          label_ar: "رقم الطلب" },
  { key: "date",           label_en: "Date & Time",      label_ar: "التاريخ والوقت" },
  { key: "amount",         label_en: "Total Amount",     label_ar: "المجموع" },
  { key: "address",        label_en: "Address",          label_ar: "العنوان" },
  { key: "house_number",   label_en: "House / Building", label_ar: "رقم المنزل" },
  { key: "area",           label_en: "Area / Block",     label_ar: "المنطقة" },
  { key: "delivery_note",  label_en: "Delivery Note",    label_ar: "ملاحظة التوصيل" },
  { key: "method",         label_en: "Payment Method",   label_ar: "طريقة الدفع" },
  { key: "benefit_number", label_en: "BenefitPay No.",   label_ar: "رقم بنفت باي" },
  { key: "store_name",     label_en: "Store Name",       label_ar: "اسم المتجر" },
  { key: "store_phone",    label_en: "Store Phone",      label_ar: "هاتف المتجر" },
];

// ── Default line templates ────────────────────────────────────────────────────

function t(template: string): WaLine {
  return { id: crypto.randomUUID(), type: "text", enabled: true, template };
}
function blank(): WaLine {
  return { id: crypto.randomUUID(), type: "blank", enabled: true, template: "" };
}
function items(): WaLine {
  return { id: crypto.randomUUID(), type: "items", enabled: true, template: "" };
}

export function makeDefaultEnLines(): WaLine[] {
  return [
    t("🛵 *Your order is confirmed!*"),
    blank(),
    t("📋 Order: *#{receipt_number}*"),
    t("📅 {date}"),
    t("👤 {customer_name}"),
    blank(),
    t("📦 Items:"),
    items(),
    blank(),
    t("💰 Total: *{amount}*"),
    blank(),
    t("📍 {address}"),
    t("🏠 {house_number}, {area}"),
    t("📝 {delivery_note}"),
    blank(),
    t("💳 Payment: {method}"),
    t("💸 BenefitPay: *{benefit_number}*"),
    blank(),
    t("Thank you for your order! 🙏"),
    t("_{store_name}  •  {store_phone}_"),
  ];
}

export function makeDefaultArLines(): WaLine[] {
  return [
    t("🛵 *تم تأكيد طلبك!*"),
    blank(),
    t("📋 الطلب: *#{receipt_number}*"),
    t("📅 {date}"),
    t("👤 {customer_name}"),
    blank(),
    t("📦 الطلبات:"),
    items(),
    blank(),
    t("💰 المجموع: *{amount}*"),
    blank(),
    t("📍 {address}"),
    t("🏠 {house_number}، {area}"),
    t("📝 {delivery_note}"),
    blank(),
    t("💳 طريقة الدفع: {method}"),
    t("💸 بنفت باي: *{benefit_number}*"),
    blank(),
    t("شكراً لطلبك! 🙏"),
    t("_{store_name}  •  {store_phone}_"),
  ];
}

// ── Default CUSTOMER-receipt templates ────────────────────────────────────────
// Sent on a normal (non-delivery) sale when a customer with a saved phone is
// selected at checkout. Same engine/variables as delivery; no address/items by
// default (a cashier can add an items line via the editor if they want one).

export function makeDefaultCustomerEnLines(): WaLine[] {
  return [
    t("✅ *Thank you, {customer_name}!*"),
    blank(),
    t("🧾 Receipt: *#{receipt_number}*"),
    t("📅 {date}"),
    t("💰 Amount: *{amount}*"),
    t("💳 Paid by: {method}"),
    t("💸 BenefitPay: *{benefit_number}*"),
    blank(),
    t("We appreciate your business! 🙏"),
    t("_{store_name}_"),
  ];
}

export function makeDefaultCustomerArLines(): WaLine[] {
  return [
    t("✅ *شكراً لك، {customer_name}!*"),
    blank(),
    t("🧾 الإيصال: *#{receipt_number}*"),
    t("📅 {date}"),
    t("💰 المبلغ: *{amount}*"),
    t("💳 طريقة الدفع: {method}"),
    t("💸 بنفت باي: *{benefit_number}*"),
    blank(),
    t("نشكر تعاملكم معنا! 🙏"),
    t("_{store_name}_"),
  ];
}

// ── localStorage persistence ──────────────────────────────────────────────────

const STORAGE_KEY          = "zanpos_wa_format";          // delivery-order message
const CUSTOMER_STORAGE_KEY = "zanpos_wa_customer_format"; // customer-receipt message

// Shared loader: reads a saved format under `key`, repairing missing line ids
// (upgrade from older saves); falls back to the supplied defaults when absent.
function loadFormat(
  key: string,
  defEn: () => WaLine[],
  defAr: () => WaLine[],
): WaFormat {
  try {
    const raw = localStorage.getItem(key);
    if (raw) {
      const parsed = JSON.parse(raw) as WaFormat;
      parsed.en_lines = parsed.en_lines.map(l => ({ ...l, id: l.id ?? crypto.randomUUID() }));
      parsed.ar_lines = parsed.ar_lines.map(l => ({ ...l, id: l.id ?? crypto.randomUUID() }));
      return parsed;
    }
  } catch { /* ignore */ }
  return { language: "en", en_lines: defEn(), ar_lines: defAr() };
}

export function loadWaFormat(): WaFormat {
  return loadFormat(STORAGE_KEY, makeDefaultEnLines, makeDefaultArLines);
}

export function saveWaFormat(fmt: WaFormat): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(fmt));
}

export function loadWaCustomerFormat(): WaFormat {
  return loadFormat(CUSTOMER_STORAGE_KEY, makeDefaultCustomerEnLines, makeDefaultCustomerArLines);
}

export function saveWaCustomerFormat(fmt: WaFormat): void {
  localStorage.setItem(CUSTOMER_STORAGE_KEY, JSON.stringify(fmt));
}

// ── Sample data for live preview ──────────────────────────────────────────────

export const SAMPLE_VARS_EN: WaVars = {
  customer_name:  "Ahmed Al-Khalifa",
  receipt_number: "MAIN-POS01-00000042",
  date:           "21 May 2026, 14:32",
  amount:         "BHD 3.500",
  address:        "Block 305, Road 1, Riffa",
  house_number:   "Villa 14",
  area:           "Block 305, Riffa",
  delivery_note:  "Ring bell twice",
  method:         "BenefitPay",
  benefit_number: "33050666",
  store_name:     "ZAN Restaurant",
  store_phone:    "+973 3305 0666",
};

export const SAMPLE_VARS_AR: WaVars = {
  customer_name:  "أحمد الخليفة",
  receipt_number: "MAIN-POS01-00000042",
  date:           "٢١ مايو ٢٠٢٦، ١٤:٣٢",
  amount:         "٣.٥٠٠ د.ب",
  address:        "مجمع ٣٠٥، طريق ١، الرفاع",
  house_number:   "فيلا ١٤",
  area:           "مجمع ٣٠٥، الرفاع",
  delivery_note:  "اقرع الجرس مرتين",
  method:         "بنفت باي",
  benefit_number: "33050666",
  store_name:     "مطعم زان",
  store_phone:    "+٩٧٣ ٣٣٠٥٠٦٦٦",
};

const SAMPLE_ITEMS_EN =
  "  • Chicken Burger × 1 ............ 2.500\n  • Large Fries × 2 ............... 1.000";
const SAMPLE_ITEMS_AR =
  "  • برجر دجاج × ١ .............. ٢.٥٠٠\n  • بطاطس كبيرة × ٢ ............. ١.٠٠٠";

// ── Message builder ───────────────────────────────────────────────────────────

export function buildWaMessage(
  lines: WaLine[],
  vars: WaVars,
  itemsList: string,
): string {
  const parts: string[] = [];

  for (const line of lines) {
    if (!line.enabled) continue;
    if (line.type === "blank") { parts.push(""); continue; }
    if (line.type === "items") { if (itemsList) parts.push(itemsList); continue; }

    let text = line.template;
    for (const [key, value] of Object.entries(vars)) {
      const placeholder = `{${key}}`;
      if (value) {
        text = text.split(placeholder).join(value);
      } else {
        // Remove placeholder and any immediately trailing separator chars
        text = text.split(placeholder).join("");
      }
    }

    // Don't emit lines that are entirely empty after substitution
    const stripped = text.replace(/[,،، .•·\-_*~]/g, "").trim();
    if (stripped.length === 0) continue;

    parts.push(text);
  }

  // Collapse 3+ consecutive blank lines into 1
  return parts
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

export function buildSampleMessage(lines: WaLine[], lang: WaLang): string {
  const vars  = lang === "ar" ? SAMPLE_VARS_AR  : SAMPLE_VARS_EN;
  const items = lang === "ar" ? SAMPLE_ITEMS_AR : SAMPLE_ITEMS_EN;
  return buildWaMessage(lines, vars, items);
}

/** Build a real message from a sale result */
export function buildDeliveryMessage(
  lines: WaLine[],
  vars: WaVars,
  saleItems: { product_name: string; quantity: string; unit_price_minor: number; line_total_minor: number }[],
  currencyExponent: number,
): string {
  const fmt = (n: number) => {
    if (currencyExponent === 0) return String(n);
    const div = 10 ** currencyExponent;
    return `${Math.floor(n / div)}.${String(n % div).padStart(currencyExponent, "0")}`;
  };

  const itemsList = saleItems
    .map(i => `  • ${i.product_name} × ${i.quantity} — ${fmt(i.line_total_minor)}`)
    .join("\n");

  return buildWaMessage(lines, vars, itemsList);
}

/**
 * Build a real CUSTOMER-receipt message from a sale result. Identical engine to
 * `buildDeliveryMessage` (lines + variables + optional auto items list) — kept as
 * a distinct name so call sites read clearly. The two differ only in which saved
 * format (and which default template) feed `lines`.
 */
export const buildCustomerMessage = buildDeliveryMessage;
