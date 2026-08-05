/**
 * WebdriverIO Tauri E2E configuration for ZANPOS.
 *
 * Prerequisites:
 *   - Tauri app built: cargo build from src-tauri/
 *   - @wdio/tauri-service installed
 */

import { join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = fileURLToPath(new URL(".", import.meta.url));
const REPO_ROOT = join(__dirname, "..", "..");
const SRC_TAURI = join(REPO_ROOT, "src-tauri");

const DEBUG =
  process.env.TAURI_BUILD === "release"
    ? join(SRC_TAURI, "target", "release")
    : join(SRC_TAURI, "target", "debug");

const BINARY = join(DEBUG, "zanpos.exe");

export const config = {
  runner: "local",
  specs: ["./specs/**/*.ts"],
  maxInstances: 1,
  services: [
    [
      "@wdio/tauri-service",
      {
        appBinaryPath: BINARY,
        driverProvider: "embedded",
      },
    ],
  ],
  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": {
        application: BINARY,
        args: [],
      },
    },
  ],
  logLevel: "warn",
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: {
    ui: "bdd",
    timeout: 60000,
  },
  before: async () => {
    await browser.pause(5000);
  },
};
