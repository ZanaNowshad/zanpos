import { describe, expect, it } from "vitest";
import {
  SETTINGS_GROUPS, canOpenGroup, groupsForRole, searchGroups,
} from "../command/pages/settings/settingsConfig";

/**
 * Replaces the previous `systemControlPanes` tests. That helper described the
 * superseded arrangement where Integrations and System each stacked several
 * feature panels into one scroll; it no longer exists.
 *
 * The invariant asserted here is strictly stronger: role filtering is declared
 * once, and it is the same source the rail renders from AND the deep-link guard
 * consults — so hidden navigation and real authorisation cannot drift apart.
 */
const label = (k: string) => k;

describe("Settings information architecture", () => {
  it("declares one group per real configuration area", () => {
    expect(SETTINGS_GROUPS.map(g => g.id)).toEqual([
      "store", "sales", "team", "hardware",
      "integrations", "ai", "data", "security", "advanced",
    ]);
  });

  it("gives every group a unique id", () => {
    const ids = SETTINGS_GROUPS.map(g => g.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("keeps advanced maintenance owner-only", () => {
    expect(groupsForRole("owner").map(g => g.id)).toContain("advanced");
    expect(groupsForRole("manager").map(g => g.id)).not.toContain("advanced");
    expect(groupsForRole("cashier").map(g => g.id)).not.toContain("advanced");
  });

  it("keeps manager-only groups away from a cashier", () => {
    const cashier = groupsForRole("cashier").map(g => g.id);
    for (const id of ["team", "integrations", "ai", "data", "security"]) {
      expect(cashier, `${id} must not be visible to a cashier`).not.toContain(id);
    }
    // A cashier can still reach ordinary store configuration.
    expect(cashier).toContain("store");
    expect(cashier).toContain("hardware");
  });

  it("denies a deep link the rail would have hidden", () => {
    // Hiding navigation is not authorisation; the guard must agree with it.
    expect(canOpenGroup("advanced", "manager")).toBe(false);
    expect(canOpenGroup("advanced", "owner")).toBe(true);
    expect(canOpenGroup("ai", "cashier")).toBe(false);
    expect(canOpenGroup("store", "cashier")).toBe(true);
  });

  it("guard and rail never disagree", () => {
    for (const role of ["owner", "manager", "cashier", "accountant"]) {
      const visible = new Set(groupsForRole(role).map(g => g.id));
      for (const g of SETTINGS_GROUPS) {
        expect(canOpenGroup(g.id, role)).toBe(visible.has(g.id));
      }
    }
  });

  it("finds groups by the words a shopkeeper would type", () => {
    const find = (q: string) => searchGroups(q, "owner", label).map(g => g.id);
    expect(find("printer")).toContain("hardware");
    expect(find("vat")).toContain("store");
    expect(find("whatsapp")).toContain("integrations");
    expect(find("backup")).toContain("data");
    expect(find("api key")).toContain("ai");
    expect(find("permissions")).toContain("team");
    expect(find("practice")).toContain("sales");
  });

  it("never surfaces a group the role may not open, even via search", () => {
    expect(searchGroups("maintenance", "manager", label).map(g => g.id)).not.toContain("advanced");
    expect(searchGroups("", "cashier", label).map(g => g.id)).not.toContain("ai");
  });

  it("returns everything visible for an empty query", () => {
    expect(searchGroups("", "owner", label)).toHaveLength(groupsForRole("owner").length);
  });
});
