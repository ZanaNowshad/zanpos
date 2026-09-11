/**
 * Prepare a fresh checkout so the Rust crate can build.
 *
 * `tauri.conf.json` declares four bundle resources; three of them are absent
 * from a clean clone or a new git worktree:
 *
 *   sidecar/whatsapp-sidecar/node.exe          gitignored, ~92 MB
 *   sidecar/whatsapp-sidecar/node_modules/     not committed
 *   ../storefront/dist/                        a build output
 *
 * The Tauri build script resolves those resources, so a missing one fails the
 * build — but it reports only `resource path X doesn't exist`, one at a time,
 * after a full dependency compile. Three sequential dead ends, each costing a
 * cold build, with nothing naming the remedy. CI already works around this
 * (`.github/workflows/ci.yml:125-140`), and the comment there says plainly that
 * "no script in the repo fetched it". This is that script.
 *
 * Idempotent: every step checks before it acts, so running it on a warm
 * checkout is cheap and safe.
 *
 *   npm run bootstrap
 */
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const SIDECAR = join(ROOT, "src-tauri", "sidecar", "whatsapp-sidecar");
const STOREFRONT = join(ROOT, "storefront");

let stepNo = 0;
const done = [];
const skipped = [];

function step(label, isSatisfied, act) {
  stepNo += 1;
  if (isSatisfied()) {
    console.log(`  ${stepNo}. ${label} — already present`);
    skipped.push(label);
    return;
  }
  console.log(`  ${stepNo}. ${label} …`);
  act();
  if (!isSatisfied()) {
    throw new Error(
      `${label} ran but did not produce what it should have. ` +
        `Fix this before building; the Tauri build script will otherwise fail ` +
        `with a bare "resource path doesn't exist".`,
    );
  }
  done.push(label);
}

/**
 * Locate npm's JS entry point so it can be run through `node` directly.
 *
 * The obvious `execFileSync("npm.cmd", …)` fails on Windows with EINVAL: since
 * the CVE-2024-27980 mitigation Node will not spawn a `.cmd` shim without a
 * shell, and `shell: true` is itself deprecated for argument-bearing calls
 * (DEP0190). Running `npm-cli.js` under the current Node sidesteps both — no
 * shim, no shell, no quoting rules that differ per platform.
 *
 * `npm_execpath` is set whenever this runs under an npm script, which is the
 * normal path; the sibling lookup covers `node scripts/bootstrap.mjs` directly.
 */
function resolveNpmCli() {
  const fromEnv = process.env.npm_execpath;
  if (fromEnv && fromEnv.endsWith(".js") && existsSync(fromEnv)) return fromEnv;

  const sibling = join(dirname(process.execPath), "node_modules", "npm", "bin", "npm-cli.js");
  if (existsSync(sibling)) return sibling;

  return null;
}

const NPM_CLI = resolveNpmCli();

function npm(args, cwd) {
  if (!NPM_CLI) {
    throw new Error(
      "Could not locate npm-cli.js. Run this through `npm run bootstrap`, " +
        "or install Node with its bundled npm.",
    );
  }
  execFileSync(process.execPath, [NPM_CLI, ...args], { cwd, stdio: "inherit" });
}

/** `npm ci` when there is a lockfile to honour, `npm install` otherwise. */
function installDeps(cwd) {
  npm([existsSync(join(cwd, "package-lock.json")) ? "ci" : "install", "--no-audit", "--no-fund"], cwd);
}

function hasDir(p) {
  return existsSync(p) && statSync(p).isDirectory();
}

console.log("Bootstrapping ZANPOS build prerequisites\n");

step(
  "root dependencies",
  () => hasDir(join(ROOT, "node_modules")),
  () => installDeps(ROOT),
);

step(
  "storefront dependencies",
  () => hasDir(join(STOREFRONT, "node_modules")),
  () => installDeps(STOREFRONT),
);

step(
  "storefront/dist (bundled as an app resource)",
  () => hasDir(join(STOREFRONT, "dist")),
  () => npm(["run", "build"], STOREFRONT),
);

step(
  "WhatsApp sidecar dependencies",
  () => hasDir(join(SIDECAR, "node_modules")),
  () => installDeps(SIDECAR),
);

// The sidecar runs whatever Node is copied here, so this pins it to the Node
// running this script. package.json asks for >=20; check rather than assume,
// because a too-old runtime fails at sidecar start, far from this step.
step(
  "sidecar Node runtime (node.exe)",
  () => existsSync(join(SIDECAR, "node.exe")),
  () => {
    const major = Number(process.versions.node.split(".")[0]);
    if (major < 20) {
      throw new Error(
        `The sidecar requires Node >= 20 (package.json engines), but this is ` +
          `${process.versions.node}. Copying it would produce a runtime that ` +
          `fails when the sidecar starts. Switch Node and re-run.`,
      );
    }
    mkdirSync(SIDECAR, { recursive: true });
    copyFileSync(process.execPath, join(SIDECAR, "node.exe"));
  },
);

console.log(
  `\nReady. ${done.length} prepared, ${skipped.length} already present.` +
    `\n\n  cd src-tauri && cargo test --lib` +
    `\n\nNote: the Rust crate is Windows-only — sha2, hmac, csv, mysql, tiberius,` +
    `\ncalamine, zip and keyring sit under [target.'cfg(windows)'.dependencies],` +
    `\nso a Linux cargo build fails on unresolved crates before running a test.`,
);
