import type { CriticalUpdateInfo } from "../tauri/updates";

export type UpdateLifecycle = "other" | "active_shift" | "post_eod" | "pre_shift";
export type UpdatePromptDecision =
  | "hidden"
  | "normal-dismissible"
  | "critical-deferrable"
  | "critical-required";

export interface UpdatePromptInput {
  availableVersion: string | null;
  criticalInfo: CriticalUpdateInfo | null;
  lifecycle: UpdateLifecycle;
  hasOpenCart: boolean;
  hasBlockingModal: boolean;
  dismissedNormalVersion: string | null;
  deferredCriticalVersion: string | null;
}

/**
 * One timing and safety policy for every update prompt. The critical flag is
 * unsigned, so it can only raise urgency when it exactly matches a candidate
 * independently returned by the signed updater path.
 */
export function decideUpdatePrompt({
  availableVersion,
  criticalInfo,
  lifecycle,
  hasOpenCart,
  hasBlockingModal,
  dismissedNormalVersion,
  deferredCriticalVersion,
}: UpdatePromptInput): UpdatePromptDecision {
  if (!availableVersion || hasOpenCart || hasBlockingModal) return "hidden";
  if (lifecycle === "active_shift" || lifecycle === "other") return "hidden";

  const isCritical =
    criticalInfo?.critical === true && criticalInfo.version === availableVersion;

  if (lifecycle === "pre_shift") {
    return isCritical ? "critical-required" : "hidden";
  }

  if (isCritical) {
    return deferredCriticalVersion === availableVersion
      ? "hidden"
      : "critical-deferrable";
  }
  return dismissedNormalVersion === availableVersion
    ? "hidden"
    : "normal-dismissible";
}
