/** Format minor units as currency string. BHD exponent=3: 1000 → "1.000" */
export function formatMoney(minor: number, exponent = 3): string {
  const divisor = Math.pow(10, exponent);
  const whole = Math.floor(Math.abs(minor) / divisor);
  const frac = Math.abs(minor) % divisor;
  const sign = minor < 0 ? "-" : "";
  return `${sign}${whole}.${String(frac).padStart(exponent, "0")}`;
}

/**
 * Parse a decimal string to minor units without floating-point rounding error.
 * Splits on the decimal point and constructs the integer directly from digits.
 * "1.500" → 1500,  "0.025" → 25,  "-2.100" → -2100
 */
export function parseMoney(value: string, exponent = 3): number {
  const trimmed = (value ?? "").trim();
  if (!trimmed) return 0;
  const negative = trimmed.startsWith("-");
  // L10: Strip thousands separators (commas and Arabic-locale spaces) before parsing
  const abs = (negative ? trimmed.slice(1) : trimmed).replace(/[,\s_]/g, "");
  const dotIdx = abs.indexOf(".");
  const whole = dotIdx === -1 ? abs : abs.slice(0, dotIdx);
  const rawFrac = dotIdx === -1 ? "" : abs.slice(dotIdx + 1);
  // Pad or truncate fraction to exactly `exponent` digits — no float involved
  const frac = rawFrac.slice(0, exponent).padEnd(exponent, "0");
  const wholePart = parseInt(whole || "0", 10);
  const fracPart = parseInt(frac, 10);
  if (isNaN(wholePart) || isNaN(fracPart)) return 0;
  const minor = wholePart * Math.pow(10, exponent) + fracPart;
  return negative ? -minor : minor;
}
