import { useEffect, useRef, useState } from "react";
import type { WaContact, WaMessage } from "../types";
import { customerList, whatsappListContacts, whatsappListMessages } from "../tauri/commands";
import { mergeContacts, rankContacts, type ContactSuggestion } from "../components/paymentContacts";
import type { SessionToken } from "../types";

/**
 * The WhatsApp address book and the chat list are whole-list reads, and both
 * are answered from local tables. Fetching them per keystroke would be wasteful
 * and, worse, would make the suggestion list flicker between "found" and
 * "searching" while the cashier types. They are read once per till session and
 * shared by every field that searches contacts.
 *
 * Keyed by actor because the commands are, and cleared on failure rather than
 * cached as empty — a WhatsApp that was disconnected at open should start
 * working when it reconnects, without a restart.
 */
const waCache = new Map<string, Promise<{ contacts: WaContact[]; messages: WaMessage[] }>>();

function loadWhatsApp(actor: SessionToken) {
  const hit = waCache.get(actor);
  if (hit) return hit;
  const pending = Promise.all([
    // A disconnected WhatsApp is the normal state on a fresh till, not an
    // error worth failing the whole lookup over. Saved customers still answer.
    whatsappListContacts(actor).catch(() => [] as WaContact[]),
    whatsappListMessages(actor).catch(() => [] as WaMessage[]),
  ]).then(([contacts, messages]) => {
    if (contacts.length === 0 && messages.length === 0) waCache.delete(actor);
    return { contacts, messages };
  }).catch(error => {
    waCache.delete(actor);
    console.warn("WhatsApp directory unavailable:", error);
    return { contacts: [] as WaContact[], messages: [] as WaMessage[] };
  });
  waCache.set(actor, pending);
  return pending;
}

/** Test seam: drops the shared WhatsApp read so the next search refetches. */
export function resetContactCache(): void {
  waCache.clear();
}

interface Result {
  rows: ContactSuggestion[];
  loading: boolean;
  /** True once a search has run and come back with nothing. */
  empty: boolean;
}

/**
 * Everyone the till knows, ranked against what has been typed.
 *
 * Customers are searched server-side because the catalogue of them is large;
 * WhatsApp is filtered here because it is already in memory. Both feed one
 * ranked list, so a person the shop has chatted with but never saved is just as
 * findable as one in the customer table.
 */
export function useContactSearch(
  sessionToken: SessionToken | undefined, query: string, limit = 6,
): Result {
  const [rows, setRows] = useState<ContactSuggestion[]>([]);
  const [loading, setLoading] = useState(false);
  const [searched, setSearched] = useState(false);
  const runId = useRef(0);

  useEffect(() => {
    // Without a session there is nobody to search on behalf of. The old
    // fallback was an empty string, which every command rejected anyway.
    const actor = sessionToken;
    const trimmed = query.trim();
    const id = ++runId.current;
    setLoading(true);

    const timer = window.setTimeout(() => {
      /* One query, because customer_list itself matches a phone both as it was
         written and with the punctuation stripped. Asking twice here — once for
         the text, once for the digits — was the first attempt, and it papered
         over a backend that could not find "+973 3600 1122" from "36001122"
         for any other caller. */
      if (!actor) { setLoading(false); return; }
      Promise.all([
        customerList(actor, trimmed, 0, Math.max(limit * 4, 40))
          .then(page => page.items)
          .catch(() => []),
        loadWhatsApp(actor),
      ]).then(([customers, wa]) => {
        if (id !== runId.current) return;
        setRows(rankContacts(mergeContacts(customers, wa.contacts, wa.messages), trimmed, limit));
        setLoading(false);
        setSearched(true);
      });
    }, 180);

    return () => window.clearTimeout(timer);
  }, [sessionToken, query, limit]);

  return { rows, loading, empty: searched && !loading && rows.length === 0 };
}
