import { describe, expect, it } from "vitest";
import { systemControlPanes } from "../components/settings/SystemControlTab";

describe("Settings command center", () => {
  it("merges system-side controls into one command center", () => {
    const labels = systemControlPanes("manager").map(p => p.label);

    expect(labels).toEqual([
      "Hub & devices",
      "WhatsApp",
      "AI control",
      "Backup & updates",
    ]);
  });

  it("keeps maintenance owner-gated", () => {
    expect(systemControlPanes("manager").map(p => p.id)).not.toContain("maintenance");
    expect(systemControlPanes("owner").map(p => p.id)).toContain("maintenance");
  });
});
