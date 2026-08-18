import { beforeEach, describe, expect, it } from "vitest";
import {
  directionFor,
  getLanguage,
  setLanguageGlobal,
} from "../hooks/useLanguage";

/**
 * Regression: the language used to live in a per-component `useState`, so each
 * of ~69 consumers held an independent copy. Toggling in the header updated
 * that component alone; screens only appeared to translate because navigating
 * remounts them and a fresh mount re-reads localStorage. The persistent shell
 * chrome never remounts, so the sidebar stayed English while `dir` became rtl
 * and the page content became Arabic.
 *
 * These pin the store contract — one authoritative value every reader shares —
 * rather than the symptom. The suite runs without a DOM (vitest.config.ts sets
 * `environment: "node"`), which also exercises the guards that let the hook run
 * where `document` and `localStorage` do not exist.
 */
describe("language store", () => {
  beforeEach(() => setLanguageGlobal("en"));

  it("is a single authoritative value, not a per-caller copy", () => {
    expect(getLanguage()).toBe("en");
    setLanguageGlobal("ar");
    // A second, independent read sees the change with nothing remounted —
    // the property the old per-component useState could not provide.
    expect(getLanguage()).toBe("ar");
  });

  it("round-trips both languages", () => {
    setLanguageGlobal("ar");
    expect(getLanguage()).toBe("ar");
    setLanguageGlobal("en");
    expect(getLanguage()).toBe("en");
  });

  it("treats a no-op change as a no-op", () => {
    setLanguageGlobal("ar");
    setLanguageGlobal("ar");
    expect(getLanguage()).toBe("ar");
  });

  it("maps direction as a pure function of the language", () => {
    expect(directionFor("ar")).toBe("rtl");
    expect(directionFor("en")).toBe("ltr");
  });

  it("survives an environment with no document or localStorage", () => {
    // Persistence and the document attributes are advisory: a missing browser
    // API must never throw into an operator flow.
    expect(() => setLanguageGlobal("ar")).not.toThrow();
    expect(getLanguage()).toBe("ar");
  });
});
