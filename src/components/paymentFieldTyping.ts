import { applyDialpadKeyToField } from "./Dialpad";

/**
 * Set a controlled field's value from outside React.
 *
 * React keeps its own record of the last value it wrote to an input. Assigning
 * `el.value` directly leaves that record stale, so the `input` event that
 * follows looks like a no-op and `onChange` never runs — the field would show
 * the new text and then snap back on the next render. Going through the
 * prototype setter updates the value the way React expects, so the change is
 * seen and the component state stays authoritative.
 */
function writeToField(el: HTMLInputElement | HTMLTextAreaElement, value: string): void {
  const proto = el instanceof HTMLTextAreaElement
    ? HTMLTextAreaElement.prototype
    : HTMLInputElement.prototype;
  const setter = Object.getOwnPropertyDescriptor(proto, "value")?.set;
  if (setter) setter.call(el, value);
  else el.value = value;
  el.dispatchEvent(new Event("input", { bubbles: true }));
}

/**
 * Send a dialpad key to the focused text field, if there is one.
 *
 * Returns true when it handled the key, so the caller can fall through to its
 * own virtual display fields. A focused field wins: that is what makes the
 * keypad usable for the phone and address fields, not just the cash amount.
 */
export function typeIntoFocusedField(key: string): boolean {
  const el = document.activeElement;
  if (!(el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement)) return false;

  const start = el.selectionStart ?? el.value.length;
  const end = el.selectionEnd ?? el.value.length;
  const next = applyDialpadKeyToField(el.value, start, end, key);
  if (next.value !== el.value) writeToField(el, next.value);
  // Selection APIs throw on input types that have no text cursor.
  try { el.setSelectionRange(next.caret, next.caret); } catch { /* not a text field */ }
  return true;
}
