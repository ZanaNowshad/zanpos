import { describe, expect, it } from "vitest";
import {
  OPERATIONS_STRING_KEYS,
  operationsText,
  operationsTranslator,
  poStatusText,
  roleText,
  countText,
  deviceStatusText,
  hubTruthStatusText,
} from "../i18n/operationsStrings";

describe("operations strings", () => {
  it("provides non-empty English and Arabic text for every operational key", () => {
    for (const key of OPERATIONS_STRING_KEYS) {
      expect(operationsText("en", key).trim(), `English ${key}`).not.toBe("");
      expect(operationsText("ar", key).trim(), `Arabic ${key}`).not.toBe("");
    }
  });

  it("binds a translator to the selected language", () => {
    const t = operationsTranslator("ar");
    expect(t("receivePurchaseOrder")).toBe("استلام أمر الشراء");
    expect(t("loyaltyPoints")).toBe("نقاط الولاء");
  });

  it("maps purchase-order statuses without changing their stored values", () => {
    expect(poStatusText("ar", "draft")).toBe("مسودة");
    expect(poStatusText("ar", "ordered")).toBe("تم إرساله للمورد");
    expect(poStatusText("ar", "partial")).toBe("مستلم جزئياً");
    expect(poStatusText("ar", "received")).toBe("مستلم بالكامل");
    expect(poStatusText("ar", "cancelled")).toBe("ملغي");
    expect(poStatusText("en", "vendor_defined")).toBe("vendor_defined");
  });

  it("maps owned roles while preserving unknown backend data", () => {
    expect(roleText("ar", "owner")).toBe("المالك");
    expect(roleText("ar", "manager")).toBe("المدير");
    expect(roleText("ar", "cashier")).toBe("أمين الصندوق");
    expect(roleText("ar", "accountant")).toBe("المحاسب");
    expect(roleText("ar", "custom_role")).toBe("custom_role");
  });

  it("maps device and Hub display states without changing unknown backend data", () => {
    expect(deviceStatusText("ar", true)).toBe("مُفعّل");
    expect(deviceStatusText("ar", false)).toBe("مُعطّل");
    expect(hubTruthStatusText("ar", "missing_on_hub")).toBe("غير موجود على الجهاز الرئيسي");
    expect(hubTruthStatusText("en", "future_state")).toBe("future_state");
  });

  it("formats operational counts with Arabic singular, dual, and plural forms", () => {
    expect(countText("ar", "points", 0)).toBe("٠ نقطة");
    expect(countText("ar", "points", 1)).toBe("نقطة واحدة");
    expect(countText("ar", "points", 2)).toBe("نقطتان");
    expect(countText("ar", "points", 5)).toBe("٥ نقاط");
    expect(countText("en", "lines", 2)).toBe("2 lines");
  });
});
