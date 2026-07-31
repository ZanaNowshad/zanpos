/**
 * Pure step definitions for the first-run onboarding wizard. No React, no
 * Tauri — kept side-effect free so it can be unit tested directly (see
 * src/__tests__/onboardingSteps.test.ts) and imported by both the orchestrator
 * and App.tsx's resume check without pulling in any component code.
 */

export type OnboardingStepKey =
  | "identity"
  | "owner_pin"
  | "whatsapp"
  | "products"
  | "printer"
  | "golive";

export interface OnboardingStepDef {
  key: OnboardingStepKey;
  label: string;
}

/** Spec order — steps 1..6. Progress dots render exactly these six labels. */
export const ONBOARDING_STEPS: OnboardingStepDef[] = [
  { key: "identity",  label: "Store" },
  { key: "owner_pin", label: "Owner PIN" },
  { key: "whatsapp",  label: "WhatsApp" },
  { key: "products",  label: "Products" },
  { key: "printer",   label: "Printer" },
  { key: "golive",    label: "Go Live" },
];

export const ONBOARDING_STEP_COUNT = ONBOARDING_STEPS.length;

/** Index (0-based) of the first step not yet resolved. Clamped to the last
 * step once everything is done, so callers can always safely render it. */
export function nextIncompleteStepIndex(done: ReadonlySet<string>): number {
  const idx = ONBOARDING_STEPS.findIndex(s => !done.has(s.key));
  return idx === -1 ? ONBOARDING_STEPS.length - 1 : idx;
}

export function isOnboardingComplete(done: ReadonlySet<string>): boolean {
  return ONBOARDING_STEPS.every(s => done.has(s.key));
}

export function stepKeyAt(index: number): OnboardingStepKey {
  const clamped = Math.max(0, Math.min(index, ONBOARDING_STEPS.length - 1));
  return ONBOARDING_STEPS[clamped].key;
}
