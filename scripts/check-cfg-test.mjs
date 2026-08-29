/**
 * Standalone runner for the ship gate's "test modules are cfg-gated" rule.
 *
 * Same logic, callable on its own so the check can be exercised without a
 * thirty-minute release build behind it. `npm run ship` remains the gate; this
 * is for confirming the rule itself behaves.
 */
import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = dirname(dirname(fileURLToPath(import.meta.url)));

function walk(dir, exts, out = []) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) walk(full, exts, out);
    else if (exts.some(e => full.endsWith(e))) out.push(full);
  }
  return out;
}

export function ungatedTestModules() {
  const offenders = [];
  for (const file of walk(join(ROOT, "src-tauri", "src"), [".rs"])) {
    const lines = readFileSync(file, "utf8").split("\n");
    lines.forEach((line, i) => {
      if (!/^\s*mod\s+\w*tests?\s*;/.test(line)) return;
      let j = i - 1;
      while (j >= 0 && /^\s*#\[/.test(lines[j])) {
        if (/cfg\(test\)/.test(lines[j])) return;
        j--;
      }
      offenders.push(`${relative(ROOT, file).split(sep).join("/")}:${i + 1}`);
    });
  }
  return offenders;
}

const offenders = ungatedTestModules();
console.log(offenders.length ? offenders.join("\n") : "none — every test module is gated");
process.exit(offenders.length ? 1 : 0);
