import { useCallback, useMemo, useState } from "react";
import type { CustomerRow } from "../types";
import { toE164, type ContactSuggestion } from "../components/paymentContacts";

export interface PaymentContactState {
  /** Exactly what the cashier typed — a name, a number, or part of either. */
  query: string;
  setQuery: (value: string) => void;
  selected: ContactSuggestion | null;
  select: (contact: ContactSuggestion) => void;
  clear: () => void;
  /** Where the receipt will be sent, or null while there is nowhere to send it. */
  e164: string | null;
  /** The saved customer to attach to the sale, when the pick is one. */
  customer: CustomerRow | null;
  /** Set only once the cashier has typed something that cannot become a number. */
  error: string | null;
}

/**
 * The one contact a delivery or digital sale is for.
 *
 * The field it drives accepts a name or a number in the same box, because the
 * cashier does not know in advance which one the customer will give them, and
 * a field that only takes digits forces "let me look them up first" into the
 * middle of a queue. What the sale actually needs is a number; that comes
 * either from what was typed, when it is one, or from the person who was
 * picked, when it is not.
 */
export function usePaymentContact(): PaymentContactState {
  const [query, setQueryValue] = useState("");
  const [selected, setSelected] = useState<ContactSuggestion | null>(null);

  const setQuery = useCallback((value: string) => {
    setQueryValue(value);
    /* Editing after choosing someone means they are choosing again. Keeping the
       old selection would send the receipt to the previous customer while the
       field showed the new one's name. */
    setSelected(null);
  }, []);

  const select = useCallback((contact: ContactSuggestion) => {
    setSelected(contact);
    setQueryValue(contact.localPhone || contact.name);
  }, []);

  const clear = useCallback(() => {
    setSelected(null);
    setQueryValue("");
  }, []);

  return useMemo(() => {
    const e164 = selected?.e164 ?? toE164(query);
    const typedDigits = query.replace(/\D/g, "");
    const error = !query.trim() || e164
      ? null
      : typedDigits.length > 0 && typedDigits.length < 8
        ? `${8 - typedDigits.length} more digit${8 - typedDigits.length === 1 ? "" : "s"}`
        : "Pick a customer below, or type their 8-digit number";
    return {
      query, setQuery, selected, select, clear,
      e164, customer: selected?.customer ?? null, error,
    };
  }, [query, selected, setQuery, select, clear]);
}
