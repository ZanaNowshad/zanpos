import { describe, expect, it } from "vitest";
import { POS_STRING_KEYS, posText, posTranslator } from "../i18n/posStrings";
import { directionFor } from "../hooks/useLanguage";

describe("till strings", () => {
  it("has a real Arabic translation for every key", () => {
    // Iterating the exported key list rather than a hand-written one means a
    // new English string with no Arabic counterpart fails here, instead of
    // silently showing English at an Arabic-speaking till.
    expect(POS_STRING_KEYS.length).toBeGreaterThan(0);
    for (const key of POS_STRING_KEYS) {
      const en = posText("en", key);
      const ar = posText("ar", key);
      expect(en.length, `English missing for ${key}`).toBeGreaterThan(0);
      expect(ar.length, `Arabic missing for ${key}`).toBeGreaterThan(0);
      expect(ar, `${key} was left in English`).not.toBe(en);
      expect(/[؀-ۿ]/.test(ar), `${key} has no Arabic script`).toBe(true);
    }
  });

  it("binds a language once and translates without re-passing it", () => {
    const t = posTranslator("ar");
    expect(t("total")).toBe(posText("ar", "total"));
  });

  it("maps Arabic to RTL and English to LTR", () => {
    expect(directionFor("ar")).toBe("rtl");
    expect(directionFor("en")).toBe("ltr");
  });
});
