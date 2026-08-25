import type { CustomerRow, WaContact, WaMessage } from "../types";

/**
 * One person, however the till happens to know them.
 *
 * A cashier asking "who is this" does not care whether the answer came from the
 * customer table, the WhatsApp address book or a chat that arrived this
 * morning — they care that the name and the number are right. Three lookups
 * that each return their own list make the cashier do the reconciling, and at a
 * queue they will pick the first row rather than the correct one. So the three
 * are merged into one ranked list here, keyed on the number, and each row
 * carries where it came from so the UI can say so.
 */
export interface ContactSuggestion {
  /** Stable React key. Derived from the merge key, so it survives re-ranking. */
  key: string;
  /** What the cashier sees. The shop's own spelling wins. */
  name: string;
  /**
   * Every other name this person is known by, deduplicated.
   *
   * A customer is saved by the shop as "Ali Baqala", calls himself "Ali ⚡" on
   * WhatsApp, and the chat header says something else again. Whichever one the
   * cashier types has to find him — the till knows all three and the cashier
   * cannot be expected to know which one it stored.
   */
  altNames: string[];
  /** Local Bahrain form, 8 digits. Empty when the source had no usable number. */
  localPhone: string;
  /** E.164, or null when the number is not a valid Bahrain mobile. */
  e164: string | null;
  sources: ContactSource[];
  /** Present only when this person is in the customer table. */
  customer: CustomerRow | null;
  loyaltyPoints: number | null;
  /** Unix seconds of their most recent WhatsApp message, if any. */
  lastChatAt: number | null;
  lastChatPreview: string | null;
}

export type ContactSource = "customer" | "whatsapp" | "chat";

/** Bahrain local form: eight digits, country code and separators stripped. */
export function localPhone(raw: string | null | undefined): string {
  const digits = (raw ?? "").replace(/\D/g, "").replace(/^(00)?973/, "");
  return digits.length >= 8 ? digits.slice(-8) : digits;
}

/** E.164 for a Bahrain mobile, or null when the number cannot be one. */
export function toE164(raw: string | null | undefined): string | null {
  const local = localPhone(raw);
  return /^\d{8}$/.test(local) ? `+973${local}` : null;
}

/**
 * A WhatsApp JID carries the number: `97333050666@s.whatsapp.net`.
 * Group JIDs (`…@g.us`) are not a person and never become a receipt target.
 */
function jidPhone(jid: string): string {
  const [user = "", domain = ""] = jid.split("@");
  if (domain.startsWith("g.")) return "";
  return localPhone(user);
}

function mergeKey(name: string, phone: string): string {
  return phone ? `p:${phone}` : `n:${name.trim().toLowerCase()}`;
}

function blank(key: string, name: string, phone: string): ContactSuggestion {
  return {
    key, name, altNames: [], localPhone: phone, e164: toE164(phone), sources: [],
    customer: null, loyaltyPoints: null, lastChatAt: null, lastChatPreview: null,
  };
}

function addSource(row: ContactSuggestion, source: ContactSource): void {
  if (!row.sources.includes(source)) row.sources.push(source);
}

/** Remember a name this person answers to, without duplicating it. */
function addName(row: ContactSuggestion, name: string | null | undefined): void {
  const trimmed = (name ?? "").trim();
  if (!trimmed) return;
  const seen = [row.name, ...row.altNames].map(n => n.toLowerCase());
  if (!seen.includes(trimmed.toLowerCase())) row.altNames.push(trimmed);
}

/**
 * Fold the three directories into one list.
 *
 * A saved customer's name wins over a WhatsApp push name: the shop chose the
 * first one and the customer chose the second, and the shop's own spelling is
 * what appears on the receipt and in the ledger. Everything else is additive —
 * a chat contributes recency to a row it did not create.
 */
export function mergeContacts(
  customers: CustomerRow[],
  waContacts: WaContact[],
  waMessages: WaMessage[],
): ContactSuggestion[] {
  const byKey = new Map<string, ContactSuggestion>();
  const take = (name: string, phone: string) => {
    const key = mergeKey(name, phone);
    const existing = byKey.get(key);
    if (existing) return existing;
    const created = blank(key, name, phone);
    byKey.set(key, created);
    return created;
  };

  for (const customer of customers) {
    const row = take(customer.name, localPhone(customer.phone));
    addName(row, row.name);
    row.name = customer.name;
    row.customer = customer;
    row.loyaltyPoints = customer.loyalty_points;
    addSource(row, "customer");
  }

  for (const contact of waContacts) {
    const phone = jidPhone(contact.id);
    if (!phone) continue;
    const row = take(contact.name || phone, phone);
    if (!row.customer && contact.name) {
      addName(row, row.name);
      row.name = contact.name;
    }
    /* Every name WhatsApp knows for this person, whether or not it is the one
       being displayed. The saved name and the push name are different strings
       and the cashier may think of either. */
    addName(row, contact.name);
    addName(row, contact.savedName);
    addName(row, contact.pushName);
    addName(row, contact.verifiedName);
    addSource(row, "whatsapp");
  }

  for (const message of waMessages) {
    if (message.is_group) continue;
    const phone = jidPhone(message.chat_jid);
    if (!phone) continue;
    const row = take(message.chat_name || phone, phone);
    if (!row.customer && message.chat_name) {
      addName(row, row.name);
      row.name = message.chat_name;
    }
    addName(row, message.chat_name);
    addSource(row, "chat");
    if (row.lastChatAt === null || message.ts > row.lastChatAt) {
      row.lastChatAt = message.ts;
      row.lastChatPreview = message.body.replace(/\s+/g, " ").trim().slice(0, 60) || null;
    }
  }

  return [...byKey.values()];
}

