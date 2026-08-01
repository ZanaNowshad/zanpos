/**
 * WebdriverIO Tauri E2E configuration for ZANPOS.
 *
 * Prerequisites:
 *   - Tauri app built in debug mode: `cargo build` from src-tauri/
 *   - No existing ZANPOS instance running on the test machine
 *
 * The WebDriver protocol connects to Tauri's built-in automation support.
 * On Windows, the app binary path must resolve to the debug or release build.
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

export const config: WebdriverIO.Config = {
  runner: "local",
  specs: ["./specs/**/*.ts"],
  maxInstances: 1,
  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": {
        application: BINARY,
        args: [],
        webviewOptions: {},
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
    // Allow time for the Tauri app to spawn and the WebView to load.
    await browser.pause(5000);
  },
};
