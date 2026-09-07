#!/usr/bin/env node
/**
 * `npm audit`, but with reviewed exceptions instead of a lower bar.
 *
 * The gate this replaces was `npm audit --audit-level=high` over every
 * dependency, and the reasoning behind that is right: vitest and its Vite
 * toolchain execute in CI, so a vulnerable dev dependency is not automatically
 * harmless. The problem is that it had become a gate nobody could pass. Every
 * remaining advisory traces to `extract-zip`, reached through
 * `@puppeteer/browsers` under the WebdriverIO Tauri harness, and its advisory
 * covers *every published version* — there is no release to move to. npm's own
 * suggested fix is to install `@wdio/tauri-service@1.0.0`, a downgrade of the
 * Tauri integration offered as a major change.
 *
 * A gate that cannot go green stops being read. It gets a `|| true`, or an
 * `--audit-level=critical`, and then the advisories that *do* matter arrive
 * inside the same silence. So the level stays where it was and the exception is
 * named instead: one module, one reason, and a date someone has to look at.
 *
 * Two properties make this stricter than what it replaces, not looser:
 *
 *   - An exception only covers advisories whose root module is listed. A new
 *     advisory anywhere else fails the build at the same severity as before.
 *   - A listed module that no longer has an advisory fails the build too. The
 *     exception cannot outlive the reason for it, which is the usual way these
 *     lists rot into blanket suppression.
 *
 * Usage: node scripts/check-npm-audit.mjs [--dir <path>] [--level high]
 */

import { execFileSync } from "node:child_process";
import process from "node:process";

/** Root modules whose advisories are accepted, with why and when to look again. */
const ACCEPTED = {
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
