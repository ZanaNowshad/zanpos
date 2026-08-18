import { AlertTriangle, WifiOff, X } from "lucide-react";
import type { ReactNode } from "react";
import type { StatusLevel } from "./StatusPill";

interface Props {
  severity: StatusLevel;
  message: string;
  action?: { label: string; onClick: () => void };
  onDismiss?: () => void;
  icon?: ReactNode;
}

const SEVERITY_ICON: Partial<Record<StatusLevel, ReactNode>> = {
  warning: <AlertTriangle size={15} strokeWidth={1.75} aria-hidden="true" />,
  critical: <AlertTriangle size={15} strokeWidth={1.75} aria-hidden="true" />,
  info: <WifiOff size={15} strokeWidth={1.75} aria-hidden="true" />,
};

export default function DegradedBanner({ severity, message, action, onDismiss, icon }: Props) {
  return (
    <div className={`oa-error-banner${severity === "ok" ? " oa-error-banner-ok" : ""}`} role="alert">
      <span className="oa-error-banner-text">
        {icon ?? SEVERITY_ICON[severity] ?? null}
        {" "}{message}
      </span>
      <div className="oa-error-banner-actions">
        {action && (
          <button className="oa-error-banner-fix" onClick={action.onClick}>
            {action.label}
          </button>
        )}
        {onDismiss && (
          <button className="oa-error-banner-close" onClick={onDismiss} title="Dismiss" aria-label="Dismiss">
            <X size={14} />
          </button>
        )}
      </div>
    </div>
  );
}
