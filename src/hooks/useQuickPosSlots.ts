import { useCallback, useEffect, useState } from "react";
import { quickPosLoad, type QuickPosSlot } from "../tauri/commands";

/**
 * The till's quick-add row.
 *
 * Refreshed on a slow interval as well as on mount: the row is chosen in the
 * back office and rides config sync, so a manager adding an item on one
 * terminal should reach the others without anyone restarting a till. Ninety
 * seconds is well inside a shift and far outside anything that could interfere
 * with scanning.
 *
 * A failure leaves the previous row in place rather than blanking it — the
 * shortcuts are a convenience, and losing them mid-queue because one poll
 * failed would be worse than showing a slightly stale row.
 */
export function useQuickPosSlots(actorUserId: string) {
  const [slots, setSlots] = useState<QuickPosSlot[]>([]);

  const refresh = useCallback(async () => {
    try {
      setSlots(await quickPosLoad(actorUserId));
    } catch { /* keep whatever is on screen */ }
  }, [actorUserId]);

  useEffect(() => {
    void refresh();
    const id = setInterval(() => { void refresh(); }, 90_000);
    return () => clearInterval(id);
  }, [refresh]);

  return { slots, refresh } as const;
}
