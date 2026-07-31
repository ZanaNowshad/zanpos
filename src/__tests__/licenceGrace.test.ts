import { describe, expect, it } from "vitest";
import { graceMessage, graceTone } from "../components/LicenseGraceBanner";

describe("licence grace countdown", () => {
  it("stays quiet early in the grace period", () => {
    // Shouting on day 1 of 30 is noise the operator stops seeing by day 25.
    expect(graceTone(30)).toBe("quiet");
    expect(graceTone(8)).toBe("quiet");
  });

  it("escalates in the last week, then the last three days", () => {
    expect(graceTone(7)).toBe("warn");
    expect(graceTone(4)).toBe("warn");
    expect(graceTone(3)).toBe("urgent");
    expect(graceTone(1)).toBe("urgent");
  });

  it("stays urgent once expired rather than wrapping to quiet", () => {
    expect(graceTone(0)).toBe("urgent");
    expect(graceTone(-5)).toBe("urgent");
  });

  it("reads naturally at each boundary", () => {
    expect(graceMessage(1)).toBe("Your licence renews tomorrow.");
    expect(graceMessage(12)).toBe("Your licence renews in 12 days.");
    expect(graceMessage(0)).toBe("Your licence has expired.");
  });
});
