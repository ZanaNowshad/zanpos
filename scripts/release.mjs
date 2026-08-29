#!/usr/bin/env node
/**
 * ZANPOS release publisher.
 *
 * Takes the artifacts `npm run ship` has already built, and puts them where
 * installed tills will find them. This is the step that replaces walking round
 * the shop with a USB stick.
 *
 * It deliberately does not build. `npm run ship` is the gate that decides
 * whether a build may reach a store, and a publisher that could build would be
 * a second way to produce an installer — one that skips the gate. Run the gate,
 * then run this.
 *
 * Usage:
 *   npm run release                 publish the current build
 *   npm run release -- --critical   mark it critical (tills refuse a new shift)
 *   npm run release -- --dry-run    show exactly what would be uploaded
 *
 * Environment (see .env.example):
 *   R2_BUCKET          bucket name, e.g. zanpos-releases
 *   R2_PUBLIC_BASE     public base URL the tills read, no trailing slash
 *
 * Everything here also works unchanged inside CI: a GitHub Actions job that
 * wants to publish unattended runs the gate and then this script, with the same
 * two variables as repository secrets. That is why the upload lives in a script
 * rather than in a workflow file.
 */

import { spawnSync } from "node:child_process";
import { readFileSync, writeFileSync, existsSync, readdirSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { loadDotEnv } from "./dotenv.mjs";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const NSIS_DIR = join(ROOT, "src-tauri", "target", "release", "bundle", "nsis");
const CONF_PATH = join(ROOT, "src-tauri", "tauri.conf.json");

/** Wrangler refuses anything larger, and says so only after the upload fails. */
const WRANGLER_MAX_BYTES = 315 * 1024 * 1024;

const args = process.argv.slice(2);
const DRY_RUN = args.includes("--dry-run");
const CRITICAL = args.includes("--critical");

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
function info(msg) { console.log(`    ${c.dim}${msg}${c.reset}`); }
function die(msg, hint) {
  console.log(`\n    ${c.red}${c.bold}STOP${c.reset}  ${msg}`);
  if (hint) console.log(`\n${hint}\n`);
  process.exit(1);
}

function mb(bytes) { return `${(bytes / 1024 / 1024).toFixed(1)} MB`; }

loadDotEnv(ROOT);

// ── 1. Where the tills are actually looking ──────────────────────────────────
//
// Checked before anything is uploaded. Publishing to a URL no device asks for
// produces a release that looks successful from here and reaches nobody, which
// is worse than failing: the shop believes it is on the new version.

step("Destination agrees with what the app was built to check");

const bucket = process.env.R2_BUCKET;
const publicBase = (process.env.R2_PUBLIC_BASE ?? "").replace(/\/+$/, "");
if (!bucket || !publicBase) {
  die(
    "R2_BUCKET and R2_PUBLIC_BASE are not both set.",
    `    Put them in ${c.bold}.env${c.reset} at the repo root:\n` +
    `      R2_BUCKET=zanpos-releases\n` +
    `      R2_PUBLIC_BASE=https://pub-xxxxxxxx.r2.dev\n`,
  );
}

const conf = JSON.parse(readFileSync(CONF_PATH, "utf8"));
const endpoints = conf.plugins?.updater?.endpoints ?? [];
const manifestUrl = `${publicBase}/latest.json`;

if (!endpoints.includes(manifestUrl)) {
  die(
    `No endpoint in tauri.conf.json matches ${manifestUrl}`,
    `    The installed tills only ever ask the URL that was compiled into them.\n` +
    `    Endpoint(s) currently compiled in:\n` +
    endpoints.map(e => `      ${e}\n`).join("") +
    `\n    Either set R2_PUBLIC_BASE to match one of those, or change the endpoint\n` +
    `    in tauri.conf.json ${c.bold}and rebuild${c.reset} — a config edit alone does not\n` +
    `    reach a till that is already installed.\n`,
  );
}
pass(`tills check ${manifestUrl}`);

// ── 2. The artifacts, and whether they were actually signed ──────────────────

step("Signed updater artifacts");

if (!existsSync(NSIS_DIR)) {
  die(
    "No NSIS bundle directory — nothing has been built.",
    `    Run ${c.bold}npm run ship${c.reset} first.\n`,
  );
}

const files = readdirSync(NSIS_DIR);

// Which file the updater actually downloads depends on the Tauri version. The
// docs describe `.nsis.zip` + `.nsis.zip.sig`; this toolchain signs the
// installer `.exe` directly with no zip wrapper. Both are accepted rather than
// assuming, because guessing wrong here produces a manifest pointing at a file
// that was never uploaded — which fails on the till, not here.
const signature = files.find(f => f.endsWith(".sig"));
const archive = signature ? signature.slice(0, -".sig".length) : undefined;

if (!archive || !signature || !existsSync(join(NSIS_DIR, archive))) {
  die(
    "The build produced no updater archive or no signature.",
    `    Found: ${files.join(", ") || "(nothing)"}\n\n` +
    `    Two things must both be true, and each fails silently on its own:\n` +
    `      • ${c.bold}bundle.createUpdaterArtifacts${c.reset} is true in tauri.conf.json\n` +
    `      • ${c.bold}TAURI_SIGNING_PRIVATE_KEY_PATH${c.reset} and\n` +
    `        ${c.bold}TAURI_SIGNING_PRIVATE_KEY_PASSWORD${c.reset} were set when the build ran\n\n` +
    `    Without the key Tauri emits the archive but no .sig, and an unsigned\n` +
    `    update is one every till will refuse — correctly.\n`,
  );
}

const archivePath = join(NSIS_DIR, archive);
const archiveBytes = statSync(archivePath).size;
if (archiveBytes > WRANGLER_MAX_BYTES) {
  die(
    `${archive} is ${mb(archiveBytes)} — over wrangler's ${mb(WRANGLER_MAX_BYTES)} ceiling.`,
    `    Wrangler cannot upload it. Switch this step to rclone or any S3-compatible\n` +
    `    client against the R2 S3 endpoint, both of which do multipart uploads.\n`,
  );
}

// A signature file that exists but is empty would upload happily and fail on
// every till, so it is read and checked rather than merely found.
const sig = readFileSync(join(NSIS_DIR, signature), "utf8").trim();
if (!sig) die(`${signature} is empty — the build did not really sign anything.`);

pass(`${archive} (${mb(archiveBytes)}) with a ${sig.length}-char signature`);

// ── 3. The manifest ──────────────────────────────────────────────────────────

step("Manifest");

const version = conf.version;
if (!version) die("tauri.conf.json has no version.");

const manifest = {
  version,
  pub_date: new Date().toISOString(),
  // Read by check_critical_update, which is a deliberately unsigned advisory
  // read used only to decide how hard to push the prompt. It can never cause an
  // install — that stays on the signed path — so a wrong value here is a UI
  // annoyance, not a security question.
  critical: CRITICAL,
  notes: `ZANPOS ${version}`,
  platforms: {
    "windows-x86_64": {
      signature: sig,
      url: `${publicBase}/${version}/${archive}`,
    },
  },
};

info(`version ${version}${CRITICAL ? "  (critical)" : ""}`);
info(`installer → ${manifest.platforms["windows-x86_64"].url}`);
info(`manifest  → ${manifestUrl}`);

if (DRY_RUN) {
  console.log(`\n${c.dim}${JSON.stringify(manifest, null, 2)}${c.reset}`);
  console.log(`\n${c.yellow}Dry run — nothing uploaded.${c.reset}\n`);
  process.exit(0);
}

// ── 4. Upload, installer first ───────────────────────────────────────────────
//
// The order is the whole correctness argument of this step. A till reads the
// manifest and immediately fetches the URL inside it, so publishing the manifest
// first opens a window where it names a file that does not exist yet. Every till
// that checks during that window fails its update, and the failure looks like a
// broken release rather than a race.
//
// Cache headers matter for the same reason in reverse: the archive never changes
// for a given version and can be cached hard, while the manifest is the thing
// that must be seen promptly or a release simply does not arrive.

step("Upload");

function wrangler(key, file, extra) {
  const cmd =
    `npx wrangler r2 object put "${bucket}/${key}" --file="${file}" --remote ${extra}`;
  console.log(`    ${c.dim}$ ${cmd}${c.reset}`);
  const res = spawnSync(cmd, { cwd: ROOT, shell: true, stdio: "inherit" });
  if (res.status !== 0) die(`Upload of ${key} failed.`);
}

// Derived, not assumed: which artifact the toolchain emits varies by Tauri
// version, and hard-coding application/zip mislabelled a bare .exe.
const contentType = archive.endsWith(".zip")
  ? "application/zip"
  : "application/octet-stream";

wrangler(
  `${version}/${archive}`,
  archivePath,
  `--content-type ${contentType} --cache-control "public, max-age=31536000, immutable"`,
);
pass(`installer uploaded to ${version}/${archive}`);

const manifestTmp = join(NSIS_DIR, "latest.json");
writeFileSync(manifestTmp, JSON.stringify(manifest, null, 2));
wrangler(
  "latest.json",
  manifestTmp,
  `--content-type application/json --cache-control "no-cache"`,
);
pass("manifest uploaded");

// ── 5. Read it back the way a till would ─────────────────────────────────────
//
// Measured, not assumed. Everything above can succeed against a bucket that is
// not publicly readable, which is a state that looks like a clean release from
// this side and reaches no till at all.

step("Verify as a till would");

const check = await fetch(manifestUrl, { cache: "no-store" }).catch(() => null);
if (!check || !check.ok) {
  die(
    `${manifestUrl} is not publicly readable (${check ? check.status : "no response"}).`,
    `    The upload worked, but the bucket is not public, so no till can read it.\n` +
    `    In R2 → your bucket → Settings → Public Development URL → Enable.\n`,
  );
}
const served = await check.json();
if (served.version !== version) {
  die(`The manifest reads back as ${served.version}, expected ${version}.`);
}
pass(`${manifestUrl} serves ${served.version}`);

console.log(`\n${c.green}${c.bold}RELEASED${c.reset}  ${version}`);
console.log(
  `${c.dim}Tills pick this up on next launch, and are prompted between shifts —\n` +
  `never mid-cart. Nothing is installed without the owner PIN.${c.reset}\n`,
);
