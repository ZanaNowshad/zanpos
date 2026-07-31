import { describe, expect, it } from "vitest";
import * as strings from "../i18n/backOfficeStrings";

const { BACK_OFFICE_STRING_KEYS, backOfficeText, backOfficeTranslator } = strings;

describe("back-office strings", () => {
  it("has a real Arabic translation for every key", () => {
    expect(BACK_OFFICE_STRING_KEYS.length).toBeGreaterThan(0);
    for (const key of BACK_OFFICE_STRING_KEYS) {
      const en = backOfficeText("en", key);
      const ar = backOfficeText("ar", key);
      expect(en.length, `English missing for ${key}`).toBeGreaterThan(0);
      expect(ar.length, `Arabic missing for ${key}`).toBeGreaterThan(0);
      expect(ar, `${key} was left in English`).not.toBe(en);
      expect(/[؀-ۿ]/.test(ar), `${key} has no Arabic script`).toBe(true);
    }
  });

  it("binds a language once and translates without re-passing it", () => {
    const t = backOfficeTranslator("ar");
    expect(t("settings")).toBe(backOfficeText("ar", "settings"));
  });

  it("keeps distinct stock terms distinct, since they mean different things", () => {
    // "inventory" (stock on hand) must not silently share a translation with
    // an unrelated key — a wrong term here changes what an operator thinks
    // they are about to do.
    expect(backOfficeText("ar", "inventory")).not.toBe(backOfficeText("ar", "products"));
    expect(backOfficeText("ar", "purchasing")).not.toBe(backOfficeText("ar", "suppliers"));
  });

  it("provides report and inventory terminology in both languages", () => {
    const expected = {
      reportSummary: ["Summary", "الملخص"],
      confirmVoidSale: ["Confirm void sale", "تأكيد إلغاء عملية البيع"],
      receiveStock: ["Receive stock", "استلام مخزون"],
      stocktake: ["Stocktake", "الجرد"],
    } as const;

    for (const [key, [english, arabic]] of Object.entries(expected)) {
      expect(backOfficeText("en", key as strings.BackOfficeStringKey)).toBe(english);
      expect(backOfficeText("ar", key as strings.BackOfficeStringKey)).toBe(arabic);
    }
  });

  it("maps owned report labels while preserving unknown backend values", () => {
    expect(typeof strings.reportSaleStatusText).toBe("function");
    expect(strings.reportSaleStatusText("ar", "completed")).toBe("مكتملة");
    expect(strings.reportSaleStatusText("ar", "server_new_state")).toBe("server_new_state");

    expect(strings.reportPaymentMethodsText("ar", "cash,card,custom_gateway")).toBe(
      "نقدًا، بطاقة، custom_gateway",
    );
    expect(strings.cashEventTypeText("ar", "paid_in")).toBe("إيداع نقدي");
    expect(strings.cashEventTypeText("ar", "custom_event")).toBe("custom_event");
  });

  it("maps owned inventory movement labels while preserving unknown values", () => {
    expect(typeof strings.inventoryMovementTypeText).toBe("function");
    expect(strings.inventoryMovementTypeText("ar", "stock_take")).toBe("جرد");
    expect(strings.inventoryMovementTypeText("ar", "server_new_movement")).toBe(
      "server_new_movement",
    );
  });
});
