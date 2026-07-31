import { describe, expect, it, vi } from "vitest";
import type { ReprintQueueEntry } from "../types";
import { clearPrintedReminder, retryPendingReprint } from "../utils/reprintQueue";

const entries: ReprintQueueEntry[] = [
  {
    id: "queue-1",
    receipt_number: "R-1001",
    store_name: "Main",
    lines: ["Receipt: #R-1001"],
    failed_at: "2026-07-27T18:00:00Z",
    error: "port busy",
    business_date: "2026-07-27",
  },
  {
    id: "queue-2",
    receipt_number: "R-1002",
    store_name: "Main",
    lines: ["Receipt: #R-1002"],
    failed_at: "2026-07-27T18:01:00Z",
    error: "offline",
    business_date: "2026-07-27",
  },
];

describe("EOD reprint queue", () => {
  it("removes only the receipt after printing and marking it printed", async () => {
    const print = vi.fn().mockResolvedValue("Printed");
    const markPrinted = vi.fn().mockResolvedValue(undefined);

    const result = await retryPendingReprint({
      entries,
      entry: entries[0],
      print,
      markPrinted,
    });

    expect(print).toHaveBeenCalledWith("Main", ["Receipt: #R-1001"]);
    expect(markPrinted).toHaveBeenCalledWith("queue-1");
    expect(result).toEqual({
      entries: [entries[1]],
      error: null,
      printedButUncleared: false,
    });
  });

  it("keeps the receipt visible when printing fails", async () => {
    const markPrinted = vi.fn();
    const result = await retryPendingReprint({
      entries,
      entry: entries[0],
      print: vi.fn().mockRejectedValue(new Error("port busy")),
      markPrinted,
    });

    expect(markPrinted).not.toHaveBeenCalled();
    expect(result.entries).toBe(entries);
    expect(result.error).toContain("port busy");
  });

  it("keeps the receipt visible when thermal printing is disabled", async () => {
    const markPrinted = vi.fn();
    const result = await retryPendingReprint({
      entries,
      entry: entries[0],
      print: vi.fn().mockResolvedValue("Thermal printing disabled"),
      markPrinted,
    });

    expect(markPrinted).not.toHaveBeenCalled();
    expect(result.entries).toBe(entries);
    expect(result.error).toContain("remains pending");
  });

  it("does not print or clear a receipt with unreadable stored lines", async () => {
    const print = vi.fn();
    const markPrinted = vi.fn();
    const result = await retryPendingReprint({
      entries,
      entry: { ...entries[0], lines: [] },
      print,
      markPrinted,
    });

    expect(print).not.toHaveBeenCalled();
    expect(markPrinted).not.toHaveBeenCalled();
    expect(result.entries).toBe(entries);
    expect(result.error).toContain("unreadable");
  });

  it("switches to mark-only recovery when printing succeeded but clearing failed", async () => {
    const print = vi.fn().mockResolvedValue("Printed");
    const result = await retryPendingReprint({
      entries,
      entry: entries[0],
      print,
      markPrinted: vi.fn().mockRejectedValue(new Error("database busy")),
    });

    expect(print).toHaveBeenCalledTimes(1);
    expect(result.entries).toBe(entries);
    expect(result.printedButUncleared).toBe(true);

    const recovered = await clearPrintedReminder(
      result.entries,
      entries[0].id,
      vi.fn().mockResolvedValue(undefined),
    );
    expect(print).toHaveBeenCalledTimes(1);
    expect(recovered.entries).toEqual([entries[1]]);
    expect(recovered.printedButUncleared).toBe(false);
  });
});
