import type { ReactNode } from "react";
import type { StatusLevel } from "./StatusPill";
import "./workspace.css";

export interface PageTemplateHeader {
  title: string;
  subtitle?: string;
  icon?: ReactNode;
  primaryAction?: { label: string; onClick: () => void; primary?: boolean; disabled?: boolean };
  secondaryActions?: { label: string; onClick: () => void; disabled?: boolean }[];
}

export interface DegradedNotice {
  severity: StatusLevel;
  message: string;
  action?: { label: string; onClick: () => void };
  onDismiss?: () => void;
}

interface Props {
  header: PageTemplateHeader;
  degraded?: DegradedNotice;
  /** Rendered below the header, above the content. Use for filters, tabs, command strips. */
  toolbar?: ReactNode;
  /**
   * Drop the content panel's own border/padding. Use when the content already
   * draws its own container (a DataTable, for example) — otherwise the page
   * renders a bordered card inside a bordered card.
   */
  contentFlat?: boolean;
  children: ReactNode;
}

/**
 * Shared page template used by all Command workspace pages.
 *
 * Provides a consistent header (title + actions), optional degraded-state
 * banner, and an optional toolbar slot.  The content area is a plain
 * `<section>` that the caller fills.
 */
export default function PageTemplate({ header, degraded, toolbar, contentFlat, children }: Props) {
  return (
    <div className="oa-workspace zp-workspace">
      {/* ── Header ── */}
      <header className="oa-topbar">
        <div className="oa-title-block">
          {header.icon && <span className="oa-title-icon">{header.icon}</span>}
          <div>
            <h1 className="oa-title">{header.title}</h1>
            {header.subtitle && <p className="oa-subtitle">{header.subtitle}</p>}
          </div>
        </div>
        <div className="oa-topbar-actions">
          {header.secondaryActions?.map((a, i) => (
            <button key={i} className="oa-tool-btn" onClick={a.onClick} disabled={a.disabled}>
              {a.label}
            </button>
          ))}
          {header.primaryAction && (
            <button
              className="oa-primary-mini"
              onClick={header.primaryAction.onClick}
              disabled={header.primaryAction.disabled}
            >
              {header.primaryAction.label}
            </button>
          )}
        </div>
      </header>

      {/* ── Degraded banner ── */}
      {degraded && (
        <div className={`oa-error-banner${degraded.severity === "ok" ? " oa-error-banner-ok" : ""}`} role="alert">
          <span className="oa-error-banner-text">{degraded.message}</span>
          <div className="oa-error-banner-actions">
            {degraded.action && (
              <button className="oa-error-banner-fix" onClick={degraded.action.onClick}>
                {degraded.action.label}
              </button>
            )}
            {degraded.onDismiss && (
              <button className="oa-error-banner-close" onClick={degraded.onDismiss} title="Dismiss" aria-label="Dismiss">
                ✕
              </button>
            )}
          </div>
        </div>
      )}

      {/* ── Toolbar ── */}
      {toolbar && <div className="oa-subnav-line">{toolbar}</div>}

      {/* ── Content ── */}
      <section className={`oa-embedded-tab zp-workspace-content${contentFlat ? " oa-embedded-tab-flat" : ""}`}>
        {children}
      </section>
    </div>
  );
}
