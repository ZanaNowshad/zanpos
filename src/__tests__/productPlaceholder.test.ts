import { describe, expect, it } from "vitest";
import { placeholderInitial, productPlaceholderSrc } from "../productPlaceholder";

/** The generated SVG, back out of the data URI, for asserting on its content. */
function decode(src: string): string {
  expect(src.startsWith("data:image/svg+xml,")).toBe(true);
  return decodeURIComponent(src.slice("data:image/svg+xml,".length));
}

describe("the letter on a placeholder", () => {
  it("prefers the category, because that is what groups the rows", () => {
    expect(placeholderInitial("Dairy", "Almarai Fresh Milk 1L")).toBe("D");
  });

  it("falls back to the product when there is no category", () => {
    expect(placeholderInitial(null, "Almarai Fresh Milk 1L")).toBe("A");
    expect(placeholderInitial("   ", "Barbican Malt")).toBe("B");
  });

  it("keeps a non-Latin first character whole", () => {
    // Arabic category names are normal here. Slicing by index would split a
    // surrogate pair and render a replacement box.
    expect(placeholderInitial("ألبان", null)).toBe("ألبان".charAt(0));
    expect(placeholderInitial("🧴 Cleaning", null)).toBe("🧴");
  });

  it("is empty when there is nothing to take a letter from", () => {
    expect(placeholderInitial(null, null)).toBe("");
  });
});

describe("the placeholder tile", () => {
  it("is a data URI an img can load without a network", () => {
    const svg = decode(productPlaceholderSrc("Dairy", "Milk"));
    expect(svg).toContain("<svg");
    expect(svg).toContain('viewBox="0 0 64 64"');
    expect(svg).toContain(">D<");
  });

  it("gives the same category the same colour every time", () => {
    // Two products in one category must look like siblings, and the tint has
    // to survive a reload — it is derived, not assigned.
    expect(productPlaceholderSrc("Dairy", "Milk"))
      .toBe(productPlaceholderSrc("Dairy", "Milk"));
    const a = decode(productPlaceholderSrc("Dairy", "Milk"));
    const b = decode(productPlaceholderSrc("Dairy", "Laban"));
    const hue = (svg: string) => svg.match(/hsl\((\d+)/)?.[1];
    expect(hue(a)).toBe(hue(b));
  });

  it("gives different categories different colours", () => {
    const hue = (name: string) =>
      decode(productPlaceholderSrc(name, "x")).match(/hsl\((\d+)/)?.[1];
    const hues = new Set(["Dairy", "Bakery", "Cleaning", "Beverages", "Produce"].map(hue));
    // Not a promise of uniqueness for every possible name — just that the
    // common case does not collapse to one colour.
    expect(hues.size).toBeGreaterThanOrEqual(4);
  });

  it("still separates products when the caller has no category", () => {
    /* The till's cart and quick-add rail work from a sale line, which carries
       no category. Seeding from a constant there made every row in the basket
       the same shade — the uniform grey box again, in a nicer colour. */
    const hue = (name: string) =>
      decode(productPlaceholderSrc(null, name)).match(/hsl\((\d+)/)?.[1];
    const hues = new Set([
      "Almarai Fresh Milk 1L", "Lipton Yellow Label 100s",
      "Barbican Malt 330ml", "Al Ain Water 1.5L", "Nadec Laban 1L",
    ].map(hue));
    expect(hues.size).toBeGreaterThanOrEqual(4);
  });

  it("is case- and space-insensitive about the category", () => {
    expect(productPlaceholderSrc(" dairy ", "Milk"))
      .toBe(productPlaceholderSrc("Dairy", "Milk"));
  });

  it("escapes a name that would otherwise break the markup", () => {
    // Product names in an imported catalogue contain & and < more often than
    // anyone expects, and this string is spliced into SVG.
    const svg = decode(productPlaceholderSrc("<script>", null));
    expect(svg).not.toContain("<script>");
    expect(svg).toContain("&lt;");
  });

  it("still renders a tile when there is no name at all", () => {
    const svg = decode(productPlaceholderSrc(null, null));
    expect(svg).toContain("<circle");
    expect(svg).toContain("<rect");
  });
});
