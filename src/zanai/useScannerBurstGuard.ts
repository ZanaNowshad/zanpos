import { useCallback, useRef } from "react";
import type { KeyboardEvent } from "react";
import { classifyScannerBurst, type TimedKey } from "./scannerBurst";

interface Options {
  focusBarcode: () => void;
}

/**
 * Keeps a barcode scan inside the ZanAI composer instead of diverting it.
 *
 * A scan and a typed barcode used to behave differently: the burst was pulled
 * out of the draft and pushed straight into the cart, while the same digits
 * typed by hand stayed in the message. Scanning is just a faster way to enter a
 * number the cashier is already writing about ("do we have more of 628001"), so
 * the two now agree — the characters land in the composer either way.
 *
 * The one thing still worth intercepting is the scanner's terminating Enter.
 * The characters arrive as ordinary keystrokes and the browser inserts them
 * with no help from us, but that trailing Enter would submit a half-written
 * message the moment the trigger was pulled. Detection therefore stays; only
 * its consequence changed. A human's Enter still sends, because a burst is
 * recognised by inter-key timing no hand can reach.
 */
export function useScannerBurstGuard({ focusBarcode }: Options) {
  const eventsRef = useRef<TimedKey[]>([]);

  return useCallback(
    (event: KeyboardEvent<HTMLTextAreaElement>): boolean => {
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
        // Not prevented: the character types into the composer as normal.
        return false;
      }
      if (event.key !== "Enter") return false;

      eventsRef.current.push({ key: "Enter", at: event.timeStamp });
      const result = classifyScannerBurst(eventsRef.current);
      eventsRef.current = [];
      if (result.kind !== "barcode") return false;

      // Swallow only the scanner's own Enter. The barcode is already in the
      // composer; sending here would fire off whatever the cashier had
      // half-written around it.
      event.preventDefault();
      event.stopPropagation();
      return true;
    },
    [focusBarcode],
  );
}
