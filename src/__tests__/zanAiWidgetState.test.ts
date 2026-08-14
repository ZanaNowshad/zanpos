import { describe, expect, it } from "vitest";
import { applyWidgetSuppression, clampWidgetRect } from "../zanai/widgetState";

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
