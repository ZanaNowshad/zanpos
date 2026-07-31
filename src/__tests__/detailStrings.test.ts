import { describe, expect, it } from "vitest";
import {
  DETAIL_STRING_KEYS,
  detailText,
  detailTranslator,
} from "../i18n/detailStrings";

describe("detail strings", () => {
  it("has a real Arabic translation for every key", () => {
    for (const key of DETAIL_STRING_KEYS) {
      const en = detailText("en", key);
      const ar = detailText("ar", key);
      expect(en.length, `English missing for ${key}`).toBeGreaterThan(0);
      expect(ar.length, `Arabic missing for ${key}`).toBeGreaterThan(0);
      expect(ar, `${key} was left in English`).not.toBe(en);
      expect(/[؀-ۿ]/.test(ar), `${key} has no Arabic script`).toBe(true);
    }
  });

  it("binds directly to the selected language without fallback", () => {
    const t = detailTranslator("ar");
    expect(t("keyboardShortcuts")).toBe("اختصارات لوحة المفاتيح");
    expect(t("archiveProduct")).toBe("أرشفة المنتج");
  });
});
