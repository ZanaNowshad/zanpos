import type { Language } from "../hooks/useLanguage";
import { EN, type OfficeAiStringKey } from "./officeAiStrings.en";
import { AR } from "./officeAiStrings.ar";

export type { OfficeAiStringKey };

export const OFFICE_AI_STRING_KEYS = Object.keys(EN) as OfficeAiStringKey[];
const TABLES: Record<Language, Record<OfficeAiStringKey, string>> = { en: EN, ar: AR };
export function officeAiText(language: Language, key: OfficeAiStringKey): string {
  return TABLES[language][key];
}
export const officeAiTranslator = (language: Language) => (key: OfficeAiStringKey) =>
  officeAiText(language, key);
export type OfficeAiTranslator = ReturnType<typeof officeAiTranslator>;
export function officeAiFormat(
  template: string,
  values: Record<string, string | number>,
): string {
  return Object.entries(values).reduce(
    (result, [key, value]) => result.replaceAll(`{${key}}`, String(value)),
    template,
  );
}
