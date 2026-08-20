import { useEffect, useState } from "react";
import type { RefObject } from "react";

/** What the right-hand column is currently being asked to edit. */
export interface FocusedField {
  label: string;
  value: string;
  /** Numeric fields get the keypad; text fields get the keyboard prompt. */
  numeric: boolean;
}

/**
 * Tracks which real input inside the modal holds the caret.
 *
 * The till has one physical place for input — the right-hand column — and what
 * appears there follows the field being edited rather than the screen being
 * shown. That only works if something knows what the caret is in, and focus is
 * the honest source: it is what the dialpad already types into, so the surface
 * and the keystrokes can never disagree about their target.
 *
 * Reads the label from `aria-label`, then the associated `<label>`, then the
 * placeholder — whichever the field actually has.
 */
export function useFocusedField(containerRef: RefObject<HTMLElement | null>) {
  const [field, setField] = useState<FocusedField | null>(null);

  useEffect(() => {
    const root = containerRef.current;
    if (!root) return;

    const describe = (el: HTMLInputElement | HTMLTextAreaElement): FocusedField => {
      const labelled = el.id ? root.querySelector(`label[for="${el.id}"]`) : null;
      const label =
        el.getAttribute("aria-label")
        ?? labelled?.textContent?.trim()
        ?? el.closest("label")?.textContent?.trim()
        ?? el.getAttribute("placeholder")
        ?? "Value";
      const mode = (el.getAttribute("inputmode") ?? "").toLowerCase();
      const numeric = el.getAttribute("type") === "tel"
        || el.getAttribute("type") === "number"
        || mode === "numeric" || mode === "decimal" || mode === "tel";
      return { label: label.replace(/\s+/g, " ").slice(0, 40), value: el.value, numeric };
    };

    const onFocus = (event: FocusEvent) => {
      const el = event.target;
      if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
        if (el.type === "checkbox" || el.type === "radio") return;
        setField(describe(el));
      }
    };
    /* Typing has to update the readout, and `input` fires on dialpad writes too
       because those go through the native value setter. */
    const onInput = (event: Event) => {
      const el = event.target;
      if ((el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement)
          && el === document.activeElement) {
        setField(describe(el));
      }
    };
    /* Blur to nothing returns the column to the summary. Blur *into* another
       field is handled by that field's own focus event, so this defers. */
    const onBlur = () => {
      setTimeout(() => {
        const active = document.activeElement;
        const stillInField = active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement;
        if (!stillInField || !root.contains(active)) setField(null);
      }, 0);
    };

    root.addEventListener("focusin", onFocus);
    root.addEventListener("focusout", onBlur);
    root.addEventListener("input", onInput);
    return () => {
      root.removeEventListener("focusin", onFocus);
      root.removeEventListener("focusout", onBlur);
      root.removeEventListener("input", onInput);
    };
  }, [containerRef]);

  return field;
}

export interface OrderSummaryRow {
  label: string;
  value: string;
  strong?: boolean;
}

/**
 * The resting state of the input column: what this sale currently amounts to.
 *
 * Shown whenever nothing is being edited, so the column is never blank and the
 * cashier can read back the whole order — total, how it will be paid, who is
 * taking it — without leaving the screen.
 */
export function PaymentOrderSummary(
  { rows, title, note }: { rows: OrderSummaryRow[]; title: string; note?: string },
) {
  return (
    <div className="pm-summary">
      <h3 className="pm-summary-title">{title}</h3>
      <dl className="pm-summary-rows">
        {rows.map(row => (
          <div key={row.label} className={`pm-summary-row${row.strong ? " is-strong" : ""}`}>
            <dt>{row.label}</dt>
            <dd>{row.value}</dd>
          </div>
        ))}
      </dl>
      {/* What the cashier does next when the money is taken elsewhere — the
          card terminal or the wallet app. This used to live in a readiness
          panel that the contextual input surface replaced; the instruction is
          the part of it worth keeping, so it moved here rather than going. */}
      {note && <p className="pm-summary-note">{note}</p>}
    </div>
  );
}
