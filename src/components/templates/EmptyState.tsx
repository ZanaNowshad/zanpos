import type { ReactNode } from "react";
import { AlertTriangle, Lock, PackageOpen, SearchX } from "lucide-react";

/**
 * Empty states are chosen from application STATE, never from appearance.
 *
 * The four cases mean different things to the person looking at the screen and
 * must not share copy. The bug this replaces: a brand-new store with an empty
 * catalogue was told to "try a different search term" — no-results copy shown
 * for a first-use situation, at the single most important onboarding moment.
 */
export type EmptyStateVariant =
  /** No records have ever existed. Teach what this area is for. */
  | "first-use"
  /** Records exist; the current query or filters exclude them all. */
  | "no-results"
  /** A capability is unavailable, so data could not load. */
  | "degraded"
  /** Records exist but this role may not see them. */
  | "restricted";

interface Action {
  label: string;
  onClick: () => void;
  primary?: boolean;
}

interface Props {
  variant?: EmptyStateVariant;
  icon?: ReactNode;
  title: string;
  description?: string;
  /** Extra line for degraded states: what still works despite the failure. */
  stillWorks?: string;
  actions?: Action[];
}

const DEFAULT_ICON: Record<EmptyStateVariant, ReactNode> = {
  "first-use": <PackageOpen size={32} strokeWidth={1.5} />,
  "no-results": <SearchX size={32} strokeWidth={1.5} />,
  degraded: <AlertTriangle size={32} strokeWidth={1.5} />,
  restricted: <Lock size={32} strokeWidth={1.5} />,
};

export default function EmptyState({
  variant = "first-use",
  icon,
  title,
  description,
  stillWorks,
  actions,
}: Props) {
  return (
    <div className={`oa-empty-state zp-empty zp-empty-${variant}`} role="status">
      <div className="oa-empty-icon zp-empty-icon" aria-hidden="true">
        {icon ?? DEFAULT_ICON[variant]}
      </div>
      <h3>{title}</h3>
      {description && <p>{description}</p>}
      {stillWorks && <p className="zp-empty-stillworks">{stillWorks}</p>}
      {actions && actions.length > 0 && (
        <div className="oa-empty-actions">
          {actions.map((action, i) => (
            <button
              key={i}
              type="button"
              className={action.primary ? "oa-primary-mini" : "oa-ghost-mini"}
              onClick={action.onClick}
            >
              {action.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
