import { useCallback, useSyncExternalStore } from "react";
import i18n from "../i18n";

/**
 * UI language for the till and back office.
 *
 * Deliberately dependency-free and modelled on `useTheme`: this project keeps
 * UI preferences in localStorage and carries no i18n library, and adding one
 * for a two-language app would be a heavier change than the problem needs.
 *
 * ONE AUTHORITATIVE STATE, SHARED BY EVERY CONSUMER.
 *
 * This hook previously held the language in a per-component `useState`
 * initialised from localStorage. Each of its ~69 consumers therefore owned an
 * independent copy: toggling the language in the header updated that component
 * only. Screens appeared to translate because navigating between them remounts
 * them, and a fresh mount re-reads the stored value — but the persistent shell
 * chrome never remounts, so the sidebar stayed English while `dir` flipped to
 * rtl and the page content became Arabic. The Arabic strings were always
 * present; nothing was missing except a shared subscription.
 *
 * The store lives at module scope and components subscribe with
 * `useSyncExternalStore`. That keeps the public API byte-for-byte identical, so
 * no call site changes, and needs no provider — which matters because consumers
 * render in both the POS and back-office trees and in `renderToStaticMarkup`
 * tests, none of which would otherwise be wrapped.
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

// ─── The store ────────────────────────────────────────────────────────────────

let current: Language = storedLanguage();
const listeners = new Set<() => void>();

function subscribe(onChange: () => void): () => void {
  listeners.add(onChange);
  return () => { listeners.delete(onChange); };
}

/** Must return a stable value, so the snapshot is the primitive, not an object. */
function getSnapshot(): Language {
  return current;
}

/** Server render has no localStorage; English is the documented default. */
function getServerSnapshot(): Language {
  return "en";
}

function commit(next: Language) {
  if (next === current) return;
  current = next;
  storeLanguage(next);
  // Applied once per change rather than in an effect per consumer: 69
  // components each writing the same two document attributes is pure waste.
  applyLanguage(next);
  listeners.forEach(listener => listener());
}

// The document must agree with the stored language on first paint, before any
// component has mounted.
applyLanguage(current);

/** Escape hatch for non-React callers and tests. */
export function setLanguageGlobal(next: Language) {
  commit(next);
}

export function getLanguage(): Language {
  return current;
}

export function useLanguage() {
  const language = useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);

  const setLanguage = useCallback((next: Language) => { commit(next); }, []);
  const toggle = useCallback(() => {
    commit(getSnapshot() === "en" ? "ar" : "en");
  }, []);

  return { language, setLanguage, toggle, dir: directionFor(language) } as const;
}
