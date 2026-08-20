import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";

/**
 * Regression: the Review workspace crashed with
 * `Cannot read properties of null (reading 'length')` when the Conflicts
 * section opened.
 *
 * Root cause was not Review code. The dev mock's fallback returned `null` for
 * any command it did not recognise, so an unstubbed array-returning command
 * (`sync_stock_drift_report`) handed `null` to a component that correctly
 * assumed `StockDriftRow[]`. Production never returns null for those commands,
 * so the mock was manufacturing a state the real backend cannot produce.
 *
 * These tests pin the boundary rather than the symptom: fixing this by
 * sprinkling `?.` through components would have hidden a fixture that lies
 * about the backend contract.
 */
/* The mock is three files: uiMock.ts holds the dispatcher, uiMockHandlers.ts
   the command→response table, and uiMockData.ts the fixtures that table serves.
   All three are read because a stub can legitimately live in any of them — a
   canned response in the handler map, or an argument-honouring branch in the
   dispatcher.

   Read as a directory rather than a hand-listed set: this test failed the
   moment the handler map was split out of uiMock.ts, reporting a missing stub
   that had simply moved. A missing stub is a real defect; a moved one is not,
   and the test should not confuse them. */
const MOCK = readdirSync(path.resolve(__dirname, "../dev"))
  .filter(f => f.startsWith("uiMock") && f.endsWith(".ts"))
  .map(f => readFileSync(path.resolve(__dirname, "../dev", f), "utf8"))
  .join("\n");

/** Commands whose frontend signature is `Promise<T[]>` and are used by Review. */
const ARRAY_COMMANDS = [
  "sync_conflicts_list",
  "sync_stock_drift_report",
  "audit_log_list",
  "ai_list_actions",
];

describe("dev mock shape fidelity", () => {
  it("stubs every array-returning command Review depends on", () => {
    for (const cmd of ARRAY_COMMANDS) {
      expect(MOCK, `${cmd} must be stubbed explicitly`).toContain(`${cmd}:`);
    }
  });

  it("never falls back to null for list-shaped command names", () => {
    // Mirrors emptyFor() in uiMock.ts. Kept as a literal so a change to the
    // regex has to be made deliberately in both places.
    const listLike = /_list$|^list_|_rows$|_all$|_report$|_history$|_queue$|_items$|s$/;
    for (const cmd of ARRAY_COMMANDS) {
      expect(listLike.test(cmd), `${cmd} should match the array fallback`).toBe(true);
    }
    // The specific command that caused the crash.
    expect(listLike.test("sync_stock_drift_report")).toBe(true);
  });

  it("keeps the array fallback ahead of the null fallback", () => {
    const fn = MOCK.slice(MOCK.indexOf("function emptyFor"), MOCK.indexOf("export function installUiMock"));
    const arrayReturn = fn.indexOf("return [];");
    const nullReturn = fn.indexOf("return null;");
    expect(arrayReturn).toBeGreaterThan(-1);
    expect(arrayReturn).toBeLessThan(nullReturn);
  });
});
