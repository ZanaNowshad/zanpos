import type { ZodSchema } from "zod";

/**
 * Guards against invalid Tauri invoke() calls by validating payloads
 * with Zod schemas before they reach the Rust boundary.
 *
 * Usage:
 *   const payload = guardPayload(customerSchema, formData);
 *   if (!payload) { setError("Invalid form data"); return; }
 *   await cmd.customerCreate(payload);
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
 * Prevents double-submit by tracking a submission lock.
 * Returns a [lock, unlock] pair.
 */
export function useSubmitGuard(): [boolean, () => void, () => void] {
  let locked = false;
  const lock = () => {
    if (locked) return false;
    locked = true;
    return true;
  };
  const unlock = () => {
    locked = false;
  };
  return [locked, lock, unlock];
}
