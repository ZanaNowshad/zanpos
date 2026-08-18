import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync, existsSync } from "node:fs";
import path from "node:path";

/**
 * The dev-only visual-QA mock must never reach a production bundle.
 *
 * This regression exists because it already leaked once: the guard correctly
 * returned false in production, but `installUiMock()` was called
 * unconditionally from main.tsx, so Rollup could not tree-shake the module and
 * its sample retail data (supplier names, product names, a fake owner session)
 * shipped inside dist/. Grepping for "uimock" missed it because identifiers
 * minify — only the string literals survived.
 *
 * The fix was to load the mock through a dynamic import behind a data-free
 * guard module. These tests pin both halves of that arrangement.
 */

const SRC = path.resolve(__dirname, "..");
const DIST_ASSETS = path.resolve(__dirname, "../../dist/assets");

/** Literals that exist ONLY in the mock. Any of them in dist/ means a leak. */
const MOCK_ONLY_STRINGS = [
  "Almarai Fresh Milk",
  "Al Jazira Trading",
  "Barbican Malt",
  "Bahrain Fresh Foods",
  "__zpMockSession",
  "IPC mocked",
  "usr_renihal",
];

describe("dev UI mock isolation", () => {
  it("is only reachable behind an import.meta.env.DEV guard", () => {
    const flag = readFileSync(path.join(SRC, "dev/uiMockFlag.ts"), "utf8");
    expect(flag).toContain("import.meta.env.DEV");
    expect(flag).toContain("uimock");
    // The guard module must hold no sample data, or importing it ships the data.
    for (const s of MOCK_ONLY_STRINGS) {
      if (s === "__zpMockSession") continue; // the accessor legitimately names it
      expect(flag).not.toContain(s);
    }
  });

  it("is loaded by dynamic import, never statically", () => {
    const main = readFileSync(path.join(SRC, "main.tsx"), "utf8");
    expect(main).toMatch(/import\(["']\.\/dev\/uiMock["']\)/);
    // A static import would defeat tree-shaking and ship the payload.
    expect(main).not.toMatch(/^import .*from ["']\.\/dev\/uiMock["']/m);
  });

  it("no production module imports the mock data module statically", () => {
    const offenders: string[] = [];
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = path.join(dir, entry.name);
        if (entry.isDirectory()) {
          if (entry.name === "dev" || entry.name === "__tests__") continue;
          // Dependency and tooling trees are not production modules, and they
          // are large enough to time this test out rather than fail it — a
          // stray npm cache under src/ put 14k files in its path.
          if (entry.name.startsWith(".") || entry.name === "node_modules") continue;
          walk(p);
          continue;
        }
        if (!/\.tsx?$/.test(entry.name)) continue;
        const src = readFileSync(p, "utf8");
        if (/^import .*from ["'].*dev\/uiMock["']/m.test(src)) offenders.push(p);
      }
    };
    walk(SRC);
    expect(offenders).toEqual([]);
  });

  it("built bundle contains no mock-only strings", () => {
    if (!existsSync(DIST_ASSETS)) {
      // Nothing built yet — the CI build step covers this case.
      return;
    }
    const bundles = readdirSync(DIST_ASSETS).filter(f => f.endsWith(".js"));
    expect(bundles.length).toBeGreaterThan(0);
    const found: string[] = [];
    for (const file of bundles) {
      const content = readFileSync(path.join(DIST_ASSETS, file), "utf8");
      for (const s of MOCK_ONLY_STRINGS) {
        if (content.includes(s)) found.push(`${file}: ${s}`);
      }
    }
    expect(found).toEqual([]);
  });
});
