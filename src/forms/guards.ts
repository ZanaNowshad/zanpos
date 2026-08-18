import { useCallback, useRef, useState } from "react";
import type { ZodSchema } from "zod";

/**
 * Guards against invalid Tauri invoke() calls by validating payloads
 * with Zod schemas before they reach the Rust boundary.
 *
 * Usage:
 *   const payload = guardPayload(productSchema, formData);
 *   if (!payload) { setError("Invalid form data"); return; }
 *   await cmd.productCreate(payload);
 *
 * Owner: ZANPOS Maintainers. Review by: 2027-01-31.
 */
export function guardPayload<TSchema extends ZodSchema>(
  schema: TSchema,
  data: unknown,
): TSchema["_output"] | null {
  const result = schema.safeParse(data);
  if (result.success) return result.data;
  return null;
}

/**
 * Returns the first validation error message, or null if valid.
 */
export function validatePayload<TSchema extends ZodSchema>(
  schema: TSchema,
  data: unknown,
): string | null {
  const result = schema.safeParse(data);
  if (result.success) return null;
  return result.error.issues[0]?.message ?? "Validation failed";
}

/**
 * Prevents double-submit with a ref-based lock that survives re-renders.
 * Returns [isLocked, tryLock, unlock].
 */
export function useSubmitGuard(): [boolean, () => boolean, () => void] {
  // The ref wins the race: it updates synchronously, so two clicks in the same
  // tick cannot both acquire the lock. State exists separately because reading
  // `ref.current` during render produces a value that never triggers a
  // re-render — the returned `isLocked` was therefore stale by construction,
  // and only went unnoticed because the sole caller discards it.
  const lockedRef = useRef(false);
  const [isLocked, setIsLocked] = useState(false);

  const tryLock = useCallback(() => {
    if (lockedRef.current) return false;
    lockedRef.current = true;
    setIsLocked(true);
    return true;
  }, []);

  const unlock = useCallback(() => {
    lockedRef.current = false;
    setIsLocked(false);
  }, []);

  return [isLocked, tryLock, unlock];
}
