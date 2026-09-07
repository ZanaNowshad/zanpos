import { forwardRef, useEffect, useImperativeHandle, useState, type ReactNode } from "react";
import { useContactSearch } from "../hooks/useContactSearch";
import ContactRow from "./ContactRow";
import type { ContactSuggestion } from "./paymentContacts";
import type { SessionToken } from "../types";

export interface SuggestionListHandle {
  /** Walk the list. Negative moves up; -1 means "back to the typed text". */
  move: (delta: number) => void;
  /** The row the caret is on, or null when the cashier is still typing. */
  take: () => ContactSuggestion | null;
}

interface Props {
  query: string;
  sessionToken?: SessionToken;
  open: boolean;
  onPick: (contact: ContactSuggestion) => void;
  limit?: number;
  className?: string;
  label?: string;
  /** Shown in place of rows while searching or when nothing matched. */
  renderEmpty?: (state: { loading: boolean; empty: boolean }) => ReactNode;
  /** Start on the first row rather than on the typed text. */
  autoHighlight?: boolean;
}

/**
 * Everyone matching what has been typed — deliberately its own component.
 *
 * This started as part of the field it sits under, with the lookup's state
 * living beside the `<input>`. That cost characters. The search is debounced
 * and resolves from a timer, so an arriving result could re-render the field on
 * its own, and React would write the `value` prop it already had back onto the
 * input and update its value tracker to match. The next keystroke was then
 * diffed against a stale base and silently discarded: typing "36001122" at
 * speed produced "3600112", and after choosing a suggestion — which sets state
 * and moves focus programmatically in the same handler — sometimes nothing at
 * all.
 *
 * Splitting the two means an arriving search result can only ever re-render the
 * list. The input's value depends on exactly one thing, the query state above
 * it, and nothing local can commit a render behind its back. Both the inline
 * dropdown and the directory dialog use this, because both had the fault.
 */
const ContactSuggestions = forwardRef<SuggestionListHandle, Props>(
  function ContactSuggestions({
    query, sessionToken, open, onPick, limit = 5,
    className = "pm-contact-suggestions", label = "Matching people",
    renderEmpty, autoHighlight = false,
  }, ref) {
    const [highlighted, setHighlighted] = useState(autoHighlight ? 0 : -1);
    const { rows, loading, empty } = useContactSearch(sessionToken, query, limit);
    const floor = autoHighlight ? 0 : -1;

    useEffect(() => { setHighlighted(floor); }, [query, floor]);

    useImperativeHandle(ref, () => ({
      move: delta => setHighlighted(current =>
        Math.max(floor, Math.min(rows.length - 1, current + delta))),
      take: () => rows[highlighted] ?? null,
    }), [rows, highlighted, floor]);

    if (!open) return null;
    if (rows.length === 0) return <>{renderEmpty?.({ loading, empty })}</>;

    return (
      <div className={className} id="payment-contact-suggestions" role="listbox" aria-label={label}>
        {rows.map((row, index) => (
          <button
            type="button"
            key={row.key}
            role="option"
            aria-selected={index === highlighted}
            className={index === highlighted ? "is-highlighted" : undefined}
            onMouseEnter={() => setHighlighted(index)}
            /* mousedown, not click: blur fires first and would close the list
               out from under the finger. */
            onMouseDown={event => { event.preventDefault(); onPick(row); }}
          >
            <ContactRow contact={row} />
          </button>
        ))}
      </div>
    );
  },
);

export default ContactSuggestions;
