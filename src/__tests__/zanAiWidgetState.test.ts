import { describe, expect, it } from "vitest";
import { applyWidgetSuppression, clampLauncherPosition, clampWidgetRect } from "../zanai/widgetState";

describe("POS ZanAI window geometry", () => {
  it("clamps size and position inside the viewport", () => {
    expect(
      clampWidgetRect(
        { x: 1_100, y: 900, width: 100, height: 200 },
        { width: 1_200, height: 800 },
      ),
    ).toEqual({ x: 840, y: 380, width: 360, height: 420 });

    expect(
      clampWidgetRect(
        { x: -50, y: -20, width: 2_000, height: 2_000 },
        { width: 1_200, height: 800 },
      ),
    ).toEqual({ x: 0, y: 0, width: 840, height: 560 });
  });

  it("recovers invalid persisted numbers to a deterministic default", () => {
    expect(
      clampWidgetRect(
        { x: Number.NaN, y: 20, width: Number.POSITIVE_INFINITY, height: 500 },
        { width: 1_200, height: 800 },
      ),
    ).toEqual({ x: 756, y: 216, width: 420, height: 560 });
  });

  it("keeps the draggable launcher fully inside the viewport", () => {
    expect(clampLauncherPosition({ x: -20, y: 900 }, { width: 1_200, height: 800 }))
      .toEqual({ x: 0, y: 752 });
    expect(clampLauncherPosition({ x: 1_500, y: -10 }, { width: 1_200, height: 800 }))
      .toEqual({ x: 1_084, y: 58 });
  });

  it("never parks the launcher on the POS top bar", () => {
    // z-index 620 puts the launcher over the clock, sync chip and Quran toggle.
    // The old default was y: 8, which covered the Quran toggle out of the box.
    const viewport = { width: 1_280, height: 800 };
    expect(clampLauncherPosition({ x: Number.NaN, y: Number.NaN }, viewport).y)
      .toBeGreaterThanOrEqual(58);
    expect(clampLauncherPosition({ x: 400, y: 12 }, viewport).y).toBe(58);
  });

  it("still fits a window too short for the top-bar clearance", () => {
    // A 90px-tall window cannot honour both the clearance and the launcher
    // height; staying inside the window wins.
    expect(clampLauncherPosition({ x: 10, y: 0 }, { width: 400, height: 90 }))
      .toEqual({ x: 10, y: 42 });
  });
});

describe("critical-flow suppression", () => {
  it("hides without changing the cashier's voluntary open preference", () => {
    expect(applyWidgetSuppression({ preferredOpen: true, visible: true }, true)).toEqual({
      preferredOpen: true,
      visible: false,
    });
    expect(applyWidgetSuppression({ preferredOpen: true, visible: false }, false)).toEqual({
      preferredOpen: true,
      visible: true,
    });
  });
});
