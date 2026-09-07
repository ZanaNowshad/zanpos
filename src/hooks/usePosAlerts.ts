import { useCallback, useEffect, useRef, useState } from "react";
import type { SessionToken } from "../types";
import {
  ghostSummary,
  paymentConfirmationsUnseenCount,
  whatsappOrderList,
  whatsappPollMessages,
} from "../tauri/commands";

/** Poll cadence. There is no live channel from the sidecar, so WhatsApp
 *  messages and new orders surface on this interval. */
const POLL_MS = 8000;

/**
 * Whether a new-order chime should sound. Pure, and exported so the rule is
 * testable without rendering: chime only on a strict increase — never on the
 * first load (no baseline yet) and never as the cashier works the queue down.
 * A chime on every poll gets the speaker unplugged within a week.
 */
export function shouldChime(previous: number | null, current: number): boolean {
  return previous !== null && current > previous;
}

/**
 * The two POS sidebar badges: the alerts bell and the unfulfilled-orders count.
 *
 * They share one interval deliberately — two timers drifting against each other
 * was how the POS ended up polling more often than intended. Both refreshers
 * swallow their own errors: a badge that fails to update must never surface an
 * error over a cashier's till.
 */
export function usePosAlerts(options: {
  sessionToken: SessionToken;

  /** Bell is admin-only; orders additionally require commerce to be enabled. */
  canOpenBackOffice: boolean;
  commerceEnabled: boolean;
}) {
  const { sessionToken, canOpenBackOffice, commerceEnabled } = options;
  const [notifCount, setNotifCount] = useState(0);
  const [orderCount, setOrderCount] = useState(0);
  const previousOrderCount = useRef<number | null>(null);
  const audioContext = useRef<AudioContext | null>(null);

  /** Short two-tone beep, built rather than shipped as an asset so there is no
   *  file to lose and no new dependency. Fully guarded: a till with no audio
   *  device must not throw into the POS. */
  const playOrderChime = useCallback(() => {
    try {
      const Ctor = window.AudioContext
        || (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
      const ctx = audioContext.current ?? (audioContext.current = new Ctor());
      if (ctx.state === "suspended") void ctx.resume().catch(() => {});
      const now = ctx.currentTime;
      [880, 1320].forEach((freq, index) => {
        const osc = ctx.createOscillator();
        const gain = ctx.createGain();
        osc.type = "sine";
        osc.frequency.value = freq;
        const start = now + index * 0.12;
        gain.gain.setValueAtTime(0.0001, start);
        gain.gain.exponentialRampToValueAtTime(0.2, start + 0.01);
        gain.gain.exponentialRampToValueAtTime(0.0001, start + 0.11);
        osc.connect(gain).connect(ctx.destination);
        osc.start(start);
        osc.stop(start + 0.12);
      });
    } catch { /* non-fatal — e.g. no audio device on this till */ }
  }, []);

  // whatsappPollMessages both ingests new owner/group messages from the sidecar
  // and returns the unread count, so this one call keeps alerts near-real-time.
  const refreshNotifications = useCallback(async () => {
    if (!canOpenBackOffice) return;
    try {
      const [ghosts, waUnread, payUnseen] = await Promise.all([
        ghostSummary(sessionToken),
        whatsappPollMessages(sessionToken).catch(() => 0),
        paymentConfirmationsUnseenCount(sessionToken).catch(() => 0),
      ]);
      /* Coerced, not just added. `+` on a null or a string concatenates rather
         than sums, and the badge then held "0null" — which is also truthy past
         the `count <= 0` guard that is supposed to hide an empty badge, so the
         till showed a permanent alert for no alerts. */
      const n = (value: unknown) => {
        const parsed = Number(value);
        return Number.isFinite(parsed) ? parsed : 0;
      };
      setNotifCount(n(ghosts.pending) + n(ghosts.found) + n(waUnread) + n(payUnseen));
    } catch { /* non-fatal */ }
  }, [canOpenBackOffice, sessionToken]);

  const refreshOrderCount = useCallback(async () => {
    if (!canOpenBackOffice || !commerceEnabled) return;
    try {
      const orders = await whatsappOrderList(sessionToken, "new");
      const count = orders.length;
      if (shouldChime(previousOrderCount.current, count)) playOrderChime();
      previousOrderCount.current = count;
      setOrderCount(count);
    } catch { /* non-fatal */ }
  }, [canOpenBackOffice, commerceEnabled, sessionToken, playOrderChime]);

  useEffect(() => {
    refreshNotifications();
    refreshOrderCount();
    if (!canOpenBackOffice) return;
    const id = setInterval(() => { refreshNotifications(); refreshOrderCount(); }, POLL_MS);
    return () => clearInterval(id);
  }, [refreshNotifications, refreshOrderCount, canOpenBackOffice]);

  return { notifCount, orderCount, refreshNotifications } as const;
}
