/** Namespace keys for i18next resource loading. */
export type I18nNamespace = "pos" | "backOffice" | "detail" | "modal" | "officeAi" | "officeAiTool" | "operations";

/** Strong-typed translation function for the `pos` namespace. */
export type PosTranslator = (key: string) => string;
