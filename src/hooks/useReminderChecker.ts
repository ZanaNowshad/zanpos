import { useEffect, useRef } from "react";
import { loadNotes, type StickyNote } from "../utils/stickyNotes";

/**
 * Checks localStorage every 30 seconds for notes whose reminder time has
 * arrived.  Calls `onDue` with the array of notes that are due.
 *
 * The hook is intentionally side-effect-free beyond reading localStorage —
 * callers are responsible for marking notes as fired / snoozed via `patchNote`.
 */
export function useReminderChecker(onDue: (notes: StickyNote[]) => void): void {
  const callbackRef = useRef(onDue);

  useEffect(() => {
    callbackRef.current = onDue;
  });

  useEffect(() => {
    const check = () => {
      const now = Date.now();
      const due = loadNotes().filter(n => {
        if (!n.reminderAt) return false;
        if (n.reminderFired) return false;
        if (n.reminderSnoozedUntil && now < n.reminderSnoozedUntil) return false;
        return now >= n.reminderAt;
      });
      if (due.length > 0) callbackRef.current(due);
    };

    check(); // fire immediately on mount (catches reminders set while app was closed)
    const id = setInterval(check, 30_000);
    return () => clearInterval(id);
  }, []); // runs once — stable via ref
}
