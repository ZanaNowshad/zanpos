import { useEffect, useRef, type RefObject } from "react";

/**
 * Replacement for the autoFocus attribute.
 * Focuses the element on mount — safe for modals and dialogs.
 *
 * Usage:
 *   const ref = useAutoFocus<HTMLInputElement>();
 *   <input ref={ref} ... />
 */
export function useAutoFocus<T extends HTMLElement>(): RefObject<T | null> {
  const ref = useRef<T | null>(null);
  useEffect(() => {
    // Small delay lets modal transitions complete before focusing
    const id = requestAnimationFrame(() => ref.current?.focus());
    return () => cancelAnimationFrame(id);
  }, []);
  return ref;
}
