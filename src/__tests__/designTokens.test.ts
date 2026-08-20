import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

/**
 * The token layer, enforced.
 *
 * Both of these had already drifted once. The radius scale existed twice with
 * different values (--radius-* at 10/14/20 and --ui-radius-* at 8/6) on top of
 * fourteen distinct hardcoded steps, so a chip, a card and a panel sitting side
 * by side had three different corners. Type had sunk to 0.6rem — 9px on this
 * 15px root — on tags that a standing operator cannot read at arm's length.
 *
 * Greps, not rendered output, because that is what catches the next hardcoded
 * value the moment it is written rather than after it ships. The rendered
 * counterpart lives in qa/token-audit.mjs, which needs a dev server.
 */

const SRC = join(process.cwd(), "src");

function cssFiles(dir: string): string[] {
  return readdirSync(dir).flatMap(entry => {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) return cssFiles(full);
    return full.endsWith(".css") ? [full] : [];
  });
}

const FILES = cssFiles(SRC).map(path => ({ path: path.slice(SRC.length + 1), text: readFileSync(path, "utf8") }));

/** Deliberate exceptions, each one a shape the scale has no step for. */
const RADIUS_EXCEPTIONS = new Set(["32px", "1px"]);

describe("design tokens", () => {
  it("has stylesheets to check", () => {
    expect(FILES.length).toBeGreaterThan(5);
  });

  it("keeps every corner on the five-step radius scale", () => {
    const offenders: string[] = [];
    for (const { path, text } of FILES) {
      for (const line of text.split("\n")) {
        const m = /border-radius:\s*(\d+px);/.exec(line);
        if (m && !RADIUS_EXCEPTIONS.has(m[1])) offenders.push(`${path}: ${line.trim()}`);
      }
    }
    expect(offenders, "use var(--radius-xs|sm|md|lg|xl|pill)").toEqual([]);
  });

  it("keeps every label at or above the 11px legibility floor", () => {
    // The root is 15px, not 16 — so anything under 0.7333rem renders below
    // the floor. That 15px base is exactly what made the original sweep miss
    // a batch of 0.72rem labels sitting at 10.8px.
    const offenders: string[] = [];
    for (const { path, text } of FILES) {
      for (const line of text.split("\n")) {
        const rem = /font-size:\s*(0?\.\d+)rem/.exec(line);
        if (rem && parseFloat(rem[1]) * 15 < 11) offenders.push(`${path}: ${line.trim()}`);
        const px = /font-size:\s*(\d+(?:\.\d+)?)px/.exec(line);
        if (px && parseFloat(px[1]) < 11) offenders.push(`${path}: ${line.trim()}`);
      }
    }
    expect(offenders, "use var(--type-office-micro) or larger").toEqual([]);
  });

  it("defines the scales once, with the aliases pointing at them", () => {
    const tokens = FILES.find(f => f.path.endsWith("tokens.css"));
    expect(tokens).toBeDefined();
    const text = tokens!.text;
    // --ui-radius-* must not be a second source of truth.
    expect(text).toMatch(/--ui-radius:\s*var\(--radius-md\)/);
    expect(text).toMatch(/--ui-radius-sm:\s*var\(--radius-sm\)/);
    expect(text).toMatch(/--type-office-micro:\s*11px/);
  });
});
