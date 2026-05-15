import { useState, useEffect } from "react";
import type { SyncStatus } from "../types";
import { syncStatus } from "../tauri/commands";

export function useSyncStatus(intervalMs = 10_000) {
  const [status, setStatus] = useState<SyncStatus | null>(null);

  useEffect(() => {
    let mounted = true;
    const poll = async () => {
      try {
        const s = await syncStatus();
        if (mounted) setStatus(s);
      } catch {
        // non-critical: keep last known status
      }
    };
    poll();
    const id = setInterval(poll, intervalMs);
    return () => { mounted = false; clearInterval(id); };
  }, [intervalMs]);

  return status;
}
