import { AlertOctagon, AlertTriangle, CircleAlert, CircleCheck, CircleDashed, Info } from "lucide-react";
import type { Capability } from "./capabilities";
import type { SeverityLevel } from "./statusTypes";
import "./capabilities.css";

/**
 * Store condition: six capabilities, each reporting independently.
 *
 * `compact` is the Today variant — one line per capability, only the ones that
 * need a human are expanded. The full variant is used by System → Health.
 */
const ICON: Record<SeverityLevel, typeof CircleCheck> = {
  ok: CircleCheck,
  info: Info,
  "setup-required": CircleDashed,
  attention: CircleAlert,
  degraded: AlertTriangle,
  blocked: AlertOctagon,
  critical: AlertOctagon,
};

const TONE: Record<SeverityLevel, string> = {
  ok: "ok",
  info: "info",
  "setup-required": "muted",
  attention: "warn",
  degraded: "warn",
  blocked: "danger",
  critical: "danger",
};

interface Props {
  capabilities: Capability[];
  labelFor: (key: string) => string;
  onAction?: (cap: Capability) => void;
  actionLabelFor?: (key: string) => string;
  variant?: "compact" | "full";
}

export default function CapabilityStrip({
  capabilities,
  labelFor,
  onAction,
  actionLabelFor,
  variant = "compact",
}: Props) {
  return (
    <ul className={`zp-caps zp-caps-${variant}`}>
      {capabilities.map(cap => {
        const Icon = ICON[cap.severity];
        const tone = TONE[cap.severity];
        const needsHuman = cap.severity !== "ok" && cap.severity !== "info";
        return (
          <li key={cap.id} className={`zp-cap zp-cap-${tone}`}>
            <span className="zp-cap-icon" aria-hidden="true"><Icon size={15} strokeWidth={1.9} /></span>
            <span className="zp-cap-body">
              <span className="zp-cap-name">{labelFor(cap.labelKey)}</span>
              <span className="zp-cap-state">{cap.state}</span>
              {/* Saying what still works is the whole point of this model. */}
              {variant === "full" && cap.stillWorks && (
                <span className="zp-cap-note">{cap.stillWorks}</span>
              )}
              {variant === "full" && cap.queued && (
                <span className="zp-cap-note">{cap.queued}</span>
              )}
              {variant === "full" && cap.lastOk && (
                <span className="zp-cap-meta">Last success {cap.lastOk}</span>
              )}
            </span>
            {needsHuman && cap.action && onAction && (
              <button type="button" className="zp-cap-action" onClick={() => onAction(cap)}>
                {actionLabelFor ? actionLabelFor(cap.action.labelKey) : "Open"}
              </button>
            )}
          </li>
        );
      })}
    </ul>
  );
}