/**
 * A name reduced to what someone is actually likely to type.
 *
 * Names in a Bahrain address book carry things nobody types at a till: emoji
 * ("Ali ⚡"), a shop tag in brackets, Arabic diacritics, and the definite
 * article on transliterations that appear both ways ("Al Osra" / "AlOsra").
 * Punctuation becomes a space rather than nothing, so "Ali-Hassan" still reads
 * as two words and matches "hassan".
 *
 * Arabic letters are kept, and the forms that vary by keyboard are folded
 * together — أ إ آ all become ا, ة becomes ه, ى becomes ي — because which one a
 * cashier types is a matter of habit, not of meaning.
 */
export function normalizeName(raw: string): string {
  return raw
    .toLowerCase()
    .normalize("NFKD")
    // Latin accents and Arabic harakat, both of which are combining marks.
    .replace(/[̀-ًͯ-ٰٟ]/g, "")
    .replace(/[أإآ]/g, "ا")
    .replace(/ة/g, "ه")
    .replace(/[ىئ]/g, "ي")
    .replace(/ؤ/g, "و")
    // Anything that is not a letter, an Arabic letter or a digit separates words.
    .replace(/[^\p{L}\p{N}]+/gu, " ")
    .trim();
}

/**
 * How well one name answers what was typed.
 *
 * The tiers below descend from certainty to plausibility, and the two that
 * matter most at a counter are the last two. A cashier types the words they
 * remember, in whatever order they surface — "baqala ali" for "Ali Baqala" —
 * and they type the middle of a word as often as the start when the start is a
 * title they have forgotten ("Bu", "Al", "Hajji"). Requiring the query to be a
 * prefix of the whole string fails both, and the cashier concludes the customer
 * is not saved and enters them again.
 */
function scoreName(name: string, query: string): number {
  const target = normalizeName(name);
  if (!target) return 0;
  if (target === query) return 95;
  if (target.startsWith(query)) return 75;

  const words = target.split(" ").filter(Boolean);
  // "ali" finds "Layla Bu Ali" as well as "Ali Hassan".
  if (words.some(word => word.startsWith(query))) return 65;
  if (target.includes(query)) return 50;

  // Every typed word appears somewhere, in any order: "baqala ali" → "Ali Baqala".
  const typed = query.split(" ").filter(Boolean);
  if (typed.length > 1 && typed.every(part => words.some(word => word.startsWith(part)))) {
    return 60;
  }
  // A single typed word inside a longer one: "sra" → "Al Osra".
  if (typed.length === 1 && words.some(word => word.includes(query))) return 40;
  return 0;
}

/**
 * How well a row answers what was typed, as a score. Higher is better;
 * zero means "do not offer this".
 *
 * The query is deliberately not classified as "a phone" or "a name" up front.
 * A cashier types `3605` meaning a number and `Fat` meaning a name, but they
 * also type `Fatima 36` — and a field that decides which mode it is in will get
 * that wrong. Scoring both interpretations and keeping the better one costs
 * nothing and never has to guess.
 */
export function scoreContact(row: ContactSuggestion, query: string): number {
  const q = query.trim().toLowerCase();
  if (!q) return 1;

  const digits = q.replace(/\D/g, "");
  const letters = normalizeName(q);
  let score = 0;

  if (digits && row.localPhone) {
    if (row.localPhone === digits) score = Math.max(score, 100);
    else if (row.localPhone.startsWith(digits)) score = Math.max(score, 80);
    else if (row.localPhone.includes(digits)) score = Math.max(score, 55);
  }
  if (letters) {
    /* Against every name this person answers to, not only the displayed one.
       An alternate name scores slightly lower so that when two people match,
       the one matched on their real display name is offered first. */
    score = Math.max(score, scoreName(row.name, letters));
    for (const alt of row.altNames) {
      score = Math.max(score, scoreName(alt, letters) - 4);
    }
  }
  if (score === 0) return 0;

  /* Tie-breaks, smallest first so they can never outrank a real match:
     a saved customer over a stranger, then someone who messaged recently. */
  if (row.customer) score += 6;
  if (row.sources.includes("chat")) score += 3;
  if (row.e164) score += 2;
  return score;
}

/** The ranked shortlist for a query. Ties break on name so the order is stable. */
export function rankContacts(
  rows: ContactSuggestion[], query: string, limit = 6,
): ContactSuggestion[] {
  return rows
    .map(row => ({ row, score: scoreContact(row, query) }))
    .filter(entry => entry.score > 0)
    .sort((a, b) => b.score - a.score || a.row.name.localeCompare(b.row.name))
    .slice(0, limit)
    .map(entry => entry.row);
}

/** "Saved · WhatsApp", for the badge under a suggestion. */
export function sourceLabel(sources: ContactSource[]): string {
  const names: Record<ContactSource, string> = {
    customer: "Saved", whatsapp: "WhatsApp", chat: "Chatted",
  };
  const order: ContactSource[] = ["customer", "chat", "whatsapp"];
  return order.filter(s => sources.includes(s)).map(s => names[s]).join(" · ");
}

/** "2h ago" / "3d ago" — how long since they last messaged. */
export function chatAge(lastChatAt: number | null, now = Date.now()): string | null {
  if (!lastChatAt) return null;
  const minutes = Math.max(0, Math.round((now / 1000 - lastChatAt) / 60));
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}
