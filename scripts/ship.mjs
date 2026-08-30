#!/usr/bin/env node
/**
 * ZANPOS ship gate.
 *
 * Every installer that reaches a store must come out of this script. It runs the
 * checks that historically caught real breakage — including the release build,
 * which is where the fat-LTO stack overflow lived and which `cargo check` alone
 * will never surface — and refuses to continue on the first hard failure.
 *
 * Usage:
 *   npm run ship                  full gate, ending in a release build
 *   npm run ship -- --no-build    everything except the release build (fast loop)
 *   npm run ship -- --update-allowlist
 *                                 rewrite the oversized-file allowlist from the
 *                                 current tree (run deliberately, not casually)
 */

import { spawnSync } from "node:child_process";
import { readFileSync, writeFileSync, readdirSync, statSync, existsSync, rmSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join, relative, sep } from "node:path";
import { loadDotEnv } from "./dotenv.mjs";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
// Before anything spawns: the release build only signs if the signing key and
// its password are in the environment it inherits, and an unsigned build is one
// every till correctly refuses.
loadDotEnv(ROOT);

// The bundler documents TAURI_SIGNING_PRIVATE_KEY as "path or content". Rather
// than depend on which, resolve a path to its content here — the value the
// bundler receives is then unambiguous. This costs nothing and removes a
// failure that only surfaces at the very end of an hour-long build, having
// produced a perfectly good installer with no signature beside it.
if (
  process.env.TAURI_SIGNING_PRIVATE_KEY &&
  existsSync(process.env.TAURI_SIGNING_PRIVATE_KEY)
) {
  process.env.TAURI_SIGNING_PRIVATE_KEY = readFileSync(
    process.env.TAURI_SIGNING_PRIVATE_KEY,
    "utf8",
  ).trim();
}

const ALLOWLIST_PATH = join(ROOT, "scripts", "oversized-files-allowlist.json");
const MAX_LINES = 500;

const args = process.argv.slice(2);
const SKIP_BUILD = args.includes("--no-build");
const UPDATE_ALLOWLIST = args.includes("--update-allowlist");

const failures = [];
const warnings = [];

const c = {
  reset: "\x1b[0m", bold: "\x1b[1m", dim: "\x1b[2m",
  red: "\x1b[31m", green: "\x1b[32m", yellow: "\x1b[33m", cyan: "\x1b[36m",
};

let stepNo = 0;
function step(title) {
  stepNo += 1;
  console.log(`\n${c.bold}${c.cyan}[${stepNo}] ${title}${c.reset}`);
}
function pass(msg) { console.log(`    ${c.green}PASS${c.reset}  ${msg}`); }
function warn(msg) { console.log(`    ${c.yellow}WARN${c.reset}  ${msg}`); warnings.push(msg); }
function fail(msg) { console.log(`    ${c.red}FAIL${c.reset}  ${msg}`); failures.push(msg); }

/** Run a command, streaming its output. Returns true on exit code 0. */
function run(cmd, cwd) {
  console.log(`    ${c.dim}$ ${cmd}${c.dim} (in ${relative(ROOT, cwd) || "."})${c.reset}`);
  const res = spawnSync(cmd, { cwd, shell: true, stdio: "inherit" });
  return res.status === 0;
}

/** Hard gate: on failure, stop the whole run immediately. */
function gate(title, cmd, cwd, label) {
  step(title);
  if (run(cmd, cwd)) {
    pass(label);
    return true;
  }
  fail(`${label} — see output above`);
  report();
  process.exit(1);
}

// ---------------------------------------------------------------- source scan

const SCAN = [
  { dir: join(ROOT, "src"), exts: [".ts", ".tsx"] },
  { dir: join(ROOT, "src-tauri", "src"), exts: [".rs"] },
  { dir: join(ROOT, "storefront", "src"), exts: [".ts", ".tsx"] },
  { dir: join(ROOT, "storefront", "worker"), exts: [".ts", ".js"] },
];
const SKIP_DIRS = new Set(["node_modules", "dist", "target", "gen", ".git"]);

function walk(dir, exts, out = []) {
  if (!existsSync(dir)) return out;
  for (const entry of readdirSync(dir)) {
    if (SKIP_DIRS.has(entry) || entry.startsWith("target")) continue;
    const full = join(dir, entry);
    const st = statSync(full);
    if (st.isDirectory()) walk(full, exts, out);
    else if (exts.some(e => entry.endsWith(e))) out.push(full);
  }
  return out;
}

function oversizedFiles() {
  const found = [];
  for (const { dir, exts } of SCAN) {
    for (const file of walk(dir, exts)) {
      const lines = readFileSync(file, "utf8").split("\n").length;
      if (lines > MAX_LINES) {
        found.push({ path: relative(ROOT, file).split(sep).join("/"), lines });
      }
    }
  }
  return found.sort((a, b) => b.lines - a.lines);
}

// ------------------------------------------------------------ allowlist mode

