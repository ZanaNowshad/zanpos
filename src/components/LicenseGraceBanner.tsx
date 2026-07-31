import { useEffect, useState } from "react";
import { ShieldAlert } from "lucide-react";
import { licenseGetEntitlement, type Entitlement } from "../tauri/license";

/**
 * How urgent the grace warning should look.
 *
 * A 30-day grace shown at full volume on day one is noise the operator learns
 * to ignore, and by the time it matters they no longer see it. It stays quiet
 * until the last week, then amber, then red in the final three days.
 */
export function graceTone(daysRemaining: number): "quiet" | "warn" | "urgent" {
  if (daysRemaining <= 3) return "urgent";
  if (daysRemaining <= 7) return "warn";
  return "quiet";
}

export function graceMessage(daysRemaining: number): string {
  if (daysRemaining <= 0) return "Your licence has expired.";
  if (daysRemaining === 1) return "Your licence renews tomorrow.";
  return `Your licence renews in ${daysRemaining} days.`;
}

/**
 * Licence grace countdown.
 *
 * THE RULE THIS OBEYS (product spec): a till that stops selling over billing
 * is a lawsuit and a reputation, in that order. This is a banner and nothing
 * more — it gates no button, blocks no sale, and hides no data. Even in the
 * `lapsed` state it only informs; the back office degrades first and the till
 * never locks.
 *
 * It is also silent when there is nothing to say: an active licence, or a
 * store with no licence at all, renders nothing.
 */
export default function LicenseGraceBanner() {
  const [entitlement, setEntitlement] = useState<Entitlement | null>(null);

  useEffect(() => {
    let cancelled = false;
    licenseGetEntitlement()
      .then(value => { if (!cancelled) setEntitlement(value); })
      .catch(() => {
        // No licence configured, or the check failed. Either way this banner
        // stays quiet — it must never be the reason someone cannot sell.
      });
    return () => { cancelled = true; };
  }, []);

  if (!entitlement || entitlement.state !== "in_grace") return null;

  const days = entitlement.days_remaining;
  const tone = graceTone(days);

  return (
    <div className={`licence-banner licence-banner-${tone}`} role="status">
      <ShieldAlert size={15} aria-hidden="true" />
      <span>
        <strong>{graceMessage(days)}</strong>{" "}
        Selling, printing and your data are unaffected.
      </span>
    </div>
  );
}
