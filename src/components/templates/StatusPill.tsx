import type { ReactNode } from "react";

export type StatusLevel = "ok" | "warning" | "critical" | "info";

interface Props {
  level: StatusLevel;
  label: string;
  detail?: string;
  icon?: ReactNode;
  onClick?: () => void;
}

const LEVEL_CLASS: Record<StatusLevel, string> = {
  ok: "oa-pulse-ok",
  warning: "oa-pulse-warning",
  critical: "oa-pulse-critical",
  info: "",
};

export default function StatusPill({ level, label, detail, icon, onClick }: Props) {
  const className = `oa-pulse-chip ${LEVEL_CLASS[level]}`;
  const title = detail ?? label;
  const description = `${label}${detail ? `: ${detail}` : ""}`;

  // A pill that does something is a button, not a span wearing a role. Native
  // <button> brings Enter *and* Space activation, focus ring and disabled
  // semantics for free; the hand-rolled key handler this replaces was the only
  // thing standing between a keyboard user and the action.
  if (onClick) {
    return (
      <button type="button" className={className} title={title} aria-label={description} onClick={onClick}>
        {icon}
        <span>{label}</span>
      </button>
    );
  }

  return (
    <span className={className} title={title} role="status" aria-label={description}>
      {icon}
      <span>{label}</span>
    </span>
  );
}
