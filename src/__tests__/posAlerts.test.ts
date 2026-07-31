import { describe, expect, it } from "vitest";
import { shouldChime } from "../hooks/usePosAlerts";

describe("new-order chime rule", () => {
  it("stays silent on the first poll, when there is no baseline yet", () => {
    // Otherwise every login would chime for orders that arrived hours ago.
    expect(shouldChime(null, 0)).toBe(false);
    expect(shouldChime(null, 7)).toBe(false);
  });

  it("sounds only when the queue grows", () => {
    expect(shouldChime(0, 1)).toBe(true);
    expect(shouldChime(2, 5)).toBe(true);
  });

  it("stays silent as the cashier works the queue down", () => {
    expect(shouldChime(5, 4)).toBe(false);
    expect(shouldChime(1, 0)).toBe(false);
  });

  it("stays silent when the count is unchanged, so polling is not audible", () => {
    expect(shouldChime(3, 3)).toBe(false);
    expect(shouldChime(0, 0)).toBe(false);
  });
});
