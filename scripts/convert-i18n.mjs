// Extract EN/AR objects from ZANPOS i18n TypeScript dictionary files
// and emit JSON locale files for i18next.
// Usage: node scripts/convert-i18n.mjs

import { readFileSync, writeFileSync, mkdirSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const I18N_DIR = join(__dirname, "..", "src", "i18n");
const LOCALES_EN = join(I18N_DIR, "locales", "en");
const LOCALES_AR = join(I18N_DIR, "locales", "ar");

const FILES = [
  { src: "backOfficeStrings.ts", ns: "backOffice" },
  { src: "detailStrings.ts", ns: "detail" },
  { src: "modalStrings.ts", ns: "modal" },
  { src: "officeAiStrings.ts", ns: "officeAi" },
  { src: "operationsStrings.ts", ns: "operations" },
  // officeAiToolStrings uses different names: OFFICE_AI_TOOL_LABELS_EN / OFFICE_AI_TOOL_LABELS_AR
  { src: "officeAiToolStrings.ts", ns: "officeAiTool", enMarker: "OFFICE_AI_TOOL_LABELS_EN =" },
];

for (const dir of [LOCALES_EN, LOCALES_AR]) {
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
}

function extractObject(src, marker) {
  const startIdx = src.indexOf(marker);
  if (startIdx === -1) throw new Error(`Marker ${marker} not found`);
  // Find opening brace after the marker
  let idx = src.indexOf("{", startIdx);
  if (idx === -1) throw new Error("Opening brace not found after " + marker);
  let depth = 1;
  let end = idx + 1;
  // Track string context
  let inString = false, stringChar = null;
  for (let i = idx + 1; i < src.length && depth > 0; i++) {
    const ch = src[i];
    if (inString) {
      if (ch === "\\") { i++; continue; }
      if (ch === stringChar) inString = false;
      else continue;
    } else {
      if (ch === '"' || ch === "'" || ch === "`") { inString = true; stringChar = ch; }
      else if (ch === "{") depth++;
      else if (ch === "}") { depth--; if (depth === 0) end = i + 1; }
    }
  }
  return src.substring(idx, end);
}

function tsObjectToJSON(tsCode) {
  // Convert TS const object to JSON:
  // 1. Remove comments
  tsCode = tsCode.replace(/\/\/.*$/gm, "").replace(/\/\*[\s\S]*?\*\//g, "");
  // 2. Quote unquoted keys
  tsCode = tsCode.replace(/([{,]\s*)([a-zA-Z_$][\w$]*)\s*:/g, '$1"$2":');
  // 3. Convert single-quoted strings to double-quoted
  tsCode = tsCode.replace(/:\s*'([^'\\]*(?:\\.[^'\\]*)*)'/g, (_, s) => {
    return ': "' + s.replace(/\\'/g, "'").replace(/"/g, '\\"') + '"';
  });
  // 4. Remove trailing commas before closing brace
  tsCode = tsCode.replace(/,(\s*})/g, "$1");
  // 5. Remove TS type annotations in comments (already handled)
  return tsCode;
}

for (const { src, ns, enMarker, arMarker } of FILES) {
  const content = readFileSync(join(I18N_DIR, src), "utf-8");

  const enMark = enMarker || "const EN =";
  const arMark = arMarker || "const AR:";

  const enRaw = extractObject(content, enMark);
  const enJSON = tsObjectToJSON(enRaw);
  writeFileSync(join(LOCALES_EN, ns + ".json"), enJSON, "utf-8");

  const arRaw = extractObject(content, arMark);
  const arJSON = tsObjectToJSON(arRaw);
  writeFileSync(join(LOCALES_AR, ns + ".json"), arJSON, "utf-8");

  console.log(`Converted ${src} -> ${ns}.json (EN + AR)`);
}

console.log("Done.");
