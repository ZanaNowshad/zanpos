/**
 * The order the payment modal asks for things, and how to move between them.
 *
 * Enter used to confirm the sale from wherever the caret was. In a receipt sale
 * that is right — there is one number to type and Enter means "done". In a
 * delivery there are five fields, and Enter on the first one saved an order
 * with no address on it. So Enter advances while there is somewhere to advance
 * to, and only completes the sale from the last field.
 *
 * Selectors rather than refs because the fields are spread across three
 * components and two of them are conditional; a ref chain would have to be
 * threaded through every one of them to express an order that is really a
 * property of the dialog.
 */
export const PAYMENT_FIELD_ORDER = [
  "#payment-customer-phone",
  "#payment-house",
  "#payment-flat",
  "#payment-road",
  "#payment-rider",
] as const;

type Field = HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement;

function isVisible(el: HTMLElement): boolean {
  return el.offsetParent !== null || el.getClientRects().length > 0;
}

/** The fields this journey is actually showing, in the order they are asked. */
export function paymentFields(root: HTMLElement | null): Field[] {
  if (!root) return [];
  return PAYMENT_FIELD_ORDER
    .map(selector => root.querySelector<Field>(selector))
    .filter((el): el is Field => !!el && !el.disabled && isVisible(el));
}

/**
 * The item after `current` in a list, or null at the end of it.
 *
 * Pure and generic so the "is there anywhere left to go" decision — the one
 * that separates Enter-means-next from Enter-means-finish — can be tested
 * without a DOM. `indexOf` returning -1 for something not in the list is what
 * makes an unfocused dialog start at the first field rather than nowhere.
 */
export function nextInOrder<T>(items: T[], current: T | null): T | null {
  return items[items.indexOf(current as T) + 1] ?? null;
}

/**
 * Move the caret to the field after the one that has it.
 *
 * Returns false when there is no next field, which is the caller's signal that
 * Enter means "finish" rather than "next".
 */
export function focusNextField(root: HTMLElement | null): boolean {
  const fields = paymentFields(root);
  const next = nextInOrder(fields, document.activeElement as Field);
  if (!next) return false;
  next.focus();
  if (next instanceof HTMLInputElement) next.select();
  return true;
}

/**
 * Put the caret in the first field this journey asks for.
 *
 * The dialog used to open with focus on a payment-method button, because the
 * method picker treated mount as a selection change. Nothing looked wrong, but
 * the cashier's first keystrokes went to a radio group: typing a customer name
 * did nothing, and the arrow keys switched cash/card instead of moving through
 * the fields. On a delivery — where the first thing asked for is who the order
 * is for — that is the opening move of the sale.
 *
 * A counter sale shows none of these fields: its number goes to the dialpad,
 * which reads keystrokes from the dialog rather than from an input. So an empty
 * list means "focus nothing", which is correct rather than a failure.
 *
 * Refuses to move a caret that is already in a field inside the dialog, so it
 * can never interrupt someone who started typing first.
 */
export function focusFirstField(root: HTMLElement | null): boolean {
  if (!root) return false;
  const active = document.activeElement;
  const typing = active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement;
  if (typing && root.contains(active)) return false;
  const [first] = paymentFields(root);
  if (!first) return false;
  first.focus();
  return true;
}

/** The first shown field that is still empty, for "what is missing" prompts. */
export function firstEmptyField(root: HTMLElement | null): Field | null {
  return paymentFields(root).find(field => !field.value.trim()) ?? null;
}
