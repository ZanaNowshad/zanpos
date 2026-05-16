import { useCallback, useEffect, useLayoutEffect, useRef } from "react";

/**
 * Fires `onIdle` after `timeoutMs` of no user activity.
 * Activity events: mousedown, keydown, touchstart, scroll.
 */
export function useIdleTimer(timeoutMs: number, onIdle: () => void) {
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const onIdleRef = useRef(onIdle);
  // Sync the ref before effects fire so the latest callback is always used
  useLayoutEffect(() => { onIdleRef.current = onIdle; });

  const reset = useCallback(() => {
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => onIdleRef.current(), timeoutMs);
  }, [timeoutMs]);

  useEffect(() => {
    const events: string[] = [
      "mousedown", "keydown", "touchstart", "scroll", "pointermove",
    ];
    events.forEach(e => window.addEventListener(e, reset, { passive: true }));
    reset(); // start timer immediately on mount
    return () => {
      events.forEach(e => window.removeEventListener(e, reset));
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [reset]);
}
