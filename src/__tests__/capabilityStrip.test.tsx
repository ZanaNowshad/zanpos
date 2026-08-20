import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import CapabilityStrip from "../command/CapabilityStrip";
import type { Capability } from "../command/capabilities";
import type { SeverityLevel } from "../command/statusTypes";
import type { OfficeTab } from "../officeai/officeAiTypes";

/**
 * Today folds the healthy capabilities into one line so the ones needing a
 * person are what the manager sees first.
 *
 * The risk that makes this worth a test is one-directional: fold something
 * that needed attention and it disappears from the landing page entirely,
 * with a green tick over it. Every severity is checked explicitly rather than
 * a representative sample, because the bug would be a single mis-sorted level.
 */

const cap = (id: string, severity: SeverityLevel): Capability => ({
  id: id as Capability["id"],
  labelKey: id,
  severity,
  state: `${id} state`,
  action: { labelKey: `open-${id}`, tab: "today" as OfficeTab },
});

const render = (caps: Capability[], variant: "compact" | "full" = "compact") =>
  renderToStaticMarkup(
    <CapabilityStrip
      capabilities={caps}
      labelFor={key => `Label:${key}`}
      actionLabelFor={key => `Action:${key}`}
      onAction={vi.fn()}
      variant={variant}
    />,
  );

const HEALTHY: SeverityLevel[] = ["ok", "info"];
const ATTENTION: SeverityLevel[] = ["setup-required", "attention", "degraded", "blocked", "critical"];

describe("capability strip", () => {
  it("folds only the severities that need nobody", () => {
    for (const severity of HEALTHY) {
      const html = render([cap("thing", severity)]);
      expect(html, `${severity} should fold`).toContain("zp-cap-folded");
      expect(html, `${severity} should not show its state`).not.toContain("thing state");
    }
  });

  it("never folds a capability that needs a person", () => {
    for (const severity of ATTENTION) {
      const html = render([cap("thing", severity)]);
      expect(html, `${severity} must stay expanded`).not.toContain("zp-cap-folded");
      expect(html, `${severity} must show its state`).toContain("thing state");
      expect(html, `${severity} must offer its action`).toContain("Action:open-thing");
    }
  });

  it("shows the healthy names on the folded line so nothing is hidden outright", () => {
    const html = render([cap("till", "ok"), cap("sync", "info"), cap("whatsapp", "degraded")]);
    expect(html).toContain("Label:till");
    expect(html).toContain("Label:sync");
    // The one needing attention is a row of its own, not part of the fold.
    expect(html).toContain("whatsapp state");
  });

  it("leaves the health page listing everything in full", () => {
    // System → Health exists to be read end to end; folding there would hide
    // the detail that page is for.
    const html = render([cap("till", "ok"), cap("sync", "info")], "full");
    expect(html).not.toContain("zp-cap-folded");
    expect(html).toContain("till state");
    expect(html).toContain("sync state");
  });

  it("renders nothing extra when every capability needs attention", () => {
    const html = render([cap("a", "critical"), cap("b", "blocked")]);
    expect(html).not.toContain("zp-cap-folded");
  });
});
