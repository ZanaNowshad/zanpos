import type { ReactNode } from "react";
import { ArrowDown, ArrowUp } from "lucide-react";
import "./datatable.css";

/**
 * Dense, scannable table shell for operational lists.
 *
 * Replaces the ad-hoc `.bo-list` row markup, which could only show a name,
 * a subtitle and a price — not enough to run a catalogue. Columns declare
 * their own alignment and priority so responsive behaviour is data-driven
 * rather than a pile of media queries per screen.
 */
export interface Column<T> {
  id: string;
  header: string;
  /** Cell renderer. Keep it pure — this runs for every visible row. */
  cell: (row: T) => ReactNode;
  align?: "start" | "end";
  /** Fixed width, e.g. "120px". Omit to size by content. */
  width?: string;
  /** Numeric columns get tabular figures so digits line up. */
  numeric?: boolean;
  sortable?: boolean;
  /**
   * 1 = always visible. 3 drops first, then 2, as the table itself narrows —
   * the thresholds are container queries on the table's own width, not the
   * window's. A trailing `rowAction` column is never dropped; it collapses to
   * icons instead, because an action the operator cannot reach is worse than a
   * cramped one.
   */
  priority?: 1 | 2 | 3;
}

export interface SortState {
  columnId: string;
  direction: "asc" | "desc";
}

interface Props<T> {
  columns: Column<T>[];
  rows: T[];
  rowKey: (row: T) => string;
  onRowClick?: (row: T) => void;
  /** Marks the row visually and via aria-selected. */
  isRowActive?: (row: T) => boolean;
  /** Dim rows that are inactive/archived without hiding them. */
  isRowMuted?: (row: T) => boolean;
  sort?: SortState;
  onSortChange?: (sort: SortState) => void;
  caption: string;
  /** Trailing per-row control (e.g. a Label button). Not part of row click. */
  rowAction?: (row: T) => ReactNode;
}

export default function DataTable<T>({
  columns,
  rows,
  rowKey,
  onRowClick,
  isRowActive,
  isRowMuted,
  sort,
  onSortChange,
  caption,
  rowAction,
}: Props<T>) {
  function toggleSort(col: Column<T>) {
    if (!col.sortable || !onSortChange) return;
    const dir = sort?.columnId === col.id && sort.direction === "asc" ? "desc" : "asc";
    onSortChange({ columnId: col.id, direction: dir });
  }

  return (
    <div className="zp-table-wrap">
      <table className="zp-table">
        <caption className="zp-visually-hidden">{caption}</caption>
        <thead>
          <tr>
            {columns.map(col => {
              const active = sort?.columnId === col.id;
              return (
                <th
                  key={col.id}
                  scope="col"
                  className={`zp-col-p${col.priority ?? 1}${col.align === "end" ? " zp-align-end" : ""}`}
                  style={col.width ? { width: col.width } : undefined}
                  aria-sort={active ? (sort!.direction === "asc" ? "ascending" : "descending") : undefined}
                >
                  {col.sortable && onSortChange ? (
                    <button type="button" className="zp-th-sort" onClick={() => toggleSort(col)}>
                      <span>{col.header}</span>
                      {active
                        ? (sort!.direction === "asc"
                            ? <ArrowUp size={13} aria-hidden="true" />
                            : <ArrowDown size={13} aria-hidden="true" />)
                        : <span className="zp-th-sort-hint" aria-hidden="true">↕</span>}
                    </button>
                  ) : (
                    col.header
                  )}
                </th>
              );
            })}
            {rowAction && <th scope="col" className="zp-col-action zp-align-end">&nbsp;</th>}
          </tr>
        </thead>
        <tbody>
          {rows.map(row => {
            const active = isRowActive?.(row) ?? false;
            const muted = isRowMuted?.(row) ?? false;
            return (
              <tr
                key={rowKey(row)}
                className={`${active ? "is-active" : ""} ${muted ? "is-muted" : ""}`.trim()}
                aria-selected={active || undefined}
              >
                {columns.map((col, i) => {
                  const content = col.cell(row);
                  const cls = [
                    `zp-col-p${col.priority ?? 1}`,
                    col.align === "end" ? "zp-align-end" : "",
                    col.numeric ? "zp-numeric" : "",
                  ].filter(Boolean).join(" ");
                  // The first cell carries the row activator so the whole row is
                  // reachable by keyboard through a single real button.
                  if (i === 0 && onRowClick) {
                    return (
                      <td key={col.id} className={cls}>
                        <button type="button" className="zp-row-activator" onClick={() => onRowClick(row)}>
                          {content}
                        </button>
                      </td>
                    );
                  }
                  return <td key={col.id} className={cls}>{content}</td>;
                })}
                {rowAction && <td className="zp-col-action zp-align-end">{rowAction(row)}</td>}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