if (UPDATE_ALLOWLIST) {
  const found = oversizedFiles();
  writeFileSync(
    ALLOWLIST_PATH,
    JSON.stringify(
      {
        _comment:
          "Files already over the 500-line rule when the ship gate was introduced. " +
          "These are warnings, not failures. Any file NOT listed here that crosses " +
          "500 lines fails the gate. Shrink entries out of this list; do not add to it.",
        maxLines: MAX_LINES,
        allow: found.map(f => f.path),
      },
      null,
      2,
    ) + "\n",
  );
  console.log(`Wrote ${found.length} entries to ${relative(ROOT, ALLOWLIST_PATH)}`);
  process.exit(0);
}

// ------------------------------------------------------------------ the gate

console.log(`${c.bold}ZANPOS ship gate${c.reset}${SKIP_BUILD ? `  ${c.dim}(--no-build)${c.reset}` : ""}`);

gate("Frontend: types, lint, tests", "npm run check", ROOT, "tsc + eslint + vitest clean");

gate("Rust: cargo check", "cargo check", join(ROOT, "src-tauri"), "src-tauri compiles");

step("Storefront: types");
if (existsSync(join(ROOT, "storefront", "node_modules"))) {
  // storefront uses `typecheck`; the root package uses `type-check`. Not a typo.
  if (run("npm run typecheck", join(ROOT, "storefront"))) pass("storefront tsc clean");
  else { fail("storefront tsc failed"); report(); process.exit(1); }
} else {
  warn("storefront/node_modules missing — skipped (run npm install in storefront/)");
}

step(`File size rule (max ${MAX_LINES} lines)`);
{
  let allow = [];
  if (existsSync(ALLOWLIST_PATH)) {
    try {
      allow = JSON.parse(readFileSync(ALLOWLIST_PATH, "utf8")).allow ?? [];
    } catch {
      warn("allowlist unreadable — treating every oversized file as new");
    }
  } else {
    warn("no allowlist found — run: npm run ship -- --update-allowlist");
  }
  const found = oversizedFiles();
  const known = found.filter(f => allow.includes(f.path));
  const fresh = found.filter(f => !allow.includes(f.path));

  for (const f of fresh) fail(`${f.path} is ${f.lines} lines (limit ${MAX_LINES}) — split it before shipping`);
  if (known.length) {
    console.log(`    ${c.dim}${known.length} pre-existing oversized file(s), allowed:${c.reset}`);
    for (const f of known.slice(0, 8)) console.log(`      ${c.dim}${f.lines.toString().padStart(5)}  ${f.path}${c.reset}`);
    if (known.length > 8) console.log(`      ${c.dim}… and ${known.length - 8} more${c.reset}`);
  }
  const shrunk = allow.filter(p => !found.some(f => f.path === p));
  if (shrunk.length) warn(`${shrunk.length} allowlisted file(s) are now under the limit — drop them from the allowlist`);
  if (!fresh.length) pass("no new oversized files");
}

step("One version, everywhere");
{
  // tauri.conf.json is the authority — it is what the bundler stamps on the
  // installer and what the release manifest advertises. The other two are
  // checked against it rather than merged into it, because a mismatch is not a
  // formatting problem: if Cargo.toml lags, `env!("CARGO_PKG_VERSION")` reports
  // the old number, and that value is what `download_and_install_update` writes
  // into the update_applied audit row and what a terminal reports as its
  // installed version. The shop would then see a till claiming a version it is
  // not running.
  const conf = JSON.parse(readFileSync(join(ROOT, "src-tauri", "tauri.conf.json"), "utf8"));
  const pkg = JSON.parse(readFileSync(join(ROOT, "package.json"), "utf8"));
  const cargo = /^version\s*=\s*"([^"]+)"/m.exec(
    readFileSync(join(ROOT, "src-tauri", "Cargo.toml"), "utf8"),
  );

  const found = {
    "tauri.conf.json": conf.version,
    "package.json": pkg.version,
    "src-tauri/Cargo.toml": cargo?.[1],
  };
  const wrong = Object.entries(found).filter(([, v]) => v !== conf.version);
  if (wrong.length) {
    for (const [file, v] of wrong) {
      fail(`${file} says ${v ?? "(unreadable)"}, tauri.conf.json says ${conf.version}`);
    }
  } else if (!/^\d+\.\d+\.\d+$/.test(conf.version)) {
    fail(`version ${conf.version} is not a plain x.y.z — the updater compares these`);
  } else {
    pass(`${conf.version} in all three`);
  }
}

step("Test modules are cfg-gated");
{
  // `#[cfg(test)]` applies to the item immediately after it, so inserting a new
  // `mod foo_tests;` above an existing one silently ungates the one below. That
  // compiles — `#[test]` is always a valid attribute — so nothing complains,
  // and the release binary quietly gains test code and its fixture strings.
  // The JS side already guards the equivalent leak in devMockIsolation.test.ts;
  // this is the Rust half.
  const offenders = [];
  for (const file of walk(join(ROOT, "src-tauri", "src"), [".rs"])) {
    const lines = readFileSync(file, "utf8").split("\n");
    lines.forEach((line, i) => {
      if (!/^\s*mod\s+\w*tests?\s*;/.test(line)) return;
      // Look back past attributes such as #[path = "…"] to the nearest
      // non-attribute line: the gate does not have to be adjacent.
      let j = i - 1;
      while (j >= 0 && /^\s*#\[/.test(lines[j])) {
        if (/cfg\(test\)/.test(lines[j])) return;
        j--;
      }
      offenders.push(`${relative(ROOT, file).split(sep).join("/")}:${i + 1}`);
    });
  }
  for (const o of offenders) fail(`${o} declares a test module without #[cfg(test)] — it would compile into the release binary`);
  if (!offenders.length) pass("every test module is gated out of release builds");
}

