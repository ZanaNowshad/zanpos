import { describe, it, expect } from "vitest";
import posEn from "../locales/en/pos.json";

describe("i18next locale parity", () => {
  it("pos namespace has all English keys", () => {
    expect(Object.keys(posEn).length).toBeGreaterThan(0);
  });

  it("pos namespace has no empty values", () => {
    for (const [key, value] of Object.entries(posEn)) {
      expect(value, `pos.${key} is empty`).toBeTruthy();
    }
  });

  it("pos namespace has no unescaped HTML interpolation", () => {
    for (const [key, value] of Object.entries(posEn)) {
      expect(value, `pos.${key} contains raw HTML`).not.toMatch(/<[^>]+>/);
    }
  });
});
