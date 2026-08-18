import { useCallback, useRef } from "react";
import type { KeyboardEvent } from "react";
import { classifyScannerBurst, removeBurstSuffix, type TimedKey } from "./scannerBurst";

interface Options {
  draft: string;
  setDraft: (value: string) => void;
  onBarcode: (barcode: string, quantity?: number) => void;
  focusBarcode: () => void;
}

export function useScannerBurstGuard({ draft, setDraft, onBarcode, focusBarcode }: Options) {
  const eventsRef = useRef<TimedKey[]>([]);

  return useCallback((event: KeyboardEvent<HTMLTextAreaElement>): boolean => {
    if (event.key === "F2" || event.key === "F3") {
      event.preventDefault();
      focusBarcode();
      eventsRef.current = [];
      return true;
    }
    if (event.shiftKey || event.ctrlKey || event.altKey || event.metaKey) {
      eventsRef.current = [];
      return false;
    }
    if (event.key.length === 1) {
      const previous = eventsRef.current.at(-1);
      if (previous && event.timeStamp - previous.at > 120) eventsRef.current = [];
      eventsRef.current.push({ key: event.key, at: event.timeStamp });
      return false;
    }
    if (event.key !== "Enter") return false;

    eventsRef.current.push({ key: "Enter", at: event.timeStamp });
    const result = classifyScannerBurst(eventsRef.current);
    eventsRef.current = [];
    if (result.kind !== "barcode") return false;

    event.preventDefault();
    event.stopPropagation();
    setDraft(removeBurstSuffix(draft, result.value));
    const quantityMatch = /^(\d{1,3})[*x](.+)$/.exec(result.value);
    if (quantityMatch) onBarcode(quantityMatch[2], Number.parseInt(quantityMatch[1], 10));
    else onBarcode(result.value);
    return true;
  }, [draft, focusBarcode, onBarcode, setDraft]);
}
