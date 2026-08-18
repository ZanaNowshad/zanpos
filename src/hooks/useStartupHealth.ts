import { useEffect, useRef, useState } from "react";
import type { AppConfig, StartupComponentStatus } from "../types";
import { startupHealthCheck, startupRestartSidecar } from "../tauri/commands";

/**
 * Poll the backend's startup health until the app is safe to show.
 *
 * Every wait here is bounded, because a till that never finishes booting is
 * worse than one that boots degraded: the loop gives up at 30s, and a sidecar
 * still reporting "starting" at 12s gets exactly one restart attempt before the
 * poll continues. `startupCheckDone` goes true on all-green, or on an error
 * that has been on screen long enough (14s) for the operator to have read it.
 */
export function useStartupHealth(configLoading: boolean, appConfig: AppConfig | null) {
  const [startupStatuses, setStartupStatuses] = useState<StartupComponentStatus[]>([]);
  const [startupTicker, setStartupTicker] = useState(0);
  const startupForceAttempted = useRef(false);


  useEffect(() => {
    if (configLoading || !appConfig || !appConfig.setup_complete) return;
    let cancelled = false;
    setStartupTicker(0);

    const poll = async (): Promise<StartupComponentStatus[]> => {
      if (cancelled) return [];
      try {
        const statuses = await startupHealthCheck();
        if (cancelled) return [];
        setStartupStatuses(statuses);
        return statuses;
      } catch {
        return [];
      }
    };

    const loop = async () => {
      let statuses = await poll();
      let elapsed = 0;
      while (!cancelled) {
        await new Promise((r) => setTimeout(r, 2000));
        if (cancelled) return;
        statuses = await poll();
        elapsed += 2;
        setStartupTicker(elapsed);

        const allOk = statuses.every((s) => s.status === "ok" || s.status !== "starting");
        if (allOk) return;

        const sidecar = statuses.find((s) => s.component === "whatsapp_sidecar");
        if (sidecar?.status === "starting" && elapsed >= 12 && !startupForceAttempted.current) {
          startupForceAttempted.current = true;
          await startupRestartSidecar().catch(() => {});
          continue;
        }
        if (elapsed >= 30) return;
      }
    };

    loop();
    return () => { cancelled = true; };
  }, [configLoading, appConfig]);

  const allComponentsGreen =
    startupStatuses.length > 0 && startupStatuses.every((s) => s.status === "ok");
  const anyComponentError =
    startupStatuses.length > 0 && startupStatuses.some((s) => s.status === "error");

  return {
    startupStatuses,
    startupTicker,
    allComponentsGreen,
    anyComponentError,
    startupCheckDone: allComponentsGreen || (anyComponentError && startupTicker >= 14),
    /* The operator's override. A component reporting an error does not always
       mean the till cannot sell — WhatsApp being down is not a reason to block
       a cash sale — so after the error has been readable for 14s they can
       proceed on their own judgement. */
    proceedAnyway: () => setStartupStatuses(current =>
      current.map(status => ({ ...status, status: "ok" }))),
  };
}
