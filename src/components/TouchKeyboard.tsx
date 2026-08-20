import { useState } from "react";
import { ArrowBigUp, Delete, Space } from "lucide-react";

interface Props {
  onKey: (key: string) => void;
}

const ROWS = [
  ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"],
  ["q", "w", "e", "r", "t", "y", "u", "i", "o", "p"],
  ["a", "s", "d", "f", "g", "h", "j", "k", "l"],
  ["z", "x", "c", "v", "b", "n", "m"],
];

/**
 * On-screen letters for the fields the dialpad cannot fill.
 *
 * A customer name, an area, a road, a flat like `3B` — none of those are
 * numbers, and the till has no keyboard plugged into it. The OS keyboard button
 * still exists for anyone who prefers it, but raising Windows' own keyboard
 * covers the modal and steals focus, so the common case is served here instead.
 *
 * Digits share the top row rather than living behind a mode switch: a flat
 * number is usually digits with one letter, and making that a two-step toggle
 * is how "3B" becomes a chore.
 */
export default function TouchKeyboard({ onKey }: Props) {
  const [shift, setShift] = useState(false);

  const press = (key: string) => {
    onKey(shift ? key.toUpperCase() : key);
    if (shift) setShift(false);
  };

  return (
    <div className="tkb" role="group" aria-label="Keyboard">
      {ROWS.map((row, index) => (
        <div className="tkb-row" key={index}>
          {index === 3 && (
            <button
              type="button"
              className={`tkb-key tkb-mod${shift ? " is-on" : ""}`}
              aria-pressed={shift}
              aria-label="Shift"
              /* Same rule as the dialpad: taking focus would move the caret out
                 of the field these keys are meant to fill. */
              onMouseDown={event => event.preventDefault()}
              onClick={() => setShift(current => !current)}
            >
              <ArrowBigUp size={16} aria-hidden="true" />
            </button>
          )}
          {row.map(key => (
            <button
              type="button"
              key={key}
              className="tkb-key"
              onMouseDown={event => event.preventDefault()}
              onClick={() => press(key)}
            >
              {shift ? key.toUpperCase() : key}
            </button>
          ))}
          {index === 3 && (
            <button
              type="button"
              className="tkb-key tkb-mod"
              aria-label="Backspace"
              onMouseDown={event => event.preventDefault()}
              onClick={() => onKey("⌫")}
            >
              <Delete size={16} aria-hidden="true" />
            </button>
          )}
        </div>
      ))}
      <div className="tkb-row">
        <button
          type="button"
          className="tkb-key tkb-space"
          aria-label="Space"
          onMouseDown={event => event.preventDefault()}
          onClick={() => onKey(" ")}
        >
          <Space size={16} aria-hidden="true" />
        </button>
      </div>
    </div>
  );
}
