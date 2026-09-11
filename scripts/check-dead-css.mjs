/**
 * Report CSS class selectors that nothing in the app appears to render.
 *
 * `src/App.css` is 18,949 lines and ordered by development phase rather than by
 * concern, so rules outlive the components that needed them without anything
 * noticing — the compiler has no opinion about CSS, and the bundler keeps
 * whatever the stylesheet declares. Ledger CSS1.
 *
 * Method: collect every class in a selector position across `src/**` *.css`,
 * then look for that exact token anywhere in the `.ts`/`.tsx` sources and
 * `index.html`. A class is reported only when the token never appears.
 *
 * Two things stop that being naive:
 *
 *   - Tokens are whole `[\w-]+` runs, so `action-btn` is not credited to
 *     `dlv-action-btn`. A substring check would call almost everything live.
 *   - Class names built from template literals (`` `oa-pulse-${level}` ``) are
 *     detected and their prefixes treated as live, so the halves that only ever
 *     exist at runtime are not reported.
 *
 * It is still a heuristic and deliberately advisory: it exits 0 and prints, so
 * nothing is deleted on its say-so. A class assembled some way this does not
 * model would be a false positive, and a stylesheet is not worth a regression.
 * Read the report, check the component, then delete.
 *
 *   node scripts/check-dead-css.mjs          summary
 *   node scripts/check-dead-css.mjs --list   every candidate with its location
 */
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const SRC = join(ROOT, "src");
const rel = (p) => relative(ROOT, p).split(sep).join("/");

function walk(dir, out = []) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) walk(full, out);
    else out.push(full);
  }
  return out;
}

const files = walk(SRC);
const cssFiles = files.filter((f) => f.endsWith(".css"));
const codeFiles = files.filter((f) => /\.(ts|tsx)$/.test(f));

/** class name -> selector sites that define it */
const defined = new Map();
for (const file of cssFiles) {
  readFileSync(file, "utf8")
    .split(/\r?\n/)
    .forEach((line, i) => {
      if (!line.includes("{")) return;
      const selector = line.split("{")[0];
      for (const match of selector.matchAll(/\.(-?[_a-zA-Z][\w-]*)/g)) {
        const cls = match[1];
        if (!defined.has(cls)) defined.set(cls, []);
        defined.get(cls).push(`${rel(file)}:${i + 1}`);
      }
    });
}

const indexHtml = join(ROOT, "index.html");
const blob =
  codeFiles.map((f) => readFileSync(f, "utf8")).join("\n") +
  (existsSync(indexHtml) ? readFileSync(indexHtml, "utf8") : "");

const tokens = new Set();
for (const match of blob.matchAll(/[\w-]+/g)) tokens.add(match[0]);

// `className={`oa-pulse-${level}`}` — the suffix only exists at runtime, so
// treat the literal prefix as covering every class beneath it.
const dynamicPrefixes = new Set();
for (const match of blob.matchAll(/`([^`]*?)\$\{/g)) {
  const trailing = match[1].split(/\s/).pop();
  if (trailing && /[\w-]+-$/.test(trailing)) dynamicPrefixes.add(trailing);
}

const candidates = [];
for (const [cls, sites] of defined) {
  if (tokens.has(cls)) continue;
  if ([...dynamicPrefixes].some((p) => cls.startsWith(p))) continue;
  candidates.push({ cls, sites });
}
candidates.sort((a, b) => a.cls.localeCompare(b.cls));

const byFile = new Map();
for (const { sites } of candidates) {
  const file = sites[0].split(":")[0];
  byFile.set(file, (byFile.get(file) ?? 0) + 1);
}

console.log(`CSS files scanned            ${cssFiles.length}`);
console.log(`Class selectors defined      ${defined.size}`);
console.log(`Dynamic prefixes honoured    ${dynamicPrefixes.size}`);
console.log(`Never referenced in ts/tsx   ${candidates.length}`);
console.log("");
console.log("By stylesheet:");
for (const [file, n] of [...byFile].sort((a, b) => b[1] - a[1])) {
  console.log(`  ${String(n).padStart(4)}  ${file}`);
}

if (process.argv.includes("--list")) {
  console.log("");
  for (const { cls, sites } of candidates) {
    const extra = sites.length > 1 ? ` (+${sites.length - 1} more)` : "";
    console.log(`  .${cls}  ${sites[0]}${extra}`);
  }
} else if (candidates.length) {
  console.log("");
  console.log("Re-run with --list for each candidate and where it is defined.");
}

// Advisory by design — see the header. Never fails a build.
process.exit(0);
