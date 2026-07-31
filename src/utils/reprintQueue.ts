import type { ReprintQueueEntry } from "../types";

export interface ReprintRetryResult {
  entries: ReprintQueueEntry[];
  error: string | null;
  printedButUncleared: boolean;
}

interface RetryPendingReprintInput {
  entries: ReprintQueueEntry[];
  entry: ReprintQueueEntry;
  print: (storeName: string, lines: string[]) => Promise<string>;
  markPrinted: (id: string) => Promise<void>;
}

function errorText(error: unknown): string {
  return typeof error === "string"
    ? error
    : error instanceof Error
      ? error.message
      : "Unknown printer error";
}

export async function retryPendingReprint({
  entries,
  entry,
  print,
  markPrinted,
}: RetryPendingReprintInput): Promise<ReprintRetryResult> {
  if (entry.lines.length === 0) {
    return {
      entries,
      error: "Stored receipt data is unreadable. The reminder was kept for manager review.",
      printedButUncleared: false,
    };
  }

  try {
    const outcome = await print(entry.store_name, entry.lines);
    if (outcome !== "Printed") {
      return {
        entries,
        error: `${outcome || "Printer unavailable"}. The receipt remains pending.`,
        printedButUncleared: false,
      };
    }
  } catch (error: unknown) {
    return {
      entries,
      error: `Print failed: ${errorText(error)}. The receipt remains pending.`,
      printedButUncleared: false,
    };
  }

  try {
    await markPrinted(entry.id);
  } catch (error: unknown) {
    return {
      entries,
      error: `Receipt printed, but its reminder could not be cleared: ${errorText(error)}. Do not retry it now.`,
      printedButUncleared: true,
    };
  }

  return {
    entries: entries.filter(({ id }) => id !== entry.id),
    error: null,
    printedButUncleared: false,
  };
}

export async function clearPrintedReminder(
  entries: ReprintQueueEntry[],
  entryId: string,
  markPrinted: (id: string) => Promise<void>,
): Promise<ReprintRetryResult> {
  try {
    await markPrinted(entryId);
    return {
      entries: entries.filter(({ id }) => id !== entryId),
      error: null,
      printedButUncleared: false,
    };
  } catch (error: unknown) {
    return {
      entries,
      error: `Could not clear the printed receipt reminder: ${errorText(error)}. No receipt was printed again.`,
      printedButUncleared: true,
    };
  }
}
