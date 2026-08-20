import { useEffect, useState } from "react";
import { Search, UserRound, UsersRound, X } from "lucide-react";
import type { CustomerRow } from "../types";
import { customerList } from "../tauri/commands";
import { normalizePhone } from "./DeliveryForm";

interface Props {
  phoneRaw: string;
  phoneError: string | null;
  selectedCustomer: CustomerRow | null;
  suggestions: CustomerRow[];
  showSuggestions: boolean;
  sessionUserId?: string;
  onPhoneChange: (raw: string) => void;
  onPhoneBlur: () => void;
  onSelect: (customer: CustomerRow) => void;
  onClearCustomer: () => void;
  onFocus: () => void;
}

function phoneDigits(phone: string | null | undefined): string {
  return (phone ?? "").replace(/\D/g, "").replace(/^(00)?973/, "").slice(-8);
}

export default function PaymentContactField({
  phoneRaw,
  phoneError,
  selectedCustomer,
  suggestions,
  showSuggestions,
  sessionUserId,
  onPhoneChange,
  onPhoneBlur,
  onSelect,
  onClearCustomer,
  onFocus,
}: Props) {
  const [directoryOpen, setDirectoryOpen] = useState(false);
  const [directoryQuery, setDirectoryQuery] = useState("");
  const [directoryResults, setDirectoryResults] = useState<CustomerRow[]>([]);

  useEffect(() => {
    if (!directoryOpen) return;
    let cancelled = false;
    const timer = window.setTimeout(() => {
      customerList(sessionUserId ?? "", directoryQuery.trim(), 0, 20)
        .then(page => { if (!cancelled) setDirectoryResults(page.items); })
        .catch(() => { if (!cancelled) setDirectoryResults([]); });
    }, 180);
    return () => { cancelled = true; window.clearTimeout(timer); };
  }, [directoryOpen, directoryQuery, sessionUserId]);

  const choose = (customer: CustomerRow) => {
    onSelect(customer);
    setDirectoryOpen(false);
    setDirectoryQuery("");
  };

  return (
    <div className="pm-contact-block">
      <div className="pm-contact-heading">
        <div>
          <span className="pm-contact-kicker">Send receipt by WhatsApp</span>
          <label htmlFor="payment-customer-phone">Customer phone <span aria-hidden="true">*</span></label>
        </div>
        {selectedCustomer && (
          <button className="pm-contact-clear" onClick={onClearCustomer} aria-label="Clear selected customer">
            <X size={14} />
          </button>
        )}
      </div>

      <div className="pm-contact-input-row">
        <span className="pm-contact-prefix">+973</span>
        <input
          id="payment-customer-phone"
          className={phoneError ? "pm-contact-input pm-contact-input-error" : "pm-contact-input"}
          inputMode="numeric"
          autoComplete="tel"
          enterKeyHint="next"
          aria-describedby="payment-phone-help"
          placeholder="33050666"
          value={phoneRaw}
          onFocus={onFocus}
          onBlur={onPhoneBlur}
          onChange={event => onPhoneChange(event.target.value.replace(/\D/g, "").slice(0, 8))}
        />
        <button
          className="pm-directory-btn"
          aria-label="Open customer directory"
          title="Open customer directory"
          onClick={() => setDirectoryOpen(true)}
        >
          <UsersRound size={18} />
        </button>
      </div>

      <div id="payment-phone-help" className="pm-contact-help">Tap the phone field, then use the dialpad</div>

      {selectedCustomer ? (
        <div className="pm-contact-match"><UserRound size={14} /> {selectedCustomer.name}</div>
      ) : phoneRaw.length === 8 && !phoneError ? (
        <div className="pm-contact-new">No saved match is required — this number will receive the WhatsApp receipt.</div>
      ) : null}
      {phoneError && <div className="pm-contact-error">{phoneError}</div>}

      {showSuggestions && suggestions.length > 0 && !selectedCustomer && (
        <div className="pm-contact-suggestions" role="listbox" aria-label="Matching customers">
          {suggestions.map(customer => (
            <button key={customer.customer_id} role="option" onMouseDown={() => choose(customer)}>
              <span>{customer.name}</span><small>{customer.phone}</small>
            </button>
          ))}
        </div>
      )}

      {directoryOpen && (
        <div className="pm-directory-backdrop" role="presentation" onMouseDown={() => setDirectoryOpen(false)}>
          <div className="pm-directory-dialog" role="dialog" aria-modal="true" aria-labelledby="customer-directory-title" onMouseDown={event => event.stopPropagation()}>
            <div className="pm-directory-header">
              <div><span>Customers</span><h3 id="customer-directory-title">Customer directory</h3></div>
              <button aria-label="Close customer directory" onClick={() => setDirectoryOpen(false)}><X size={18} /></button>
            </div>
            <div className="pm-directory-search"><Search size={17} /><input autoFocus placeholder="Search name or phone" value={directoryQuery} onChange={event => setDirectoryQuery(event.target.value)} /></div>
            <div className="pm-directory-results">
              {directoryResults.map(customer => (
                <button key={customer.customer_id} onClick={() => choose(customer)}>
                  <span>{customer.name}</span><small>{phoneDigits(customer.phone) || "No phone"}</small>
                </button>
              ))}
              {directoryResults.length === 0 && <div className="pm-directory-empty">No matching customers</div>}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

interface BlockProps {
  phoneRaw: string;
  phoneError: string | null;
  selectedCustomer: CustomerRow | null;
  suggestions: CustomerRow[];
  showSuggestions: boolean;
  sessionUserId?: string;
  onFocus: () => void;
  onPhoneBlur: () => void;
  changeCustomerSearch: (raw: string) => void;
  selectCustomer: (customer: CustomerRow) => void;
  removeCustomer: () => void;
  setPhoneRaw: (raw: string) => void;
  setPhoneError: (message: string | null) => void;
  setContactNumber: (normalized: string) => void;
}

/**
 * The contact field plus the three ways its value changes: typing, picking a
 * saved customer, and clearing one.
 *
 * Those handlers were thirty lines of inline arrow functions in PaymentModal,
 * which is where the 500-line limit first bit. They belong next to the field
 * they drive anyway — each one has to keep `phoneRaw`, the error and the
 * normalised `contact_number` in step, and doing that in three places written
 * far apart is how they drift.
 */
export function PaymentContactBlock({
  phoneRaw, phoneError, selectedCustomer, suggestions, showSuggestions, sessionUserId,
  onFocus, onPhoneBlur, changeCustomerSearch, selectCustomer, removeCustomer,
  setPhoneRaw, setPhoneError, setContactNumber,
}: BlockProps) {
  return (
    <PaymentContactField
      phoneRaw={phoneRaw}
      phoneError={phoneError}
      selectedCustomer={selectedCustomer}
      suggestions={suggestions}
      showSuggestions={showSuggestions}
      sessionUserId={sessionUserId}
      onFocus={onFocus}
      onPhoneBlur={onPhoneBlur}
      onPhoneChange={raw => {
        const normalized = normalizePhone(raw);
        setPhoneRaw(raw);
        setPhoneError(raw && !normalized ? "Enter 8 digits" : null);
        setContactNumber(normalized ?? "");
        changeCustomerSearch(raw);
      }}
      onSelect={customer => {
        const raw = (customer.phone ?? "").replace(/\D/g, "").replace(/^(00)?973/, "").slice(-8);
        const normalized = normalizePhone(raw);
        selectCustomer(customer);
        setPhoneRaw(raw);
        setPhoneError(normalized ? null : "This customer does not have a valid Bahrain mobile number");
        setContactNumber(normalized ?? "");
      }}
      onClearCustomer={() => {
        removeCustomer();
        setPhoneRaw("");
        setPhoneError(null);
        setContactNumber("");
      }}
    />
  );
}
