import { describe, expect, it, vi } from "vitest";

// convertFileSrc is a Tauri binding; the point of these tests is the branch that
// decides whether it is called at all, so the stub makes that visible.
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => `asset://${p}`,
}));

const {
  isRemoteImage,
  productImageSrc,
  resolveProductImageValue,
  shouldShowProductImage,
  validateImageUrl,
} = await import("../productImage");

describe("product image source resolution", () => {
  it("passes a web link through untouched", () => {
    // Running a URL through convertFileSrc mangles it into something no browser
    // will load, which is the bug this branch exists to prevent.
    expect(productImageSrc("https://cdn.example.com/milk.jpg")).toBe(
      "https://cdn.example.com/milk.jpg",
    );
  });

  it("sends a local path through the asset protocol", () => {
    expect(productImageSrc("C:/images/milk.png")).toBe("asset://C:/images/milk.png");
  });

  it("treats blank and missing values as no image", () => {
    expect(productImageSrc("")).toBeNull();
    expect(productImageSrc("   ")).toBeNull();
    expect(productImageSrc(null)).toBeNull();
    expect(productImageSrc(undefined)).toBeNull();
  });

  it("does not mistake a Windows path for a URL", () => {
    expect(isRemoteImage("C:\\images\\milk.png")).toBe(false);
    expect(isRemoteImage("/var/images/milk.png")).toBe(false);
  });
});

describe("image URL validation", () => {
  it("accepts http and https", () => {
    expect(validateImageUrl("https://example.com/a.jpg")).toBeNull();
    expect(validateImageUrl("http://example.com/a.jpg")).toBeNull();
  });

  it("rejects schemes that are not web links", () => {
    // data: and blob: would let a pasted string carry arbitrary bytes into the
    // catalogue; file: reads from disk outside the picker's control.
    for (const bad of [
      "javascript:alert(1)",
      "data:image/svg+xml;base64,AAAA",
      "file:///etc/passwd",
      "ftp://example.com/a.png",
    ]) {
      expect(validateImageUrl(bad)).toBe("scheme");
    }
  });

  it("reports malformed input separately from a bad scheme", () => {
    expect(validateImageUrl("not a url")).toBe("malformed");
    expect(validateImageUrl("https://")).toBe("malformed");
  });

  it("treats an empty field as nothing to apply, not an error", () => {
    expect(validateImageUrl("")).toBe("empty");
    expect(validateImageUrl("   ")).toBe("empty");
  });
});

describe("product image form state", () => {
  it("uses a valid pending URL when Save follows paste without waiting for blur state", () => {
    expect(resolveProductImageValue("C:/images/old.png", " https://cdn.example.com/new?id=4 ")).toEqual({
      value: "https://cdn.example.com/new?id=4",
      error: null,
    });
  });

  it("keeps the selected image when the URL field is blank", () => {
    expect(resolveProductImageValue(" C:/images/milk.png ", "   ")).toEqual({
      value: "C:/images/milk.png",
      error: null,
    });
  });

  it("blocks saving a malformed pending URL", () => {
    expect(resolveProductImageValue("C:/images/milk.png", "not a url")).toEqual({
      value: "C:/images/milk.png",
      error: "malformed",
    });
  });

  it("shows the fallback only for the image URL that failed", () => {
    expect(shouldShowProductImage("https://cdn.example.com/broken.jpg", null)).toBe(true);
    expect(
      shouldShowProductImage(
        "https://cdn.example.com/broken.jpg",
        "https://cdn.example.com/broken.jpg",
      ),
    ).toBe(false);
    expect(
      shouldShowProductImage(
        "https://cdn.example.com/replacement.jpg",
        "https://cdn.example.com/broken.jpg",
      ),
    ).toBe(true);
  });
});
