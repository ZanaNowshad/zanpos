import { describe, expect, it } from "vitest";
import {
  MODAL_STRING_KEYS,
  modalText,
  modalTranslator,
  ownedModalLabel,
} from "../i18n/modalStrings";

/**
 * A handful of strings are identical in both languages on purpose because they
 * are not prose: an example URL shown as placeholder text reads the same to an
 * Arabic speaker, and "translating" example.com would make it worse. Anything
 * added here needs that justification — the default remains that an identical
 * string is an untranslated one.
 */
const NOT_TRANSLATABLE = new Set(["imageUrlPlaceholder"]);

describe("modal strings", () => {
  it("has a real Arabic translation for every key", () => {
    for (const key of MODAL_STRING_KEYS) {
      const en = modalText("en", key);
      const ar = modalText("ar", key);
      expect(en.length, `English missing for ${key}`).toBeGreaterThan(0);
      expect(ar.length, `Arabic missing for ${key}`).toBeGreaterThan(0);
      if (NOT_TRANSLATABLE.has(key)) continue;
      expect(ar, `${key} was left in English`).not.toBe(en);
      expect(/[؀-ۿ]/.test(ar), `${key} has no Arabic script`).toBe(true);
    }
  });

  it("binds a language without a runtime fallback", () => {
    const t = modalTranslator("ar");
    expect(t("openShift")).toBe("فتح الوردية");
    expect(t("stocktake")).toBe("الجرد");
  });

  it("maps only owned enum labels and preserves unknown backend values", () => {
    expect(ownedModalLabel("ar", "completed")).toBe("مكتملة");
    expect(ownedModalLabel("ar", "future_server_status")).toBe("future_server_status");
  });
});
