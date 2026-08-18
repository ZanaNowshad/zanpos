import type { ReactNode } from "react";
import "./datatable.css";

/**
 * Search + filters row for list workspaces.
 *
 * Replaces the lone `.bo-search` input that floated in its own oversized card:
 * the field grew to a sensible width, filters sat nowhere, and the result count
 * was invisible. Filters live beside search so the whole query state reads as
 * one control group.
 */
interface Props {
  search: { value: string; onChange: (v: string) => void; placeholder: string; label: string };
  filters?: ReactNode;
  /** e.g. "1–50 of 1,284". Announced politely so filtering is audible. */
  count?: string;
  /** Shown only when a query or filter is actually applied. */
  onClear?: () => void;
  clearLabel?: string;
}

export default function Toolbar({ search, filters, count, onClear, clearLabel }: Props) {
  return (
    <div className="zp-toolbar" role="search">
      <div className="zp-search">
        <input
          type="search"
          value={search.value}
          placeholder={search.placeholder}
          aria-label={search.label}
          onChange={e => search.onChange(e.target.value)}
        />
      </div>
      {filters}
      {onClear && (
        <button type="button" className="zp-chip-clear" onClick={onClear}>
          {clearLabel ?? "Clear"}
        </button>
      )}
      <span className="zp-toolbar-spacer" />
      {count && <span className="zp-toolbar-count" aria-live="polite">{count}</span>}
    </div>
  );
}
