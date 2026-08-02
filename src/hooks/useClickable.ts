import { useCallback, type KeyboardEvent } from "react";

/**
 * Returns accessibility props for non-native interactive elements.
 * Use with div/span elements that have onClick handlers.
 *
 * Example:
 *   const clickable = useClickable(onClose);
 *   <div {...clickable} onClick={onClose}>Close</div>
 */
export function useClickable(
  handler: (() => void) | undefined,
): Record<string, unknown> {
  const onKeyDown = useCallback(
    (e: KeyboardEvent<HTMLElement>) => {
      if (handler && (e.key === "Enter" || e.key === " ")) {
        e.preventDefault();
        handler();
      }
    },
    [handler],
  );

  return {
    role: "button",
    tabIndex: 0,
    onKeyDown,
  };
}
