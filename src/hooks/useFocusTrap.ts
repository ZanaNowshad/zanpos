import { type RefObject, useEffect, useRef } from "react";

const FOCUSABLE_SELECTORS = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  '[tabindex]:not([tabindex="-1"])',
].join(", ");

/**
 * Traps keyboard focus inside `containerRef` while the component is mounted.
 *
 * - Moves focus to the first focusable element on mount.
 * - Tab / Shift+Tab cycle within the container.
 * - Closes on Escape (calls `onClose` if provided).
 *
 * WCAG 2.1 SC 2.1.2 — No Keyboard Trap requires that focus CAN be moved away,
 * which we satisfy by releasing the trap when the modal unmounts.
 */
export function useFocusTrap<T extends HTMLElement>(
  containerRef: RefObject<T | null>,
  onClose?: () => void,
) {
  // Remember where focus was before the modal opened so we can restore it.
  const previouslyFocused = useRef<Element | null>(null);

  useEffect(() => {
    previouslyFocused.current = document.activeElement;

    // Move focus to the first focusable element inside the container.
    const container = containerRef.current;
    if (container) {
      const first = container.querySelector<HTMLElement>(FOCUSABLE_SELECTORS);
      first?.focus();
    }

    function handleKeyDown(e: KeyboardEvent) {
      if (!containerRef.current) return;

      if (e.key === "Escape") {
        e.preventDefault();
        onClose?.();
        return;
      }

      if (e.key !== "Tab") return;

      const focusable = Array.from(
        containerRef.current.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTORS),
      ).filter(el => !el.closest("[aria-hidden='true']"));

      if (focusable.length === 0) {
        e.preventDefault();
        return;
      }

      const first = focusable[0];
      const last = focusable[focusable.length - 1];

      if (e.shiftKey) {
        // Shift+Tab — wrap to last when leaving first
        if (document.activeElement === first) {
          e.preventDefault();
          last.focus();
        }
      } else {
        // Tab — wrap to first when leaving last
        if (document.activeElement === last) {
          e.preventDefault();
          first.focus();
        }
      }
    }

    document.addEventListener("keydown", handleKeyDown);

    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      // Restore focus to the element that was active before the modal opened.
      if (
        previouslyFocused.current instanceof HTMLElement &&
        document.body.contains(previouslyFocused.current)
      ) {
        previouslyFocused.current.focus();
      }
    };
  }, [containerRef, onClose]);
}
