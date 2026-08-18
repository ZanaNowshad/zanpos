/**
 * Unified command-centre translator — re-exports all OfficeAI, back-office,
 * and operations strings from a single module so page components have one
 * import instead of three.
 *
 * The underlying string buckets are unchanged; this is a convenience barrel.
 */
export {
  officeAiText as commandText,
  officeAiTranslator as commandTranslator,
  officeAiFormat as commandFormat,
  OFFICE_AI_STRING_KEYS as COMMAND_STRING_KEYS,
  type OfficeAiStringKey as CommandStringKey,
  type OfficeAiTranslator as CommandTranslator,
} from "./officeAiStrings";

// Back-office strings (settings labels, reports, etc.)
export { backOfficeTranslator } from "./backOfficeStrings";

// Operations strings (purchasing, deliveries, customers, users)
export { operationsTranslator } from "./operationsStrings";
