import type { ReactNode } from "react";

type SkeletonVariant = "table" | "card" | "detail" | "text";

interface Props {
  variant?: SkeletonVariant;
  /** Number of skeleton rows/cards to render. Default: table=5, card=3, detail=4, text=1. */
  count?: number;
  /** When true, renders a full-page skeleton (header + content). */
  fullPage?: boolean;
  /** Optional override class for the wrapper. */
  className?: string;
}

/** Skeleton rows for a table-like list. */
function TableSkeleton({ rows }: { rows: number }) {
  return (
    <div className="oa-skeleton-table" aria-hidden="true">
      {Array.from({ length: rows }).map((_, i) => (
        <div key={i} className="oa-skeleton-row">
          <span className="oa-skeleton-cell" style={{ width: `${30 + Math.sin(i * 2.3) * 15}%` }} />
          <span className="oa-skeleton-cell" style={{ width: `${15 + Math.cos(i * 1.7) * 10}%` }} />
          <span className="oa-skeleton-cell" style={{ width: `${10 + Math.sin(i * 3.1) * 8}%` }} />
        </div>
      ))}
    </div>
  );
}

/** Skeleton cards in a grid. */
function CardSkeleton({ cards }: { cards: number }) {
  return (
    <div className="oa-skeleton-cards" aria-hidden="true">
      {Array.from({ length: cards }).map((_, i) => (
        <div key={i} className="oa-skeleton-card">
          <div className="oa-skeleton-line" style={{ width: "60%" }} />
          <div className="oa-skeleton-line" style={{ width: "85%" }} />
          <div className="oa-skeleton-line" style={{ width: "40%" }} />
        </div>
      ))}
    </div>
  );
}

/** Skeleton for a detail / record view with label–value pairs. */
function DetailSkeleton({ rows }: { rows: number }) {
  return (
    <div className="oa-skeleton-detail" aria-hidden="true">
      {Array.from({ length: rows }).map((_, i) => (
        <div key={i} className="oa-skeleton-field">
          <span className="oa-skeleton-label" />
          <span className="oa-skeleton-value" style={{ width: `${25 + Math.sin(i * 1.9) * 20}%` }} />
        </div>
      ))}
    </div>
  );
}

/** Simple inline text skeleton (one or more lines). */
function TextSkeleton({ lines }: { lines: number }) {
  return (
    <div className="oa-skeleton-text" aria-hidden="true">
      {Array.from({ length: lines }).map((_, i) => (
        <div key={i} className="oa-skeleton-line" style={{ width: i === lines - 1 ? "40%" : "100%" }} />
      ))}
    </div>
  );
}

const SKELETON_MAP: Record<SkeletonVariant, (c: number) => ReactNode> = {
  table: (c) => <TableSkeleton rows={c} />,
  card: (c) => <CardSkeleton cards={c} />,
  detail: (c) => <DetailSkeleton rows={c} />,
  text: (c) => <TextSkeleton lines={c} />,
};

export default function LoadingSkeleton({
  variant = "table",
  count,
  fullPage = false,
  className,
}: Props) {
  const actualCount = count ?? (variant === "table" ? 5 : variant === "card" ? 3 : variant === "detail" ? 4 : 1);
  const content = SKELETON_MAP[variant](actualCount);

  if (!fullPage) {
    return <div className={className} role="status" aria-label="Loading…">{content}</div>;
  }

  return (
    <div className={`oa-workspace ${className ?? ""}`} role="status" aria-label="Loading page…">
      <header className="oa-topbar">
        <div className="oa-title-block">
          <span className="oa-skeleton-line" style={{ width: "180px", height: "1.25rem" }} />
        </div>
      </header>
      <section className="oa-embedded-tab">
        {content}
      </section>
    </div>
  );
}
