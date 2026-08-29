/**
 * Minimal `.env` loader, shared by the ship gate and the release publisher.
 *
 * Exists because the updater signing key is password-protected, and that
 * password has to reach the `tauri build` subprocess without being typed into a
 * shell (where it lands in history) or passed on a command line (where it is
 * visible to anything reading the process table). `.env` is already gitignored,
 * so it is the one place a secret can sit.
 *
 * Values already present in the environment win, so CI — where these arrive as
 * repository secrets and no `.env` file exists — behaves identically without a
 * special case.
 *
 * No dependency on purpose: this runs before `npm install` has necessarily
 * brought anything else in, and a release tool that can fail on a missing
 * package is a release tool that fails at the worst moment.
 */

import { readFileSync, existsSync } from "node:fs";

export function loadDotEnv(root) {
  const path = `${root}/.env`;
  if (!existsSync(path)) return false;
  // Split on CRLF as well as LF. Windows editors write CRLF, and JavaScript's
  // `.` does not match `\r` — so `(.*)$` silently failed on every line that had
  // a value, while `NAME=\r` still matched because `\s*` after the `=` absorbed
  // the carriage return. The result was a file that looked loaded (one variable
  // set) with every actual value missing, which surfaced as an unsigned build
  // rather than as an error about the file.
  for (const line of readFileSync(path, "utf8").split(/\r?\n/)) {
    const match = /^\s*([A-Za-z0-9_]+)\s*=\s*(.*)$/.exec(line);
    if (!match) continue;
    const value = match[2].trim().replace(/^["']|["']$/g, "");
    if (process.env[match[1]] === undefined) process.env[match[1]] = value;
  }
  return true;
}
