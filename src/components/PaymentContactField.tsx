import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { Check, MessageCircle, UserRound, UsersRound, X } from "lucide-react";
import type { PaymentContactState } from "../hooks/usePaymentContact";
import type { ContactSuggestion } from "./paymentContacts";
import ContactSuggestions, { type SuggestionListHandle } from "./ContactSuggestions";
import CustomerDirectoryDialog from "./CustomerDirectoryDialog";

interface Props {
  contact: PaymentContactState;
  sessionUserId?: string;
  onFocus: () => void;
  /** True when this journey sends the receipt over WhatsApp rather than paper. */
  isDelivery: boolean;
}

/**
 * Who the sale is for — one box that takes a name or a number.
 *
 * It used to strip every non-digit as it was typed, so a cashier told "it's for
 * Fatima" had to leave the field, open a separate directory, and come back. The
 * customer standing at the counter says whichever of the two they think of
 * first, and the till should take either.
 *
 * The list underneath merges the customer table with the WhatsApp address book
 * and recent chats, so someone the shop has messaged but never saved is as
 * findable as one who is on file. It is a separate component for a reason worth
 * knowing before merging it back: see ContactSuggestions.
 */
export default function PaymentContactField({ contact, sessionUserId, onFocus, isDelivery }: Props) {
  const [directoryOpen, setDirectoryOpen] = useState(false);
  const [dropdownOpen, setDropdownOpen] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<SuggestionListHandle>(null);
  /* Closing on blur has to be deferred, or the list is gone before the tap that
     chose a row lands on it. The timer then has to be cancelled when focus
     comes back: without that, tapping away and returning within the delay let a
     stale timer close the list a moment after the cashier had reopened it, and
     no amount of typing brought it back. */
  const blurTimer = useRef<number | null>(null);
  const cancelClose = () => {
    if (blurTimer.current !== null) window.clearTimeout(blurTimer.current);
    blurTimer.current = null;
  };
  useEffect(() => cancelClose, []);

  const choose = (row: ContactSuggestion) => {
    contact.select(row);
    setDropdownOpen(false);
    setDirectoryOpen(false);
    inputRef.current?.focus();
  };

  /* Arrow keys walk the list and Enter takes the highlighted row. Enter with
     nothing highlighted is left alone so it can reach the modal's own
     field-advance handler. */
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      setDropdownOpen(true);
      listRef.current?.move(event.key === "ArrowDown" ? 1 : -1);
      return;
    }
    if (event.key === "Enter") {
      const pick = listRef.current?.take();
      if (!pick) return;
      event.preventDefault();
      event.stopPropagation();
      choose(pick);
    }
  };

  return (
    <div className="pm-contact-block">
      <div className="pm-contact-heading">
        <div>
          <span className="pm-contact-kicker">
            {isDelivery ? "Order contact" : "Send receipt by WhatsApp"}
          </span>
          <label htmlFor="payment-customer-phone">Customer name or number <span aria-hidden="true">*</span></label>
        </div>
        <button
          type="button"
          className="pm-directory-btn"
          aria-label="Browse the customer directory"
          title="Browse the customer directory"
          onClick={() => setDirectoryOpen(true)}
        >
          <UsersRound size={17} aria-hidden="true" />
          <span>Browse</span>
        </button>
      </div>

      <div className={`pm-contact-input-row${contact.error ? " is-error" : ""}${contact.e164 ? " is-ready" : ""}`}>
        <input
          ref={inputRef}
          id="payment-customer-phone"
          className="pm-contact-input"
          /* Not `numeric`: the field takes a name too, and an inputmode of
             numeric tells a touch keyboard to offer digits only. The keypad is
             still what opens for it — see data-keyboard. */
          inputMode="text"
          data-keyboard="pad"
          autoComplete="off"
          enterKeyHint="next"
          aria-autocomplete="list"
          aria-expanded={dropdownOpen}
          aria-controls="payment-contact-suggestions"
          role="combobox"
          placeholder="Fatima, or 33050666"
          value={contact.query}
          onFocus={() => { cancelClose(); onFocus(); setDropdownOpen(true); }}
          onBlur={() => {
            cancelClose();
            blurTimer.current = window.setTimeout(() => setDropdownOpen(false), 160);
          }}
          onKeyDown={onKeyDown}
          onChange={event => { cancelClose(); contact.setQuery(event.target.value); setDropdownOpen(true); }}
        />
        {contact.query && (
          <button
            type="button"
            className="pm-contact-clear"
            aria-label="Clear customer"
            onMouseDown={event => event.preventDefault()}
            onClick={() => { contact.clear(); inputRef.current?.focus(); }}
          >
            <X size={15} />
          </button>
        )}
      </div>

      <ContactStatus contact={contact} />

      <ContactSuggestions
        ref={listRef}
        query={contact.query}
        sessionUserId={sessionUserId}
        open={dropdownOpen && !contact.selected}
        onPick={choose}
      />

      {directoryOpen && (
        <CustomerDirectoryDialog
          sessionUserId={sessionUserId}
          onPick={choose}
          onClose={() => setDirectoryOpen(false)}
        />
      )}
    </div>
  );
}

/**
 * One line under the field saying where the receipt is going, or what is
 * stopping it. Never both, and never blank once anything has been typed —
 * silence after typing reads as "still thinking" and the cashier waits.
 */
function ContactStatus({ contact }: { contact: PaymentContactState }) {
  if (contact.error) {
    return <div className="pm-contact-error" role="status">{contact.error}</div>;
  }
  if (contact.selected) {
    return (
      <div className="pm-contact-match" role="status">
        <UserRound size={14} aria-hidden="true" />
        <strong>{contact.selected.name}</strong>
        <span>{contact.selected.e164 ?? "no WhatsApp number on file"}</span>
        {contact.selected.sources.includes("chat") && <MessageCircle size={13} aria-hidden="true" />}
      </div>
    );
  }
  if (contact.e164) {
    return (
      <div className="pm-contact-match" role="status">
        <Check size={14} aria-hidden="true" />
        <span>Receipt goes to {contact.e164}</span>
      </div>
    );
  }
  return (
    <div className="pm-contact-help">
      Type a name to search, or 8 digits to send to a number that is not saved.
    </div>
  );
}
