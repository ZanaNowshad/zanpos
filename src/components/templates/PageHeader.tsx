import type { ReactNode } from "react";

interface Action {
  label: string;
  onClick: () => void;
  /** When true renders as a primary (accent) button. Defaults to false (ghost). */
  primary?: boolean;
  disabled?: boolean;
}

interface Props {
  title: string;
  subtitle?: string;
  icon?: ReactNode;
  primaryAction?: Action;
  secondaryActions?: Action[];
  children?: ReactNode;
}

export default function PageHeader({
  title,
  subtitle,
  icon,
  primaryAction,
  secondaryActions,
  children,
}: Props) {
  return (
    <header className="oa-topbar">
      <div className="oa-title-block">
        {icon && <span className="oa-title-icon">{icon}</span>}
        <div>
          <h1 className="oa-title">{title}</h1>
          {subtitle && <p className="oa-subtitle">{subtitle}</p>}
        </div>
      </div>
      <div className="oa-topbar-actions">
        {children}
        {secondaryActions?.map((action, i) => (
          <button
            key={i}
            className="oa-tool-btn"
            onClick={action.onClick}
            disabled={action.disabled}
          >
            {action.label}
          </button>
        ))}
        {primaryAction && (
          <button
            className="oa-primary-mini"
            onClick={primaryAction.onClick}
            disabled={primaryAction.disabled}
          >
            {primaryAction.label}
          </button>
        )}
      </div>
    </header>
  );
}
