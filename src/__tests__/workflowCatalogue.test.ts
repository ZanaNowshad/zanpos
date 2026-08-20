import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { WORKFLOW_CATALOGUE, WORKFLOW_GROUPS } from "../officeai/workflowCatalogue";

/**
 * The launcher's workflow ids must match the Rust registry exactly.
 *
 * This is a contract across a language boundary with no compiler between the
 * two sides. A typo does not throw: the prompt still reaches the model, the
 * model calls `load_workflow` with a name the registry does not have, and the
 * guide silently fails to load — so the answer looks plausible and is not the
 * procedure the operator asked for. Reading the Rust source is ugly but it is
 * the only thing that actually holds the two lists together.
 */

const REGISTRY_SOURCE = readFileSync(
  join(process.cwd(), "src-tauri", "src", "ai", "workflows.rs"),
  "utf8",
);

/** Names in WORKFLOW_REGISTRY, the list `load_workflow` accepts. */
function rustWorkflowNames(): string[] {
  const start = REGISTRY_SOURCE.indexOf("WORKFLOW_REGISTRY");
  const end = REGISTRY_SOURCE.indexOf("];", start);
  expect(start, "WORKFLOW_REGISTRY not found — did workflows.rs move?").toBeGreaterThan(-1);
  const block = REGISTRY_SOURCE.slice(start, end);
  return [...block.matchAll(/\("([a-z_]+)",/g)].map(m => m[1]);
}

describe("workflow launcher matches the Rust registry", () => {
  it("reads a registry with entries in it", () => {
    expect(rustWorkflowNames().length).toBeGreaterThan(10);
  });

  it("offers every workflow the backend knows", () => {
    const rust = new Set(rustWorkflowNames());
    const ui = new Set(WORKFLOW_CATALOGUE.map(w => w.id));
    const missingFromUi = [...rust].filter(n => !ui.has(n));
    expect(missingFromUi, "written in workflows.rs but unreachable from the UI").toEqual([]);
  });

  it("never offers a workflow the backend does not have", () => {
    const rust = new Set(rustWorkflowNames());
    const unknown = WORKFLOW_CATALOGUE.filter(w => !rust.has(w.id)).map(w => w.id);
    expect(unknown, "offered in the launcher but absent from the registry").toEqual([]);
  });

  it("names the workflow inside the prompt so the model loads the right guide", () => {
    for (const w of WORKFLOW_CATALOGUE) {
      expect(w.prompt, `${w.id} prompt must name its workflow`).toContain(w.id);
    }
  });

  it("gives every entry a label, a blurb and a real group", () => {
    const groups = new Set(WORKFLOW_GROUPS.map(g => g.id));
    for (const w of WORKFLOW_CATALOGUE) {
      expect(w.label.trim().length, `${w.id} label`).toBeGreaterThan(0);
      expect(w.blurb.trim().length, `${w.id} blurb`).toBeGreaterThan(0);
      expect(groups.has(w.group), `${w.id} group "${w.group}" is not in WORKFLOW_GROUPS`).toBe(true);
    }
  });

  it("has no group that renders empty", () => {
    for (const group of WORKFLOW_GROUPS) {
      const count = WORKFLOW_CATALOGUE.filter(w => w.group === group.id).length;
      expect(count, `group "${group.id}" has no workflows`).toBeGreaterThan(0);
    }
  });

  it("lists each workflow once", () => {
    const ids = WORKFLOW_CATALOGUE.map(w => w.id);
    expect(ids.length).toBe(new Set(ids).size);
  });
});
