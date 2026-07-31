import { describe, expect, it } from "vitest";
import { formatSyncAge, syncAgeMinutes } from "../components/SyncChip";
import { whatsappHealth } from "../components/pos/WhatsAppPill";

describe("sync age", () => {
  it("reads as words an operator can act on, not raw minutes", () => {
    // "180m ago" and "3d ago" should not require arithmetic at a till.
    expect(formatSyncAge(0)).toBe("just now");
    expect(formatSyncAge(12)).toBe("12m ago");
    expect(formatSyncAge(90)).toBe("1h ago");
    expect(formatSyncAge(60 * 26)).toBe("1d ago");
  });

  it("says never rather than inventing an age", () => {
    expect(formatSyncAge(null)).toBe("never");
    expect(formatSyncAge(null, 4)).toBe("4d ago");
  });

  it("treats a missing or unparseable timestamp as unknown", () => {
    expect(syncAgeMinutes(null)).toBeNull();
    expect(syncAgeMinutes("not a date")).toBeNull();
  });

  it("never reports a negative age when a clock runs ahead", () => {
    const future = new Date(Date.now() + 60_000).toISOString();
    expect(syncAgeMinutes(future)).toBe(0);
  });
});

describe("WhatsApp pill health", () => {
  it("is amber while the first check is still running", () => {
    expect(whatsappHealth(null, false)).toBe("degraded");
  });

  it("is red only on a confirmed disconnection", () => {
    expect(whatsappHealth(false, false)).toBe("down");
  });

  it("goes amber, not red, when the news is stale", () => {
    // The watchdog restarts the sidecar routinely. A pill that flashed red
    // every time it did its job would be ignored within a week.
    expect(whatsappHealth(true, true)).toBe("degraded");
    expect(whatsappHealth(true, false)).toBe("ok");
  });
});
