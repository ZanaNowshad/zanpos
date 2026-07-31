import { MessageCircle, MessageCircleOff } from "lucide-react";

interface Props {
  /** null while the first status check is still in flight. */
  connected: boolean | null;
  /** True when the most recent check failed, so `connected` is last-known,
   *  not current. Passed as a flag rather than a timestamp so nothing has to
   *  read the clock during render. */
  stale: boolean;
}

/**
 * WhatsApp connection state, at the till rather than only in Settings.
 *
 * This exists because the failure is silent: when the sidecar drops, orders
 * simply stop arriving. Nothing on screen changes, no error appears, and the
 * first anyone knows is a customer asking where their order went. A watchdog
 * already restarts the sidecar — what was missing is the cashier being able
 * to see that it happened.
 *
 * Three states, deliberately: connected, degraded (a check has not succeeded
 * recently — usually a restart in progress), and down. Collapsing degraded
 * into down would make the pill flicker red every time the watchdog does its
 * job, and a pill that cries wolf gets ignored.
 */
export function whatsappHealth(connected: boolean | null, stale: boolean): "ok" | "degraded" | "down" {
  if (connected === null) return "degraded";  // first check still running
  if (!connected) return "down";              // confirmed disconnected
  if (stale) return "degraded";               // last known good, but the check failed
  return "ok";
}

export default function WhatsAppPill({ connected, stale }: Props) {
  const health = whatsappHealth(connected, stale);
  const label = health === "ok" ? "WhatsApp" : health === "degraded" ? "WhatsApp…" : "WhatsApp off";
  const title = {
    ok: "WhatsApp connected — orders are arriving",
    degraded: "Checking WhatsApp — the sidecar may be restarting",
    down: "WhatsApp disconnected — orders are NOT arriving. Pair again in Settings.",
  }[health];

  return (
    <span className={`pos-pill pos-pill-${health}`} title={title} aria-label={title}>
      {health === "down"
        ? <MessageCircleOff size={13} aria-hidden="true" />
        : <MessageCircle size={13} aria-hidden="true" />}
      <span>{label}</span>
    </span>
  );
}
