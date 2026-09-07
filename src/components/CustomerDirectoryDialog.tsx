import { useEffect, useRef, useState } from "react";
import { Search, X } from "lucide-react";
import ContactSuggestions, { type SuggestionListHandle } from "./ContactSuggestions";
import type { ContactSuggestion } from "./paymentContacts";
import { typeIntoFocusedField } from "./paymentFieldTyping";
import TouchKeyboard from "./TouchKeyboard";
import type { SessionToken } from "../types";

interface Props {
  sessionToken?: SessionToken;
  onPick: (contact: ContactSuggestion) => void;
  onClose: () => void;
}

/**
 * Browse everyone the till knows, when the cashier does not have the number.
 *
 * This used to be a search box that could not be typed into. Three things had
 * to be true for it to work and none were: the field has to actually receive
 * keystrokes, Escape has to close *this* rather than the payment behind it, and
 * the results arriving must not be able to re-render the box mid-word — see
 * ContactSuggestions, which is a separate component for exactly that reason.
 *
 * The keyboard is part of the dialog, not an afterthought: the till is a
 * touchscreen with nothing plugged into it, so a search box with no way to type
 * is decoration.
 */
export default function CustomerDirectoryDialog({ sessionToken, onPick, onClose }: Props) {
  const [query, setQuery] = useState("");
  const [showKeys, setShowKeys] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<SuggestionListHandle>(null);

  useEffect(() => { inputRef.current?.focus(); }, []);

  /*
   * Capture phase, and propagation stopped. The payment modal's focus trap has
   * its own Escape on document, so a bubbling listener here would fire second
   * and close the sale as well as the directory — the cashier would press
   * Escape to back out of a search and lose the basket.
   */
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        onClose();
        return;
      }
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        event.stopPropagation();
        listRef.current?.move(event.key === "ArrowDown" ? 1 : -1);
        return;
      }
      if (event.key === "Enter") {
        /* Enter belongs to the list while it is open. Without this it reached
           the payment modal's confirm handler and completed the sale from
           inside a search box. */
        event.preventDefault();
        event.stopPropagation();
        const pick = listRef.current?.take();
        if (pick) onPick(pick);
      }
    };
    document.addEventListener("keydown", onKey, true);
    return () => document.removeEventListener("keydown", onKey, true);
  }, [onClose, onPick]);

  /* Keeps the caret in the search box while the on-screen keys are used, so a
     tap on a letter has somewhere to land. */
  const typeKey = (key: string) => {
    inputRef.current?.focus();
    typeIntoFocusedField(key);
  };

  return (
    <div className="pm-directory-backdrop" role="presentation" onMouseDown={onClose}>
      <div
        className="pm-directory-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="customer-directory-title"
        onMouseDown={event => event.stopPropagation()}
      >
        <div className="pm-directory-header">
          <div>
            <span>Customers</span>
            <h3 id="customer-directory-title">Customer directory</h3>
          </div>
          <button type="button" aria-label="Close customer directory" onClick={onClose}>
            <X size={18} />
          </button>
        </div>

        <div className="pm-directory-search">
          <Search size={17} aria-hidden="true" />
          <input
            ref={inputRef}
            aria-label="Search customers by name or number"
            placeholder="Name or number — type either"
            value={query}
            onChange={event => setQuery(event.target.value)}
          />
          {query && (
            <button
              type="button"
              className="pm-directory-clear"
              aria-label="Clear search"
              onMouseDown={event => event.preventDefault()}
              onClick={() => { setQuery(""); inputRef.current?.focus(); }}
            >
              <X size={15} />
            </button>
          )}
          <button
            type="button"
            className={`pm-directory-keys-btn${showKeys ? " is-on" : ""}`}
            aria-pressed={showKeys}
            /* Taking focus would move the caret out of the field these keys
               are meant to fill. */
            onMouseDown={event => event.preventDefault()}
            onClick={() => setShowKeys(open => !open)}
          >
            {showKeys ? "Hide keys" : "Keys"}
          </button>
        </div>

        <ContactSuggestions
          ref={listRef}
          query={query}
          sessionToken={sessionToken}
          open
          limit={40}
          autoHighlight
          className="pm-directory-results"
          label="Customers"
          onPick={onPick}
          renderEmpty={({ loading }) => (
            <div className="pm-directory-results">
              <div className="pm-directory-empty">
                {loading
                  ? "Searching…"
                  : `No one matches “${query}”. Type the number into the field instead — the receipt still sends to a customer who has never been saved.`}
              </div>
            </div>
          )}
        />

        {showKeys && (
          <div className="pm-directory-keys">
            <TouchKeyboard onKey={typeKey} />
          </div>
        )}
      </div>
    </div>
  );
}
