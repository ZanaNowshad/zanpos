import { describe, expect, it } from "vitest";
// @ts-expect-error Vitest runs in Node; the app intentionally omits Node types.
import { readFileSync } from "node:fs";

const tokensCss = readFileSync(new URL("../styles/tokens.css", import.meta.url), "utf8");
const appCss = readFileSync(new URL("../App.css", import.meta.url), "utf8");
const operatorCss = readFileSync(new URL("../operator-ux.css", import.meta.url), "utf8");

describe("type scale contract", () => {
  it("defines exactly two semantic POS sizes", () => {
    const posSizeTokens = tokensCss.match(/--type-pos-[a-z-]+:/g) ?? [];
    expect(posSizeTokens).toEqual(["--type-pos-base:", "--type-pos-large:"]);
  });

  it("defines the four back-office hierarchy steps", () => {
    expect(tokensCss).toContain("--type-office-page:");
    expect(tokensCss).toContain("--type-office-section:");
    expect(tokensCss).toContain("--type-office-body:");
    expect(tokensCss).toContain("--type-office-caption:");
  });

  it("applies the POS scale to the total, keypad and amount due", () => {
    expect(appCss).toMatch(/\.cart-grand-total\s*\{[^}]*var\(--type-pos-large\)/s);
    expect(appCss).toMatch(/\.numpad-panel \.dialpad-key\s*\{[^}]*var\(--type-pos-large\)/s);
    expect(operatorCss).toMatch(/\.pm-shell-calm \.pm-amount-value\s*\{[^}]*var\(--type-pos-large\)/s);
    expect(operatorCss).toMatch(/\.pm-ready-amount\s*\{[^}]*var\(--type-pos-large\)/s);
  });

  it("keeps monetary values tabular while applying the scale", () => {
    expect(appCss).toMatch(/\.cart-grand-total\s*\{[^}]*tabular-nums/s);
    expect(appCss).toMatch(/\.pm-amount-value\s*\{[^}]*tabular-nums/s);
    expect(appCss).toMatch(/\.rpt-card-value\s*\{[^}]*tabular-nums/s);
    expect(operatorCss).toMatch(/\.pm-ready-amount\s*\{[^}]*tabular-nums/s);
  });

  it("applies all four office steps to real surfaces", () => {
    expect(appCss).toMatch(/\.settings-page-title\s*\{[^}]*var\(--type-office-page\)/s);
    expect(appCss).toMatch(/\.bo-form-title\s*\{[^}]*var\(--type-office-section\)/s);
    expect(appCss).toMatch(/\.bo-content\s*\{[^}]*var\(--type-office-body\)/s);
    expect(appCss).toMatch(/\.bo-label\s*\{[^}]*var\(--type-office-caption\)/s);
  });
});