step("Worker copies in sync");
{
  // storefront/worker/index.ts is the readable source; worker_embedded.js is what
  // actually ships to stores. Nothing syncs them, so a route added to one and not
  // the other silently works in dev and dies in the field.
  const a = join(ROOT, "storefront", "worker", "index.ts");
  const b = join(ROOT, "src-tauri", "src", "storefront", "worker_embedded.js");
  if (existsSync(a) && existsSync(b)) {
    const routes = f => new Set((readFileSync(f, "utf8").match(/["'`]\/api\/[a-zA-Z0-9/_-]+/g) ?? [])
      .map(s => s.slice(1)));
    const ra = routes(a), rb = routes(b);
    const onlyA = [...ra].filter(r => !rb.has(r));
    const onlyB = [...rb].filter(r => !ra.has(r));
    if (onlyA.length || onlyB.length) {
      if (onlyA.length) warn(`routes only in worker/index.ts: ${onlyA.join(", ")}`);
      if (onlyB.length) warn(`routes only in worker_embedded.js: ${onlyB.join(", ")}`);
    } else {
      pass(`${ra.size} route(s) present in both copies`);
    }
  } else {
    warn("one of the worker copies is missing — skipped");
  }
}

if (!SKIP_BUILD) {
  step("Updater signing works");
  {
    // Proven by signing a throwaway file, not by checking that a variable is
    // non-empty. The bundler signs at the very END of the release build, so a
    // misconfigured key fails 56 minutes in — which is exactly how long it took
    // to discover that the bundler reads TAURI_SIGNING_PRIVATE_KEY while the
    // `signer` CLI reads TAURI_SIGNING_PRIVATE_KEY_PATH. Setting only the latter
    // builds perfectly and silently produces no signature.
    //
    // Two seconds here, and the failure names itself.
    const probe = join(ROOT, "src-tauri", "target", "_signing-probe.txt");
    if (!process.env.TAURI_SIGNING_PRIVATE_KEY) {
      fail("TAURI_SIGNING_PRIVATE_KEY is not set — the build would emit no .sig");
      report();
      process.exit(1);
    }
    writeFileSync(probe, "signing probe\n");
    const signed = run(`npx tauri signer sign "${probe}" < ${sep === "\\" ? "NUL" : "/dev/null"}`, ROOT);
    const sigPath = `${probe}.sig`;
    const ok = signed && existsSync(sigPath) && readFileSync(sigPath, "utf8").trim().length > 0;
    for (const f of [probe, sigPath]) if (existsSync(f)) rmSync(f);
    if (!ok) {
      fail("the signing key did not produce a signature — fix this before building");
      report();
      process.exit(1);
    }
    pass("key signs; the release build will emit a .sig");
  }

  gate(
    "Release build (the one that catches release-only crashes)",
    "npm run tauri build",
    ROOT,
    "release installer built",
  );
} else {
  step("Release build");
  warn("skipped via --no-build — NOT shippable to a store");
}

report();

const CHECKLIST = `
${c.bold}Hardware smoke test — run on the real machine before any store gets this build.${c.reset}
A green gate proves the code compiles and the tests pass. It does not prove the
printer prints or that WhatsApp is paired. Five minutes, in order:

   [ ] 1. Launch the release build (not dev) and log in with a PIN
   [ ] 2. Complete one cash sale, scanner included
   [ ] 3. Receipt prints on the store's own paper — then unplug the printer,
          sell again, confirm the sale still completes and the failure is visible
   [ ] 4. Send one WhatsApp message from the app
   [ ] 5. Ask ZanAI one question and get an answer
   [ ] 6. Publish the storefront and load the public URL on a phone
   [ ] 7. Settings → Send diagnostics returns a result
   [ ] 8. Close and relaunch: no crash banner, no lost data
`;

if (failures.length === 0) console.log(CHECKLIST);

function report() {
  console.log(`\n${c.bold}${"─".repeat(60)}${c.reset}`);
  if (failures.length) {
    console.log(`${c.red}${c.bold}SHIP GATE FAILED${c.reset}  ${failures.length} failure(s), ${warnings.length} warning(s)`);
    for (const f of failures) console.log(`  ${c.red}✗${c.reset} ${f}`);
  } else {
    console.log(`${c.green}${c.bold}SHIP GATE PASSED${c.reset}  ${warnings.length} warning(s)`);
  }
  for (const w of warnings) console.log(`  ${c.yellow}!${c.reset} ${w}`);
}

process.exit(failures.length ? 1 : 0);
