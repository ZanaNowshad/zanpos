import { chatAge, sourceLabel, type ContactSuggestion } from "./paymentContacts";

/**
 * One person as a pickable row, in the dropdown and in the directory alike.
 *
 * Two lines, not one. The first attempt put the name, the source badges, the
 * number, the points and the chat age on a single flex line with `min-width: 0`
 * on the name — so in a 300px column the name was the part that collapsed. The
 * top row rendered as `Saved · Cha36001122App 340 pts`, with the badge sitting
 * on top of the number and no name at all, which is the one piece of
 * information the cashier is actually reading.
 *
 * The name now has a floor and truncates with an ellipsis, and everything that
 * qualifies it moves to a second line where there is room for it.
 */
export default function ContactRow({ contact }: { contact: ContactSuggestion }) {
  const age = chatAge(contact.lastChatAt);
  const qualifiers = [
    contact.loyaltyPoints ? `${contact.loyaltyPoints} pts` : null,
    age,
  ].filter(Boolean).join(" · ");

  return (
    <>
      <span className="pm-sugg-line">
        <span className="pm-sugg-name">{contact.name}</span>
        <small>{contact.localPhone || "No number"}</small>
      </span>
      <span className="pm-sugg-line pm-sugg-meta">
        <em>{sourceLabel(contact.sources)}</em>
        {qualifiers && <small>{qualifiers}</small>}
      </span>
    </>
  );
}
