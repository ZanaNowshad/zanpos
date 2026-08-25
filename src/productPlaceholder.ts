/**
 * What a product looks like before its photo arrives.
 *
 * Images are fetched in the background now, so at any moment some part of the
 * catalogue has none — a product added a minute ago, one whose barcode is not
 * in any public database, or a whole catalogue on its first day. Those used to
 * render as an identical grey box glyph, which makes a till basket of five
 * items read as five copies of nothing.
 *
 * A placeholder here is tinted by category and carries its initial, so the rows
 * differ from each other and a cashier can tell dairy from cleaning at a glance
 * without reading. It is deliberately flat and lettered rather than a stock
 * photograph: it must never be mistaken for a picture of the actual product,
 * because a cashier picking by image would then pick the wrong thing.
 *
 * Generated as an SVG data URI rather than shipped as files. There is no fixed
 * set of categories — a shop invents its own — so the tint has to be derived
 * from the name rather than looked up, and a generated tile costs no bundle
 * size and no network.
 */

/**
 * A stable hue for a name. FNV-1a, because the requirement is only that the
 * same category always gets the same colour and that neighbouring names get
 * different ones — not that it is hard to reverse.
 */
function hueOf(seed: string): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < seed.length; i += 1) {
    hash ^= seed.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  /* Steps of 37° around the wheel rather than the raw remainder: 37 and 360
     share no factors, so successive hashes land far apart and two categories
     rarely come out the same colour. */
  return (hash % 360) * 37 % 360;
}

/** The letter shown on the tile. Falls back through category, then product. */
export function placeholderInitial(
  categoryName?: string | null,
  productName?: string | null,
): string {
  const source = (categoryName ?? "").trim() || (productName ?? "").trim();
  // `codePointAt` rather than `[0]`: an Arabic or emoji first character is one
  // code point but two UTF-16 units, and slicing by index splits it.
  const first = source ? String.fromCodePoint(source.codePointAt(0) ?? 0) : "";
  return first.toLocaleUpperCase();
}

/**
 * An `<img src>` for a product with no image of its own.
 *
 * Sized 64×64 with a viewBox, so it scales to whatever the call site renders it
 * at — the till thumbnail, the catalogue row and the quick-add tile are all
 * different sizes and all use this.
 */
export function productPlaceholderSrc(
  categoryName?: string | null,
  productName?: string | null,
): string {
  /* Category first, so two products on the same shelf look like siblings in
     the catalogue. Where the caller has no category — the till's cart and
     quick-add rail work from a sale line, which carries none — the product's
     own name seeds it instead. Falling back to a constant there made every row
     in the basket the same shade of lilac, which is the uniform grey box again
     in a nicer colour. */
  const seed = (categoryName ?? "").trim().toLowerCase()
    || (productName ?? "").trim().toLowerCase()
    || "uncategorised";
  const hue = hueOf(seed);
  const initial = placeholderInitial(categoryName, productName);
  /* Two tints of one hue: a soft fill that reads as a surface rather than a
     photo, and a saturated mark that stays legible at 32px. Kept well away
     from the till's own accent so a placeholder never looks like a selection. */
  const fill = `hsl(${hue} 46% 93%)`;
  const edge = `hsl(${hue} 38% 82%)`;
  const ink = `hsl(${hue} 45% 42%)`;

  const svg = [
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">`,
    `<rect width="64" height="64" rx="10" fill="${fill}" stroke="${edge}"/>`,
    initial
      ? `<text x="32" y="41" text-anchor="middle" font-family="system-ui,sans-serif"`
        + ` font-size="27" font-weight="700" fill="${ink}">${escapeXml(initial)}</text>`
      // No name at all to take a letter from — a bare tinted tile still beats
      // a shared grey box, because it differs from its neighbours.
      : `<circle cx="32" cy="32" r="9" fill="${ink}" opacity="0.35"/>`,
    `</svg>`,
  ].join("");

  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

/** A product name can contain `&` or `<`, and this string goes inside markup. */
function escapeXml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}
