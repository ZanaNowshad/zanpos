import type { ReactNode } from "react";
import type { StatusLevel } from "./StatusPill";
import "./workspace.css";
import { useTitleIsRedundant } from "../../navigation/PageHeadingContext";
import OverflowMenu from "./OverflowMenu";

/** Beyond this many secondary actions the strip cannot also hold filters. */
const MAX_INLINE_ACTIONS = 2;

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
  /* When the section tab already says "Categories", the heading saying it again
     is a third statement of the same location. Hidden, not deleted — the
     document still needs an h1, and a tab strip is a <nav>. */
  const titleRedundant = useTitleIsRedundant(header.title);

  /* Two secondary actions fit beside a primary and a filter strip; six do not.
     Past that, the rare ones go behind a single button so the toolbar keeps
     its width and the table keeps its rows. */
  const secondary = header.secondaryActions ?? [];
  const inlineSecondary = secondary.length > MAX_INLINE_ACTIONS ? [] : secondary;
  const overflowSecondary = secondary.length > MAX_INLINE_ACTIONS ? secondary : [];

  const actions = (
    <>
      {inlineSecondary.map((a, i) => (
        <button key={i} className="oa-tool-btn" onClick={a.onClick} disabled={a.disabled}>
          {a.label}
        </button>
      ))}
      {overflowSecondary.length > 0 && <OverflowMenu actions={overflowSecondary} />}
      {header.primaryAction && (
        <button
          className="oa-primary-mini"
          onClick={header.primaryAction.onClick}
          disabled={header.primaryAction.disabled}
        >
          {header.primaryAction.label}
        </button>
      )}
    </>
  );

  /* Hiding the title was not enough: the band kept its padding and its row of
     buttons, so Products spent 85px on a header whose only text was invisible.
     With a toolbar directly beneath it that is two horizontal strips doing one
     job. When the heading says nothing the buttons drop onto a single strip
     and the band goes away — worth ~85px of a 768px screen, which is two more
     product rows.

     A subtitle is real content, so it keeps the band. A missing toolbar is
     not a reason to keep it: Staff has no filters and still spent ~100px on a
     title nobody needed and one button, because below 1024px the band stacks
     its actions under the title. The strip renders either way. */
  const collapseHeader = titleRedundant && !header.subtitle;

  return (
    <div className="oa-workspace zp-workspace">
      {/* ── Header ── */}
      {collapseHeader ? (
        <h1 className="oa-title zp-visually-hidden">{header.title}</h1>
      ) : (
        <header className="oa-topbar">
          <div className="oa-title-block">
            {header.icon && !titleRedundant && <span className="oa-title-icon">{header.icon}</span>}
            <div>
              <h1 className={titleRedundant ? "oa-title zp-visually-hidden" : "oa-title"}>{header.title}</h1>
              {header.subtitle && <p className="oa-subtitle">{header.subtitle}</p>}
            </div>
          </div>
          <div className="oa-topbar-actions">{actions}</div>
        </header>
      )}

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

      {/* ── Toolbar (carries the page actions when the header band collapsed) ── */}
      {(toolbar || collapseHeader) && (
        <div className={`oa-subnav-line${collapseHeader ? " oa-subnav-line-merged" : ""}`}>
          {toolbar}
          {collapseHeader && <div className="oa-subnav-actions">{actions}</div>}
        </div>
      )}

      {/* ── Content ── */}
      <section className={`oa-embedded-tab zp-workspace-content${contentFlat ? " oa-embedded-tab-flat" : ""}`}>
        {children}
      </section>
    </div>
  );
}
