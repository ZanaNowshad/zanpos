import { describe, expect, it } from "vitest";
import {
  OFFICE_AI_STRING_KEYS,
  officeAiFormat,
  officeAiText,
  officeAiTranslator,
} from "../i18n/officeAiStrings";
import {
  OFFICE_AI_TOOL_LABELS_EN,
  officeAiToolLabel,
  type OfficeAiToolName,
} from "../i18n/officeAiToolStrings";
import { QUICK_ACTIONS } from "../officeai/officeAiTypes";
import { buildOfficePulseModel } from "../officeai/officeAiData";
import type { OfficeAiOverviewSnapshot } from "../officeai/officeAiTypes";

describe("OfficeAI strings", () => {
  it("has a non-English Arabic translation for every owned UI key", () => {
    expect(OFFICE_AI_STRING_KEYS.length).toBeGreaterThan(100);
    for (const key of OFFICE_AI_STRING_KEYS) {
      const english = officeAiText("en", key);
      const arabic = officeAiText("ar", key);
      expect(arabic.length, `Arabic missing for ${key}`).toBeGreaterThan(0);
      expect(arabic, `Arabic equals English for ${key}`).not.toBe(english);
      expect(/[؀-ۿ]/.test(arabic), `${key} has no Arabic script`).toBe(true);
    }
  });

  it("has a compile-safe Arabic display label for every known AI tool", () => {
    for (const name of Object.keys(OFFICE_AI_TOOL_LABELS_EN) as OfficeAiToolName[]) {
      const arabic = officeAiToolLabel("ar", name);
      expect(arabic.length, `Arabic tool label missing for ${name}`).toBeGreaterThan(0);
      expect(/[؀-ۿ]/.test(arabic), `${name} tool label has no Arabic script`).toBe(true);
    }
  });

  it("formats counts without changing stable machine prompt payloads", () => {
    expect(officeAiFormat(officeAiText("ar", "eventsPending"), { count: 3 })).toContain("3");
    expect(QUICK_ACTIONS[0].prompt).toBe("Give me today's sales summary");
    expect(QUICK_ACTIONS[1].prompt).toBe("Which products are low on stock?");
  });

  it("builds user-owned overview labels in Arabic while preserving opaque findings", () => {
    const snapshot: OfficeAiOverviewSnapshot = {
      loading: false,
      refreshedAt: "2026-07-28T08:00:00Z",
      errors: [],
      today: null,
      lowStockCount: 0,
      outOfStockCount: 0,
      sync: null,
      whatsapp: null,
      whatsappUnread: 0,
      paymentConfirmations: [],
      provider: null,
      aiEnabled: false,
      aiConfig: null,
      featureToggles: null,
      health: null,
      benefitNumber: null,
    };
    const model = buildOfficePulseModel(snapshot, 0, 3, officeAiTranslator("ar"));
    expect(model.signals[0].label).toBe(officeAiText("ar", "netSalesSignal"));
    expect(model.attention.some(item => item.title === officeAiText("ar", "aiDisabled"))).toBe(true);
  });
});
