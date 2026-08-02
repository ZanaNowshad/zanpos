/**
 * Compatibility bridge between legacy typed-record translators and i18next.
 *
 * ## Current state (2026-08-02)
 *
 * i18next v26.3.6 + react-i18next v17.0.11 are initialized with 7 namespaces
 * (pos, backOffice, detail, modal, officeAi, officeAiTool, operations) across
 * EN and AR. Resources are bundled at build time (offline-only). Document
 * direction (dir="ltr"/"rtl") is updated via i18next language change events.
 *
 * ## Migration plan
 *
 * 82 files currently import bespoke typed-record translators (e.g.
 * operationsTranslator, modalTranslator) from the legacy *Strings.ts modules.
 * These modules maintain their own EN/AR records that duplicate the i18next
 * JSON locale files. The migration to direct useTranslation() hooks proceeds
 * in this order:
 *
 *   1. Authentication and POS (PosPage, LoginPage, payment flow)
 *   2. Operations and settings (back-office tabs, settings panels)
 *   3. Office AI and storefront
 *
 * Per-component migration steps:
 *   1. Replace `import { xxxTranslator } from "../i18n/xxxStrings"`
 *      with `import { useTranslation } from "react-i18next"`
 *   2. Replace `const t = useMemo(() => xxxTranslator(language), [language])`
 *      with `const { t } = useTranslation("xxx")`
 *   3. Verify EN/AR parity tests pass for the namespace
 *
 * This compat module remains until all consumers are migrated. After migration:
 *   - Remove compat.ts
 *   - Remove legacy *Strings.ts modules (or repurpose as type-only)
 *   - Remove duplicate EN/AR records from legacy modules
 *
 * Owner: ZANPOS Maintainers. Review by: 2027-01-31.
 */

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
