import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { checkForUpdates } from "../tauri/commands";
import { checkCriticalUpdate, type CriticalUpdateInfo } from "../tauri/updates";
import {
  decideUpdatePrompt,
  type UpdateLifecycle,
  type UpdatePromptDecision,
} from "../utils/criticalUpdateGate";

const DEFERRED_CRITICAL_KEY = "zanpos_deferred_critical_update";

export interface UpdatePromptLifecycle {
  decision: UpdatePromptDecision;
  version: string | null;
  completeEod: () => void;
  dismissPostEod: () => void;
  prepareNewShift: () => Promise<boolean>;
  allowCriticalShift: () => void;
  activateShift: () => void;
}

export interface UpdateClassification {
  availableVersion: string | null;
  criticalInfo: CriticalUpdateInfo | null;
}

interface UpdateStorage {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
  removeItem: (key: string) => void;
}

export function readDeferredCritical(storage: UpdateStorage = localStorage): string | null {
  try {
    return storage.getItem(DEFERRED_CRITICAL_KEY);
  } catch {
    return null;
  }
}

export function persistDeferredCritical(
  version: string | null,
  storage: UpdateStorage = localStorage,
): void {
  try {
    if (version) storage.setItem(DEFERRED_CRITICAL_KEY, version);
    else storage.removeItem(DEFERRED_CRITICAL_KEY);
  } catch {
    // Advisory persistence is best-effort and must never affect a shift.
  }
}

export function resolveUpdateClassification(
  classification: Promise<UpdateClassification>,
  timeout: Promise<null>,
): Promise<UpdateClassification | null> {
  return Promise.race([classification, timeout]);
}

export function useUpdatePromptLifecycle(): UpdatePromptLifecycle {
  const [availableVersion, setAvailableVersion] = useState<string | null>(null);
  const [criticalInfo, setCriticalInfo] = useState<CriticalUpdateInfo | null>(null);
  const [lifecycle, setLifecycle] = useState<UpdateLifecycle>("other");
  const [dismissedNormalVersion, setDismissedNormalVersion] = useState<string | null>(null);
  const [deferredCriticalVersion, setDeferredCriticalVersion] = useState<string | null>(
    readDeferredCritical,
  );
  const classificationRef = useRef<Promise<UpdateClassification> | null>(null);

  useEffect(() => {
    const classification = Promise.all([
      checkForUpdates().catch(() => null),
      checkCriticalUpdate().catch(() => null),
    ]).then(([version, info]) => ({ availableVersion: version, criticalInfo: info }));
    classificationRef.current = classification;
    classification.then(({ availableVersion: version, criticalInfo: info }) => {
      setAvailableVersion(version);
      setCriticalInfo(info);
    });
  }, []);

  const decision = useMemo(() => decideUpdatePrompt({
    availableVersion,
    criticalInfo,
    lifecycle,
    hasOpenCart: false,
    hasBlockingModal: false,
    dismissedNormalVersion,
    deferredCriticalVersion,
  }), [
    availableVersion,
    criticalInfo,
    lifecycle,
    dismissedNormalVersion,
    deferredCriticalVersion,
  ]);

  const completeEod = useCallback(() => {
    setDismissedNormalVersion(null);
    setDeferredCriticalVersion(null);
    persistDeferredCritical(null);
    setLifecycle("post_eod");
  }, []);

  const dismissPostEod = useCallback(() => {
    if (!availableVersion) return;
    const matchingCritical =
      criticalInfo?.critical === true && criticalInfo.version === availableVersion;
    if (matchingCritical) {
      setDeferredCriticalVersion(availableVersion);
      persistDeferredCritical(availableVersion);
    } else {
      setDismissedNormalVersion(availableVersion);
    }
  }, [availableVersion, criticalInfo]);

  const prepareNewShift = useCallback(async () => {
    let version = availableVersion;
    let advisory = criticalInfo;
    if (classificationRef.current) {
      const timeout = new Promise<null>((resolve) => setTimeout(() => resolve(null), 750));
      const settled = await resolveUpdateClassification(classificationRef.current, timeout);
      if (settled) {
        version = settled.availableVersion;
        advisory = settled.criticalInfo;
      }
    }
    const preShiftDecision = decideUpdatePrompt({
      availableVersion: version,
      criticalInfo: advisory,
      lifecycle: "pre_shift",
      hasOpenCart: false,
      hasBlockingModal: false,
      dismissedNormalVersion,
      deferredCriticalVersion,
    });
    if (preShiftDecision === "critical-required") {
      if (version) {
        setDeferredCriticalVersion(version);
        persistDeferredCritical(version);
      }
      setLifecycle("pre_shift");
      return false;
    }
    setLifecycle("active_shift");
    return true;
  }, [
    availableVersion,
    criticalInfo,
    dismissedNormalVersion,
    deferredCriticalVersion,
  ]);

  const allowCriticalShift = useCallback(() => {
    if (availableVersion) {
      setDeferredCriticalVersion(availableVersion);
      persistDeferredCritical(availableVersion);
    }
    setLifecycle("active_shift");
  }, [availableVersion]);
  const activateShift = useCallback(() => setLifecycle("active_shift"), []);

  return {
    decision,
    version: availableVersion,
    completeEod,
    dismissPostEod,
    prepareNewShift,
    allowCriticalShift,
    activateShift,
  };
}
