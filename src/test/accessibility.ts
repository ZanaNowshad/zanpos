/**
 * Accessibility test helper for ZANPOS.
 *
 * ESLint jsx-a11y rules provide automated enforcement at lint-time. Runtime axe
 * audits require a DOM (jsdom) test environment which is not yet configured
 * (current Vitest environment: node). When jsdom is added, uncomment the
 * axeAudit() helper and wrap critical-path component renders to get automated
 * violation reporting.
 *
 * Until then, every critical flow listed below must pass documented manual checks
 * against the keyboard + Arabic RTL checklist.
 */

// --- Manual a11y checklist (must be verified per release) --------------------

/** Critical user flows covered by the audit checklist */
export const CRITICAL_A11Y_FLOWS = [
  "login (PIN entry, error feedback, focus trap)",
  "shift open/close (status readback, cashier confirmation)",
  "payment finalization (amount readback, tender selection keyboard nav)",
  "manager PIN override (dialog name, focus return, Escape dismissal)",
  "refund flow (reason selection, amount confirmation, receipt anchoring)",
  "product grid (category filter tabs, product card focus order, scan feedback)",
  "navigation sidebar (collapsible regions, current-page indicator, skip link)",
  "language direction change (AR↔EN, RTL layout, text alignment, icon mirroring)",
] as const;

/** Keyboard interactions required for each flow */
export const KEYBOARD_CHECKS = [
  "Tab order follows visual reading order (LTR for EN, RTL for AR)",
  "Enter / Space activates buttons and links",
  "Escape dismisses modals/dialogs and returns focus to trigger",
  "Arrow keys navigate within composite widgets (menus, tabs, grids)",
  "Focus never lands on a non-interactive or hidden element",
  "Focus trap inside modals prevents background tabbing",
] as const;

/** Arabic RTL-specific checks */
export const ARABIC_RTL_CHECKS = [
  "html[dir='rtl'] is set when Arabic is active",
  "Visual layout mirrors correctly (sidebar, modals, tooltips)",
  "Directional icons flip (arrows, chevrons, back/forward)",
  "Text alignment is right-aligned for Arabic blocks",
  "Input fields accept and display Arabic text without corruption",
  "Numeric fields (money, quantities) remain LTR within RTL pages",
] as const;

// --- Deliberate-failure fixtures (lint-time) ---------------------------------

/**
 * @example Unlabeled button — should trigger jsx-a11y/control-has-associated-label
 */
// const BAD_BUTTON = <button onClick={() => {}} />; // ❌ ESLint error expected

/**
 * @example Properly labeled button — should pass lint
 */
// const GOOD_BUTTON = <button aria-label="Close dialog" onClick={() => {}} />; // ✅

// --- Runtime axe audit (available once jsdom test environment is active) ------

/**
 * Wraps axe-core runtime audit. Uncomment after adding jsdom to Vitest config.
 *
 * import { axe } from "vitest-axe";
 *
 * export async function axeAudit(container: HTMLElement): Promise<void> {
 *   const results = await axe(container);
 *   const violations = results.violations.filter(
 *     (v) => v.impact === "serious" || v.impact === "critical"
 *   );
 *   if (violations.length > 0) {
 *     throw new Error(
 *       `a11y violations: ${violations.map((v) => v.id + ": " + v.description).join("; ")}`
 *     );
 *   }
 * }
 */

// Prevent "unused export" noise when the file is imported in tests.
export const _a11yHelper = {} as const;
