import type { CartLine, CatalogProduct, Locale } from "./types";

export function formatMoney(minor: number, code: string, decimals: number, locale: Locale) {
  const safeDecimals = Math.min(6, Math.max(0, Math.trunc(decimals)));
  return new Intl.NumberFormat(locale === "ar" ? "ar-BH" : "en-BH", {
    style: "currency",
    currency: code,
    minimumFractionDigits: safeDecimals,
    maximumFractionDigits: safeDecimals,
  }).format(minor / 10 ** safeDecimals);
}

export function normalizeQuantity(value: string | number, decimals = 0) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed <= 0) return 0;
  const places = Math.min(3, Math.max(0, Math.trunc(decimals)));
  const scale = 10 ** places;
  return Math.floor(parsed * scale + Number.EPSILON) / scale;
}

export function lineTotalMinor(line: CartLine) {
  return Math.round(line.priceMinor * line.quantity);
}

export function refreshCartLines(
  lines: CartLine[],
  products: CatalogProduct[],
  locale: Locale,
) {
  const byId = new Map(products.map(product => [product.id, product]));
  return lines.flatMap(line => {
    const product = byId.get(line.id);
    if (!product?.available) return [];
    return [{
      ...line,
      name: product.name[locale],
      priceMinor: product.priceMinor,
    }];
  });
}

export function buildWhatsAppOrder(input: {
  orderId: string;
  currency: string;
  decimals: number;
  locale: Locale;
  lines: CartLine[];
}) {
  const ar = input.locale === "ar";
  const total = input.lines.reduce((sum, line) => sum + lineTotalMinor(line), 0);
  const human = [
    ar ? `طلب ${input.orderId}` : `Order ${input.orderId}`,
    ...input.lines.map((line) =>
      `${line.name} × ${line.quantity} — ${formatMoney(lineTotalMinor(line), input.currency, input.decimals, input.locale)}`),
    `${ar ? "الإجمالي" : "Total"}: ${formatMoney(total, input.currency, input.decimals, input.locale)}`,
  ].join("\n");
  const items = input.lines.map(({ id, quantity }) => `${id}:${quantity}`).join(",");
  const machine = [
    "[ZANPOS:v1]",
    `order_id=${input.orderId}`,
    `currency=${input.currency}`,
    `items=${items}`,
    "[/ZANPOS]",
  ].join("\n");
  return `${human}\n\n${machine}`;
}
