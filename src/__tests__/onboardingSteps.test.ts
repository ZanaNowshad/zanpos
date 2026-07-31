import { describe, it, expect } from "vitest";
import {
  ONBOARDING_STEPS,
  ONBOARDING_STEP_COUNT,
  nextIncompleteStepIndex,
  isOnboardingComplete,
  stepKeyAt,
} from "../components/onboarding/onboardingSteps";

describe("onboardingSteps", () => {
  it("defines the six spec-order steps", () => {
    expect(ONBOARDING_STEP_COUNT).toBe(6);
    expect(ONBOARDING_STEPS.map(s => s.key)).toEqual([
      "identity", "owner_pin", "whatsapp", "products", "printer", "golive",
    ]);
  });

  it("resumes at the first unresolved step", () => {
    expect(nextIncompleteStepIndex(new Set())).toBe(0);
    expect(nextIncompleteStepIndex(new Set(["identity"]))).toBe(1);
    expect(nextIncompleteStepIndex(new Set(["identity", "owner_pin"]))).toBe(2);
  });

  it("ignores unrelated step names when resolving the resume index", () => {
    expect(nextIncompleteStepIndex(new Set(["identity", "owner_pin", "bogus"]))).toBe(2);
  });

  it("clamps to the last step once everything is resolved", () => {
    const allDone = new Set(ONBOARDING_STEPS.map(s => s.key));
    expect(nextIncompleteStepIndex(allDone)).toBe(5);
    expect(isOnboardingComplete(allDone)).toBe(true);
  });

  it("is not complete until every step is resolved", () => {
    expect(isOnboardingComplete(new Set(["identity", "owner_pin", "whatsapp"]))).toBe(false);
  });

  it("clamps out-of-range indexes when resolving a step key", () => {
    expect(stepKeyAt(-5)).toBe("identity");
    expect(stepKeyAt(0)).toBe("identity");
    expect(stepKeyAt(999)).toBe("golive");
  });
});
