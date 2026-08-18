import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from "react";
import type { SeverityLevel, StatusItem, PillLevel } from "./statusTypes";
import { severityToPillLevel, worstSeverity } from "./statusTypes";

export interface StatusSnapshot {
  items: StatusItem[];
  overall: SeverityLevel;
  /** When was the last successful sync (Unix ms). */
  lastSyncAt: number | null;
  /** Number of items queued for sync. */
  syncQueueDepth: number;
}

interface StatusContextValue extends StatusSnapshot {
  /** Add or update a status item. */
  upsert: (item: StatusItem) => void;
  /** Remove a status item by id. */
  dismiss: (id: string) => void;
  /** Dismiss all non-persistent items. */
  dismissAll: () => void;
  /** Set sync-related state. */
  setSyncState: (lastSyncAt: number | null, queueDepth: number) => void;
  /** Clear all status items (e.g. on logout). */
  reset: () => void;
}

const StatusContext = createContext<StatusContextValue>({
  items: [],
  overall: "ok",
  lastSyncAt: null,
  syncQueueDepth: 0,
  upsert: () => {},
  dismiss: () => {},
  dismissAll: () => {},
  setSyncState: () => {},
  reset: () => {},
});

export function useStatus() {
  return useContext(StatusContext);
}

/** Convert a StatusItem to a header-compatible pill format. */
export function statusToPill(item: StatusItem): { id: string; label: string; level: PillLevel; icon?: string } {
  return {
    id: item.id,
    label: item.label,
    level: severityToPillLevel(item.severity),
    icon: item.icon,
  };
}

interface Props {
  children: ReactNode;
}

export default function StatusProvider({ children }: Props) {
  const [items, setItems] = useState<StatusItem[]>([]);
  const [lastSyncAt, setLastSyncAt] = useState<number | null>(null);
  const [syncQueueDepth, setSyncQueueDepth] = useState(0);

  const upsert = useCallback((item: StatusItem) => {
    setItems(prev => {
      const idx = prev.findIndex(i => i.id === item.id);
      if (idx === -1) return [...prev, item];
      const next = [...prev];
      next[idx] = item;
      return next;
    });
  }, []);

  const dismiss = useCallback((id: string) => {
    setItems(prev => prev.filter(i => i.id !== id || i.persistent));
  }, []);

  const dismissAll = useCallback(() => {
    setItems(prev => prev.filter(i => i.persistent));
  }, []);

  const setSyncState = useCallback((lastSync: number | null, depth: number) => {
    setLastSyncAt(lastSync);
    setSyncQueueDepth(depth);
  }, []);

  const reset = useCallback(() => {
    setItems([]);
    setLastSyncAt(null);
    setSyncQueueDepth(0);
  }, []);

  const overall = useMemo(
    () => worstSeverity(items.map(i => i.severity)),
    [items],
  );

  const value = useMemo<StatusContextValue>(() => ({
    items,
    overall,
    lastSyncAt,
    syncQueueDepth,
    upsert,
    dismiss,
    dismissAll,
    setSyncState,
    reset,
  }), [items, overall, lastSyncAt, syncQueueDepth, upsert, dismiss, dismissAll, setSyncState, reset]);

  return (
    <StatusContext.Provider value={value}>
      {children}
    </StatusContext.Provider>
  );
}
