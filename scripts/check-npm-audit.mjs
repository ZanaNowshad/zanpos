#!/usr/bin/env node
/**
 * `npm audit`, but with reviewed exceptions instead of a lower bar.
 *
 * The enforced severity floor stays at `high`. Two roots in the root
 * WebdriverIO/Tauri e2e toolchain are temporarily accepted because no patched
 * release exists: `extract-zip` via `@puppeteer/browsers`, and `braces` via
 * `@wdio/mocha-framework` → `mocha` → `chokidar`.
 *
 * These are root devDependencies used by the desktop e2e harness and are not
 * bundled into the Tauri installer. The production bundle contains built
 * frontend/storefront assets plus the independently audited WhatsApp sidecar.
 * Forced npm fixes would instead install breaking WebdriverIO downgrades.
 *
 * An exception only covers paths whose actual advisory root is named below.
 * Any other high/critical root still fails, and an exception becomes stale as
 * soon as its advisory disappears from a dependency tree where it is present.
 *
 * Usage: node scripts/check-npm-audit.mjs [--dir <path>] [--level high]
 */

import { execFileSync } from "node:child_process";
import process from "node:process";

/** Root modules whose advisories are accepted, with why and when to look again. */
const ACCEPTED = {
  "braces": {
    why:
      "GHSA-vfj7-8cjw-p6xm affects every published braces release through 3.0.3 " +
      "and GitHub lists no patched version. Reached only through " +
      "@wdio/mocha-framework → mocha → chokidar in the root WebdriverIO Tauri " +
      "e2e harness. Those root devDependencies are not bundled by Tauri; the " +
      "production bundle contains built frontend/storefront assets and the " +
      "separately audited WhatsApp sidecar, not this Mocha toolchain.",
    review: "2026-12-01",
  },
  "extract-zip": {
    why:
      "Vulnerable in every published version (unpatched upstream). Reached only " +
      "through @puppeteer/browsers → @wdio/utils, the WebdriverIO Tauri e2e " +
      "harness. It is a devDependency, it is not run by this job, and nothing " +
      "from it ships in the installer. The available 'fix' downgrades " +
      "@wdio/tauri-service to 1.0.0.",
    review: "2026-12-01",
  },
};

const args = process.argv.slice(2);
const dir = args.includes("--dir") ? args[args.indexOf("--dir") + 1] : process.cwd();
const level = args.includes("--level") ? args[args.indexOf("--level") + 1] : "high";
const RANK = { info: 0, low: 1, moderate: 2, high: 3, critical: 4 };
const floor = RANK[level] ?? RANK.high;

let report;
try {
  // `npm audit` exits non-zero when it finds anything, so the output is read
  // from the thrown result rather than treated as a failure on its own.
  // A shell is needed on Windows to resolve `npm` (a .cmd shim). Node warns
  // about that because unescaped arguments can become commands — here the two
  // arguments are string literals in this file and `dir` never reaches the
  // shell, so there is nothing for a caller to inject through.
  report = execFileSync("npm", ["audit", "--json"], {
    cwd: dir,
    encoding: "utf8",
    shell: process.platform === "win32",
  });
} catch (error) {
  report = error.stdout;
}
if (!report) {
  console.error("npm audit produced no output — failing closed.");
  process.exit(2);
}

const audit = JSON.parse(report);
const vulns = audit.vulnerabilities ?? {};

/**
 * The modules an advisory actually originates from, not the ones that depend on
 * it.
 *
 * npm reports two shapes in `via`: an object, which is the advisory itself, and
 * a bare string, which is "I am vulnerable because this other module is". The
 * chain has to be followed, or every package that merely depends on the real
 * culprit looks like its own root and no exception can ever match it.
 */
function roots(entry, seen = new Set()) {
  const found = new Set();
  for (const via of entry.via ?? []) {
    if (typeof via === "object" && via.name) {
      found.add(via.name);
    } else if (typeof via === "string" && !seen.has(via)) {
      seen.add(via);
      const next = vulns[via];
      if (next) for (const m of roots(next, seen)) found.add(m);
      else found.add(via);
    }
  }
  return found;
}

const blocking = [];
const excused = new Set();
for (const [name, entry] of Object.entries(vulns)) {
  if ((RANK[entry.severity] ?? 0) < floor) continue;
  const from = roots(entry);
  // An entry with no advisory of its own is "depends on something vulnerable";
  // it is excused exactly when everything underneath it is.
  const source = from.size ? from : new Set([name]);
  const unexcused = [...source].filter((m) => !(m in ACCEPTED));
  if (unexcused.length === 0) {
    for (const m of source) excused.add(m);
  } else {
    blocking.push(`${entry.severity.padEnd(8)} ${name}  (via ${unexcused.join(", ")})`);
  }
}

// An exception is stale when the module is in this tree and no longer has an
// advisory. A module that is absent entirely is simply not applicable here —
// this script runs against three separate package trees, and only one of them
// has ever contained the harness these exceptions are about.
const stale = Object.keys(ACCEPTED).filter((m) => m in vulns && !excused.has(m));

if (blocking.length) {
  console.error(`\nnpm audit: ${blocking.length} advisory/advisories at or above "${level}" with no accepted exception:\n`);
  for (const line of blocking) console.error("  " + line);
  console.error("\nFix them, or add the root module to ACCEPTED in scripts/check-npm-audit.mjs with a reason.\n");
}
if (stale.length) {
  console.error(`\nStale exception(s) in scripts/check-npm-audit.mjs — no longer needed, remove them:\n`);
  for (const m of stale) console.error("  " + m);
  console.error("");
}
if (blocking.length || stale.length) process.exit(1);

const counts = audit.metadata?.vulnerabilities ?? {};
console.log(
  `npm audit clean at "${level}" in ${dir} ` +
  `(${excused.size ? [...excused].join(", ") + " accepted by documented exception" : "no exceptions applied"}; ` +
  `raw totals ${JSON.stringify(counts)})`,
);
