import { AlertOctagon, AlertTriangle, CircleAlert, CircleCheck, CircleDashed, Info } from "lucide-react";
import type { Capability } from "./capabilities";
import type { SeverityLevel } from "./statusTypes";
import "./capabilities.css";

/**
 * Store condition: six capabilities, each reporting independently.
 *
 * `full` (System → Health) lists every capability with its notes — that page
 * exists to be read in full. `compact` (Today) shows only what needs a person
 * and folds the rest into one line.
 *
 * The compact variant used to render all six as equal rows, which meant the
 * landing page spent roughly 350px of a 768px screen saying "selling and
 * printing available", "connected", "ready" — three ways of saying nothing is
 * wrong — while the two capabilities that actually wanted attention sat at the
 * bottom of the list. A manager opens Today to find out what needs them.
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
  /* `info` counts as healthy: "catching up" is sync working, not sync broken,
     and promoting it to an exception would cry wolf every few minutes. */
  const needsAttention = (cap: Capability) => cap.severity !== "ok" && cap.severity !== "info";
  const shown = variant === "compact" ? capabilities.filter(needsAttention) : capabilities;
  const folded = variant === "compact" ? capabilities.filter(cap => !needsAttention(cap)) : [];

  return (
    <ul className={`zp-caps zp-caps-${variant}`}>
      {folded.length > 0 && (
        /* One line, names only. No prose, so it needs no new strings and reads
           the same in Arabic: a tick and the list of things that are fine. */
        <li className="zp-cap zp-cap-ok zp-cap-folded">
          <span className="zp-cap-icon" aria-hidden="true"><CircleCheck size={15} strokeWidth={1.9} /></span>
          <span className="zp-cap-folded-names">
            {folded.map(cap => labelFor(cap.labelKey)).join(" · ")}
          </span>
        </li>
      )}
      {shown.map(cap => {
        const Icon = ICON[cap.severity];
        const tone = TONE[cap.severity];
        const needsHuman = needsAttention(cap);
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
