import type { ReactNode } from "react";
import Button from "./Button";

interface Props {
  loading: boolean;
  error: string | null;
  isEmpty: boolean;
  /** Shown in the empty state — the action that would fill the list. */
  emptyTitle: string;
  emptyHint?: string;
  emptyAction?: { label: string; onAction: () => void };
  onRetry?: () => void;
  /** Rough row count for the skeleton, so it approximates the real list. */
  skeletonRows?: number;
  children: ReactNode;
}

/**
 * The four states every list surface owes the operator.
 *
 * Loading, empty, error and success are not decoration — a list that renders
 * nothing is ambiguous between "still loading", "nothing here yet" and "the
 * query failed", and an operator resolves that ambiguity by waiting, then
 * refreshing, then calling for help. Naming the state removes the guess.
 *
 * The empty state carries an ACTION, not just a message: "No orders yet" is a
 * dead end, "No orders yet — share your store QR" is a next step.
 */
export default function ListState({
  loading, error, isEmpty, emptyTitle, emptyHint, emptyAction,
  onRetry, skeletonRows = 4, children,
}: Props) {
  // Error first: a stale list rendered under a failed refresh is worse than
  // an honest error, because it looks current.
  if (error) {
    return (
      <div className="ui-list-state ui-list-error" role="alert">
        <strong>Could not load this list</strong>
        <span>{error}</span>
        {onRetry && <Button variant="secondary" onClick={onRetry}>Retry</Button>}
      </div>
    );
  }

  if (loading) {
    return (
      <div className="ui-list-state ui-list-loading" aria-busy="true" aria-live="polite">
        <span className="ui-visually-hidden">Loading…</span>
        {Array.from({ length: skeletonRows }, (_, row) => (
          <div key={row} className="ui-skeleton-row" aria-hidden="true" />
        ))}
      </div>
    );
  }

  if (isEmpty) {
    return (
      <div className="ui-list-state ui-list-empty">
        <strong>{emptyTitle}</strong>
        {emptyHint && <span>{emptyHint}</span>}
        {emptyAction && (
          <Button role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}  variant="primary" onClick={emptyAction.onAction}>{emptyAction.label}</Button>
        )}
      </div>
    );
  }

  return <>{children}</>;
}
