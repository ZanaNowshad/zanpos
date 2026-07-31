import { useEffect, useState } from "react";

const format = () =>
  new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

/**
 * Wall-clock time for the POS header, refreshed every 10 seconds.
 *
 * Ticking well under a minute rather than on the minute boundary keeps the
 * displayed time from lagging by up to a minute after a resume from sleep,
 * which matters on a till where the clock is what a cashier checks against a
 * customer's receipt.
 */
export function useClockTime(): string {
  const [time, setTime] = useState(format);

  useEffect(() => {
    const id = setInterval(() => setTime(format()), 10_000);
    return () => clearInterval(id);
  }, []);

  return time;
}
