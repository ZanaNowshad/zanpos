import type { CustomerRow } from "../../../types";

/**
 * Customer + loyalty contract, traced from source.
 *
 * Model (migrations/0001_initial.sql, customers table):
 *   customer_id, branch_id, name, phone, email, loyalty_points INTEGER, notes,
 *   created_at.
 *
 * Commands (src-tauri/src/commands/customer_commands.rs):
 *   customer_list(actor, search)        rbac::require_any_role   server-side search
 *   customer_get(actor, id)             rbac::require_any_role
 *   customer_create(input)              rbac::require_any_role
 *   customer_update(input)              rbac::require_any_role
 *   customer_add_loyalty(actor, id, n)  rbac::manager_or_owner   → new balance
 *
 * Deliberately absent, and therefore never rendered:
 *   • No loyalty ledger. `loyalty_points` is a single integer; there is no
 *     history table, so "recent loyalty activity" cannot be shown honestly.
 *   • No tiers, rewards, cashback or expiry.
 *   • No delete/deactivate command, so the directory has no destructive action.
 *   • No per-customer sales query. `report_sales_list` filters by date and
 *     branch only, so spend, visit count and last visit are NOT derivable.
 */

/** Loyalty is one integer. There is no tier, and no second balance. */
export type LoyaltyState = "none" | "active";

export function loyaltyState(c: Pick<CustomerRow, "loyalty_points">): LoyaltyState {
  return (c.loyalty_points ?? 0) > 0 ? "active" : "none";
}

export type LoyaltyDirection = "add" | "redeem";

export type LoyaltyError =
  | "not-a-number"
  | "zero"
  | "negative-input"
  | "insufficient";

/**
 * Mirrors `customer_add_loyalty` exactly:
 *   • `points == 0`                    → Validation "Points delta must be non-zero"
 *   • `points < 0 && before + points < 0` → Validation "Insufficient loyalty points"
 *
 * The user enters a positive magnitude and picks a direction; the sign is
 * applied here so a redemption cannot accidentally be sent as an award.
 */
export function validateLoyaltyInput(
  raw: string,
  direction: LoyaltyDirection,
  currentBalance: number,
): LoyaltyError | null {
  const trimmed = raw.trim();
  if (trimmed === "") return "zero";
  const n = Number(trimmed);
  if (!Number.isFinite(n) || !Number.isInteger(n)) return "not-a-number";
  if (n < 0) return "negative-input";
  if (n === 0) return "zero";
  if (direction === "redeem" && currentBalance - n < 0) return "insufficient";
  return null;
}

/** The signed delta to send, or null when the input is not valid. */
export function loyaltyDelta(
  raw: string,
  direction: LoyaltyDirection,
  currentBalance: number,
): number | null {
  if (validateLoyaltyInput(raw, direction, currentBalance)) return null;
  const n = Number(raw.trim());
  return direction === "redeem" ? -n : n;
}

/** Balance after a valid adjustment — used for the confirmation copy. */
export function projectedBalance(
  raw: string,
  direction: LoyaltyDirection,
  currentBalance: number,
): number | null {
  const delta = loyaltyDelta(raw, direction, currentBalance);
  return delta === null ? null : currentBalance + delta;
}

// ─── Directory ────────────────────────────────────────────────────────────────

export type DirectoryState = "first-use" | "no-results" | "degraded" | "ready";

/**
 * Which empty state a directory should render. `hasQuery` distinguishes "this
 * store has no customers yet" from "this search matched nothing" — the same
 * distinction the catalogue makes.
 */
export function directoryState(
  rows: CustomerRow[],
  hasQuery: boolean,
  error: string | null,
): DirectoryState {
  if (error) return "degraded";
  if (rows.length > 0) return "ready";
  return hasQuery ? "no-results" : "first-use";
}

/** Required-field check mirroring `customer_create`: only name is mandatory. */
export function validateCustomerName(name: string): boolean {
  return name.trim().length > 0;
}

export type CustomerField = "name" | "phone" | "email";

export type CustomerFieldError =
  | "name-required"
  | "name-too-long"
  | "phone-too-long"
  | "phone-charset"
  | "email-invalid";

/** Backend limit, from `customer_create`. The retired zod schema said 200 and
 *  would have rejected names the store is entitled to save. */
const NAME_MAX = 255;
const PHONE_MAX = 30;

/** `c.is_ascii_digit() || " +-()".contains(c)` — the backend's exact set. */
const PHONE_ALLOWED = /^[0-9 +\-()]*$/;

/**
 * Per-field validation mirroring `customer_create` / `customer_update`, plus one
 * rule the backend does not have.
 *
 * Email is never validated server-side: any string is stored. An address typed
 * wrongly is therefore accepted in silence and only discovered when someone
 * tries to use it, so the format check is done here and nowhere else claims it
 * was enforced.
 */
export function validateCustomerField(
  field: CustomerField,
  raw: string,
): CustomerFieldError | null {
  const value = raw.trim();
  switch (field) {
    case "name":
      if (value === "") return "name-required";
      return value.length > NAME_MAX ? "name-too-long" : null;
    case "phone":
      if (value === "") return null;
      if (value.length > PHONE_MAX) return "phone-too-long";
      return PHONE_ALLOWED.test(value) ? null : "phone-charset";
    case "email":
      if (value === "") return null;
      // Deliberately permissive: reject what is clearly not an address, not
      // what some stricter grammar dislikes.
      return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(value) ? null : "email-invalid";
  }
}

/** First blocking error across the whole form, or null when it can be sent. */
export function firstCustomerError(form: {
  name: string; phone: string; email: string;
}): { field: CustomerField; error: CustomerFieldError } | null {
  for (const field of ["name", "phone", "email"] as const) {
    const error = validateCustomerField(field, form[field]);
    if (error) return { field, error };
  }
  return null;
}

// ─── Loyalty programme ────────────────────────────────────────────────────────
//
// Programme totals are NOT computed here. The directory is paginated, so any
// client-side sum would silently describe only the rows currently loaded.
// `customer_loyalty_summary` and `customer_top_balances` aggregate and rank in
// SQL across the actor's whole branch; see LoyaltyPage.
