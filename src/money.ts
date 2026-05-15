/** Format minor units as currency string. BHD exponent=3: 1000 → "1.000" */
export function formatMoney(minor: number, exponent = 3): string {
  const divisor = Math.pow(10, exponent);
  const whole = Math.floor(Math.abs(minor) / divisor);
  const frac = Math.abs(minor) % divisor;
  const sign = minor < 0 ? "-" : "";
  return `${sign}${whole}.${String(frac).padStart(exponent, "0")}`;
}

/** Parse a decimal string to minor units. "1.500" → 1500 */
export function parseMoney(value: string, exponent = 3): number {
  const num = parseFloat(value);
  if (isNaN(num)) return 0;
  return Math.round(num * Math.pow(10, exponent));
}
