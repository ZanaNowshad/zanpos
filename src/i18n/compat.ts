import type { Language } from "../hooks/useLanguage";
import i18n from "./index";

export function t(key: string, ns = "pos"): string {
  return i18n.t(key, { ns, lng: i18n.language });
}

export function setLanguage(lang: Language) {
  i18n.changeLanguage(lang);
}

export function getLanguage(): Language {
  return i18n.language as Language;
}
