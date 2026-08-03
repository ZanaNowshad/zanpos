import { describe, it, expect } from "vitest";

const namespaces = [
  "pos",
  "backOffice",
  "modal",
  "detail",
  "operations",
  "officeAi",
  "officeAiTool",
] as const;

describe("i18next locale parity", () => {
  for (const ns of namespaces) {
    describe(ns, () => {
      const en = require(`../locales/en/${ns}.json`);
      const ar = require(`../locales/ar/${ns}.json`);

      it("has keys in both languages", () => {
        const enKeys = Object.keys(en);
        const arKeys = Object.keys(ar);
        expect(enKeys.length).toBeGreaterThan(0);

        const onlyEn = enKeys.filter((k) => !arKeys.includes(k));
        const onlyAr = arKeys.filter((k) => !enKeys.includes(k));

        expect(onlyEn, `keys missing in AR: ${onlyEn.join(", ")}`).toEqual([]);
        expect(onlyAr, `keys missing in EN: ${onlyAr.join(", ")}`).toEqual([]);
      });

      it("has no empty values in either language", () => {
        for (const [key, value] of Object.entries(en)) {
          if (typeof value === "string") {
            expect(value, `${ns}.${key} (EN) is empty`).toBeTruthy();
          }
        }
        for (const [key, value] of Object.entries(ar)) {
          if (typeof value === "string") {
            expect(value, `${ns}.${key} (AR) is empty`).toBeTruthy();
          }
        }
      });

      it("has no raw HTML in interpolation strings", () => {
        for (const [key, value] of Object.entries(en)) {
          if (typeof value === "string") {
            expect(value, `${ns}.${key} (EN) contains raw HTML`).not.toMatch(/<[^>]+>/);
          }
        }
        for (const [key, value] of Object.entries(ar)) {
          if (typeof value === "string") {
            expect(value, `${ns}.${key} (AR) contains raw HTML`).not.toMatch(/<[^>]+>/);
          }
        }
      });

      it("has no invalid i18next interpolation syntax", () => {
        const invalidPattern = /\{\{(?!\s*\/?[a-zA-Z][\w.]*\s*}})/;
        for (const [key, value] of Object.entries(en)) {
          if (typeof value === "string" && value.includes("{{")) {
            expect(value, `${ns}.${key} (EN) has invalid interpolation`).not.toMatch(
              invalidPattern,
            );
          }
        }
        for (const [key, value] of Object.entries(ar)) {
          if (typeof value === "string" && value.includes("{{")) {
            expect(value, `${ns}.${key} (AR) has invalid interpolation`).not.toMatch(
              invalidPattern,
            );
          }
        }
      });
    });
  }
});
