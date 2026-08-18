// ── Shared Dialpad component + key logic ──────────────────────────────────────

/** Apply a single dialpad key press to a numeric string. */
export function applyDialpadKey(current: string, key: string): string {
  if (key === "⌫") return current.slice(0, -1);
  if (key === "C")  return "";
  if (key === ".") {
    if (current.includes(".")) return current;
    return (current || "0") + ".";
  }
  if (key === "00") {
    if (current.includes(".")) return current; // no 00 after decimal
    if (!current) return current;
    return current + "00";
  }
  // digit — don't allow leading zero except before decimal
  if (current === "0") return key;
  return current + key;
}

/**
 * Apply a dialpad key to a real text field, honouring its selection.
 *
 * The virtual display fields above take a whole string and rewrite it, but a
 * focused `<input>` has a caret and possibly a selection, and the cashier
 * expects the key to land where the caret is. Kept pure so the caret arithmetic
 * can be tested without a DOM.
 */
export function applyDialpadKeyToField(
  value: string,
  selectionStart: number,
  selectionEnd: number,
  key: string,
): { value: string; caret: number } {
  const start = Math.max(0, Math.min(selectionStart, value.length));
  const end = Math.max(start, Math.min(selectionEnd, value.length));

  if (key === "C") return { value: "", caret: 0 };
  if (key === "⌫") {
    // A selection is replaced by the delete; otherwise take the character
    // before the caret, and do nothing at the very start of the field.
    if (start !== end) return { value: value.slice(0, start) + value.slice(end), caret: start };
    if (start === 0) return { value, caret: 0 };
    return { value: value.slice(0, start - 1) + value.slice(start), caret: start - 1 };
  }
  return { value: value.slice(0, start) + key + value.slice(end), caret: start + key.length };
}

interface Props {
  onKey: (k: string) => void;
}

const ROWS = [
  ["7", "8", "9"],
  ["4", "5", "6"],
  ["1", "2", "3"],
  ["C", "0", "⌫"],
];

export default function Dialpad({ onKey }: Props) {
  return (
    <div className="dialpad" role="group" aria-label="Dialpad">
      {ROWS.map((row, ri) => (
        <div key={ri} className="dialpad-row">
          {row.map(k => (
            <button
              type="button"
              key={k}
              className={`dialpad-key${k === "⌫" ? " dialpad-key-back" : ""}${k === "C" ? " dialpad-key-clear" : ""}`}
              onMouseDown={e => { e.preventDefault(); onKey(k); }}
            >
              {k}
            </button>
          ))}
        </div>
      ))}
      <div className="dialpad-row">
        <button type="button" className="dialpad-key dialpad-key-wide" onMouseDown={e => { e.preventDefault(); onKey("."); }}>.</button>
        <button type="button" className="dialpad-key dialpad-key-wide" onMouseDown={e => { e.preventDefault(); onKey("00"); }}>00</button>
      </div>
    </div>
  );
}
