import { useCallback, useEffect, useLayoutEffect, useRef } from "react";

const WARN_BEFORE_MS = 60_000; // show warning 60 s before lock

/**
 * Fires `onIdle` after `timeoutMs` of no user activity.
 * Optionally fires `onWarning` 60 s before `onIdle` (when timeoutMs > 60 s).
 * Fires `onResume` when activity resets the timer while the warning is visible.
 * Activity events: mousedown, keydown, touchstart, scroll, pointermove.
 */
export function useIdleTimer(
  timeoutMs: number,
  onIdle: () => void,
  onWarning?: () => void,
  onResume?: () => void,
) {
  const idleRef    = useRef<ReturnType<typeof setTimeout> | null>(null);
  const warnRef    = useRef<ReturnType<typeof setTimeout> | null>(null);
  const warned     = useRef(false);

  // Always-fresh callbacks
  const onIdleRef    = useRef(onIdle);
  const onWarnRef    = useRef(onWarning);
  const onResumeRef  = useRef(onResume);
  useLayoutEffect(() => {
    onIdleRef.current   = onIdle;
    onWarnRef.current   = onWarning;
    onResumeRef.current = onResume;
  });

  const reset = useCallback(() => {
    // Disabled when timeout is zero or negative (used in pages that don't need auto-lock)
    if (timeoutMs <= 0) return;

    // If warning was visible and user acted — fire resume callback
    if (warned.current) {
      warned.current = false;
      onResumeRef.current?.();
    }

    if (idleRef.current)  clearTimeout(idleRef.current);
    if (warnRef.current)  clearTimeout(warnRef.current);

    // Schedule idle timer
    idleRef.current = setTimeout(() => onIdleRef.current(), timeoutMs);

    // Schedule warning (only if timeout is long enough to warn)
    const warnDelay = timeoutMs - WARN_BEFORE_MS;
    if (warnDelay > 0 && onWarnRef.current) {
      warnRef.current = setTimeout(() => {
        warned.current = true;
        onWarnRef.current?.();
      }, warnDelay);
    }
  }, [timeoutMs]);

  useEffect(() => {
    // No-op if idle timeout is disabled
    if (timeoutMs <= 0) return;

    const events: string[] = [
      "mousedown", "keydown", "touchstart", "scroll", "pointermove",
    ];
    events.forEach(e => window.addEventListener(e, reset, { passive: true }));
    reset(); // start timer immediately on mount
    return () => {
      events.forEach(e => window.removeEventListener(e, reset));
      if (idleRef.current) clearTimeout(idleRef.current);
      if (warnRef.current) clearTimeout(warnRef.current);
    };
  }, [reset, timeoutMs]);
}
