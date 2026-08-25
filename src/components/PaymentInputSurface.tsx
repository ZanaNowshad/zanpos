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
      /* `data-keyboard` overrides the inputmode, for the fields where the two
         disagree on purpose. The contact box takes a name as well as a number,
         so its inputmode has to be `text` — but nine entries in ten are a phone
         number, so it should still open on the keypad. */
      const preferred = el.getAttribute("data-keyboard");
      const numeric = preferred === "pad" || (preferred !== "keys" && (
        el.getAttribute("type") === "tel"
        || el.getAttribute("type") === "number"
        || mode === "numeric" || mode === "decimal" || mode === "tel"));
      return { label: label.replace(/\s+/g, " ").slice(0, 40), value: el.value, numeric };
    };

    /* Deferred for the same reason `onInput` below is, and the reason is worth
       reading there before making either of them synchronous again. Focus does
       not change a field's value, so this one looked safe — but a cashier who
       taps a field and starts typing immediately gets the first keystrokes
       inside the render this listener kicked off, and those were the ones that
       disappeared. All three listeners now hand back to React first. */
    const onFocus = (event: FocusEvent) => {
      const el = event.target;
      if (!(el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement)) return;
      if (el.type === "checkbox" || el.type === "radio") return;
      setTimeout(() => {
        if (el === document.activeElement) setField(describe(el));
      }, 0);
    };
    /*
     * Typing has to update the readout, and `input` fires on dialpad writes too
     * because those go through the native value setter.
     *
     * Deferred, and that is not a nicety. React delegates `change` from the
     * root container, so this listener — bound to the dialog, which is *inside*
     * the root — runs first. Updating state here re-rendered the modal
     * synchronously, and React's commit wrote the controlled `value` prop back
     * onto the input, resetting it to what it was before the keystroke. By the
     * time React's own delegated handler ran, its value tracker compared the
     * field against the value it had just restored, saw no change, and never
     * dispatched `onChange`. The keystroke vanished. Every text field in the
     * modal was unusable — the phone number, the address, the customer search —
     * and the cause looked like a focus bug because the caret stayed put.
     *
     * A macrotask puts the re-render after the whole event dispatch, so React
     * sees the change first. A microtask would not: the checkpoint runs between
     * listeners.
     */
    const onInput = (event: Event) => {
      const el = event.target;
      if (!(el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement)) return;
      if (el !== document.activeElement) return;
      setTimeout(() => {
        if (el !== document.activeElement) return;
        const next = describe(el);
        setField(current =>
          current && current.label === next.label && current.value === next.value
            ? current
            : next);
      }, 0);
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
