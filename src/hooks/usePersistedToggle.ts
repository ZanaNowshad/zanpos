import { useCallback, useState } from "react";

/**
 * A boolean preference that survives restarts, stored in localStorage.
 *
 * Matches how `useTheme` and `useLanguage` persist UI preferences here, so a
 * till that is closed at the end of a shift reopens the way the cashier left
 * it. Reads treat anything other than "1" as false, so a corrupted or absent
 * value falls back to `defaultValue` rather than throwing on a POS at open.
 */
export function usePersistedToggle(storageKey: string, defaultValue = false) {
  const [value, setValue] = useState<boolean>(() => {
    const saved = localStorage.getItem(storageKey);
    return saved === null ? defaultValue : saved === "1";
  });

  const toggle = useCallback(() => {
    setValue(previous => {
      const next = !previous;
      localStorage.setItem(storageKey, next ? "1" : "0");
      return next;
    });
  }, [storageKey]);

  return [value, toggle] as const;
}
