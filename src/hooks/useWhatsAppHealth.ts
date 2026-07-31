import { useEffect, useState } from "react";
import { whatsappStatus } from "../tauri/commands";

export function useWhatsAppHealth(enabled: boolean, userId: string) {
  const [connected, setConnected] = useState<boolean | null>(null);
  const [stale, setStale] = useState(false);

  useEffect(() => {
    if (!enabled) return;
    let cancelled = false;
    const check = async () => {
      try {
        const status = await whatsappStatus(userId);
        if (!cancelled) {
          setConnected(status.connected);
          setStale(false);
        }
      } catch {
        // A failed check is not proof of disconnection while the sidecar restarts.
        if (!cancelled) setStale(true);
      }
    };
    void check();
    const id = setInterval(check, 20_000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [enabled, userId]);

  return { connected, stale } as const;
}
