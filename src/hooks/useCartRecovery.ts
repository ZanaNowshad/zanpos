import { useCallback, useEffect, useRef, useState } from "react";
import type { Cart } from "../types";

const STORAGE_KEY = "zanpos_cart_recovery";

/** What was in the till when the lights went out. */
export interface RecoverableCart {
  cart: Cart;
  savedAt: string;
  lineCount: number;
}

/**
 * Decides whether a stored snapshot is worth interrupting the operator for.
 *
 * Exported so tests exercise this exact function rather than a copy of it —
 * a duplicated guard in a test proves the copy works, not the code.
 * Returns null for anything empty, fully voided, or unparseable: offering to
 * restore something broken is worse than offering nothing.
 */
export function parseSnapshot(raw: string | null): RecoverableCart | null {
  if (!raw) return null;
  try {
    const saved = JSON.parse(raw) as RecoverableCart;
    return saved?.cart?.lines?.some(line => !line.voided) ? saved : null;
  } catch {
    return null;
  }
}

/**
 * Recovers the in-progress cart after a power cut or a kill.
 *
 * Held carts are a different thing: a cashier holds a cart deliberately, and
 * prompting about those on every launch would train people to dismiss the
 * prompt without reading it. This covers the case nobody chose — the shop
 * lost power mid-sale and the cart only ever existed in memory.
 *
 * localStorage is the right store here precisely because it is not the
 * database: it survives a process kill and a power cut, and it never touches
 * the sale transaction. A snapshot is not a sale and must never be counted as
 * one, so nothing here writes to SQLite.
 *
 * The snapshot is cleared on a completed sale and on an explicit clear, so the
 * only thing that can survive to the next launch is genuinely unfinished work.
 */
export function useCartRecovery(cart: Cart, options: { enabled: boolean }) {
  const [recoverable, setRecoverable] = useState<RecoverableCart | null>(null);
  // Read once, before the live cart has a chance to overwrite the snapshot.
  const checkedOnMount = useRef(false);

  useEffect(() => {
    if (checkedOnMount.current) return;
    checkedOnMount.current = true;
    try {
      const saved = parseSnapshot(localStorage.getItem(STORAGE_KEY));
      if (saved) setRecoverable(saved);
      else localStorage.removeItem(STORAGE_KEY);
    } catch {
      // localStorage itself unavailable (private mode, quota): recovery is a
      // convenience and must never interrupt selling.
    }
  }, []);

  // Snapshot on every change. Synchronous and tiny; it runs in an effect so it
  // is never on the critical path between a scan and the cart updating.
  useEffect(() => {
    if (!options.enabled) return;
    const active = cart.lines.filter(line => !line.voided);
    if (active.length === 0) {
      localStorage.removeItem(STORAGE_KEY);
      return;
    }
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify({
        cart,
        savedAt: new Date().toISOString(),
        lineCount: active.length,
      } satisfies RecoverableCart));
    } catch {
      // Quota or private-mode failure: recovery is a convenience, never a
      // reason to interrupt selling.
    }
  }, [cart, options.enabled]);

  /** Called once the operator has chosen — either way the snapshot goes. */
  const dismiss = useCallback(() => {
    setRecoverable(null);
    try { localStorage.removeItem(STORAGE_KEY); } catch { /* non-fatal */ }
  }, []);

  return { recoverable, dismiss } as const;
}
