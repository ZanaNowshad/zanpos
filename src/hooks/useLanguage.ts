import { useCallback, useEffect, useState } from "react";
import i18n from "../i18n";

/**
 * UI language for the till and back office.
 *
 * Deliberately dependency-free and modelled on `useTheme`: this project keeps
 * UI preferences in localStorage and carries no i18n library, and adding one
 * for a two-language app would be a heavier change than the problem needs.
 *
 * Scope note: before this existed, every string in the POS and back office was
 * a hardcoded English literal — only receipts, the storefront and WhatsApp
 * templates were ever bilingual. This is the foundation plus the till strings;
 * the back office is still English and needs the same treatment surface by
 * surface, ideally with a native reviewer, since a mistranslated action at a
 * till causes real mistakes rather than mild confusion.
 */
export type Language = "en" | "ar";

const STORAGE_KEY = "zanpos_language";

function isValidLanguage(value: string | null): value is Language {
  return value === "en" || value === "ar";
}

function storedLanguage(): Language {
  try {
    if (typeof localStorage === "undefined") return "en";
    const saved = localStorage.getItem(STORAGE_KEY);
    return isValidLanguage(saved) ? saved : "en";
  } catch {
    return "en";
  }
}

function storeLanguage(language: Language) {
  try {
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(STORAGE_KEY, language);
    }
  } catch {
    // Language persistence is advisory and must never break an operator flow.
  }
}

/** Arabic is RTL; numbers stay LTR within it, which the browser handles. */
export const directionFor = (language: Language): "rtl" | "ltr" =>
  language === "ar" ? "rtl" : "ltr";

function applyLanguage(language: Language) {
  if (typeof document !== "undefined") {
    document.documentElement.setAttribute("lang", language);
    document.documentElement.setAttribute("dir", directionFor(language));
  }
  i18n.changeLanguage(language).catch(() => {});
}

export function useLanguage() {
  const [language, setLanguageState] = useState<Language>(storedLanguage);

  useEffect(() => {
    applyLanguage(language);
  }, [language]);

  const setLanguage = useCallback((next: Language) => {
    storeLanguage(next);
    setLanguageState(next);
  }, []);

  const toggle = useCallback(() => {
    setLanguageState(prev => {
      const next: Language = prev === "en" ? "ar" : "en";
      storeLanguage(next);
      return next;
    });
  }, []);

  return { language, setLanguage, toggle, dir: directionFor(language) } as const;
}
