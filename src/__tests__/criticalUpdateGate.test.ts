import { describe, expect, it } from "vitest";
import { decideUpdatePrompt, type UpdatePromptInput } from "../utils/criticalUpdateGate";
import {
  persistDeferredCritical,
  readDeferredCritical,
  resolveUpdateClassification,
} from "../hooks/useUpdatePromptLifecycle";

const base: UpdatePromptInput = {
  availableVersion: "2.1.0",
  criticalInfo: null,
  lifecycle: "post_eod",
  hasOpenCart: false,
  hasBlockingModal: false,
  dismissedNormalVersion: null,
  deferredCriticalVersion: null,
};

const critical = {
  ...base,
  criticalInfo: { version: "2.1.0", critical: true },
} satisfies UpdatePromptInput;

describe("decideUpdatePrompt", () => {
  it("suppresses every update while a shift is active, even with an empty cart", () => {
    expect(decideUpdatePrompt({ ...base, lifecycle: "active_shift" })).toBe("hidden");
    expect(decideUpdatePrompt({ ...critical, lifecycle: "active_shift" })).toBe("hidden");
  });

  it("offers a normal update only after EOD and allows dismissal", () => {
    expect(decideUpdatePrompt(base)).toBe("normal-dismissible");
    expect(decideUpdatePrompt({
      ...base,
      dismissedNormalVersion: "2.1.0",
    })).toBe("hidden");
  });

  it("offers a critical update after EOD but allows deferral until the next shift", () => {
    expect(decideUpdatePrompt(critical)).toBe("critical-deferrable");
    expect(decideUpdatePrompt({
      ...critical,
      deferredCriticalVersion: "2.1.0",
    })).toBe("hidden");
  });

  it("forces a deferred critical prompt before the next shift", () => {
    expect(decideUpdatePrompt({
      ...critical,
      lifecycle: "pre_shift",
      deferredCriticalVersion: "2.1.0",
    })).toBe("critical-required");
  });

  it("never forces a normal update before a shift", () => {
    expect(decideUpdatePrompt({ ...base, lifecycle: "pre_shift" })).toBe("hidden");
  });

  it("suppresses the prompt over an open cart", () => {
    expect(decideUpdatePrompt({ ...critical, hasOpenCart: true })).toBe("hidden");
    expect(decideUpdatePrompt({
      ...critical,
      lifecycle: "pre_shift",
      hasOpenCart: true,
    })).toBe("hidden");
  });

  it("suppresses the prompt while another modal owns the operator flow", () => {
    expect(decideUpdatePrompt({ ...critical, hasBlockingModal: true })).toBe("hidden");
    expect(decideUpdatePrompt({
      ...critical,
      lifecycle: "pre_shift",
      hasBlockingModal: true,
    })).toBe("hidden");
  });

  it("scopes normal dismissal to one version", () => {
    expect(decideUpdatePrompt({
      ...base,
      availableVersion: "2.2.0",
      dismissedNormalVersion: "2.1.0",
    })).toBe("normal-dismissible");
  });

  it("expires critical deferral at the pre-shift boundary", () => {
    const deferred = { ...critical, deferredCriticalVersion: "2.1.0" };
    expect(decideUpdatePrompt(deferred)).toBe("hidden");
    expect(decideUpdatePrompt({ ...deferred, lifecycle: "pre_shift" }))
      .toBe("critical-required");
  });

  it("does not force from unmatched or advisory-only critical metadata", () => {
    expect(decideUpdatePrompt({
      ...critical,
      lifecycle: "pre_shift",
      criticalInfo: { version: "2.2.0", critical: true },
    })).toBe("hidden");
    expect(decideUpdatePrompt({
      ...critical,
      lifecycle: "pre_shift",
      availableVersion: null,
    })).toBe("hidden");
  });

  it("uses a joined classification when both checks settle before the bound", async () => {
    const classification = Promise.resolve({
      availableVersion: "2.1.0",
      criticalInfo: { version: "2.1.0", critical: true },
    });
    await expect(resolveUpdateClassification(classification, new Promise(() => {})))
      .resolves.toEqual(await classification);
  });

  it("fails open when update classification exceeds the bound", async () => {
    const neverSettles = new Promise<never>(() => {});
    await expect(resolveUpdateClassification(neverSettles, Promise.resolve(null)))
      .resolves.toBeNull();
  });

  it("swallows unavailable storage without blocking the timing policy", () => {
    const unavailable = {
      getItem: () => { throw new Error("disabled"); },
      setItem: () => { throw new Error("disabled"); },
      removeItem: () => { throw new Error("disabled"); },
    };
    expect(readDeferredCritical(unavailable)).toBeNull();
    expect(() => persistDeferredCritical("2.1.0", unavailable)).not.toThrow();
    expect(() => persistDeferredCritical(null, unavailable)).not.toThrow();
  });
});
