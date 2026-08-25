import { useState } from "react";
import { ArrowBigUp, CornerDownLeft, Delete } from "lucide-react";

interface Props {
  onKey: (key: string) => void;
  /** Moves the caret to the next field. Omitted where there is nowhere to go. */
  onNext?: () => void;
  nextLabel?: string;
}

const LATIN = [
  ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"],
  ["q", "w", "e", "r", "t", "y", "u", "i", "o", "p"],
  ["a", "s", "d", "f", "g", "h", "j", "k", "l"],
  ["z", "x", "c", "v", "b", "n", "m"],
];

/* Standard Arabic layout, digits kept Latin: a Bahraini phone number, a house
   number and a road number are all written in Latin digits here, and swapping
   them for Arabic-Indic ones would produce an address the rider cannot read
   and a number WhatsApp cannot dial. */
const ARABIC = [
  ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"],
  ["ض", "ص", "ث", "ق", "ف", "غ", "ع", "ه", "خ", "ح"],
  ["ش", "س", "ي", "ب", "ل", "ا", "ت", "ن", "م", "ك"],
  ["ئ", "ء", "ؤ", "ر", "ى", "ة", "و", "ز", "ظ", "ط"],
];

/** Characters a name or an address needs and the letter rows do not carry. */
const PUNCTUATION = ["-", "/", ".", ","];

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
 * is how "3B" becomes a chore. Arabic is a layout switch rather than a separate
 * keyboard for the same reason — half the names in the shop's book are written
 * in it, and a cashier should not have to decide which keyboard to open before
 * they have heard the name.
 */
export default function TouchKeyboard({ onKey, onNext, nextLabel = "Next" }: Props) {
  const [shift, setShift] = useState(false);
  const [script, setScript] = useState<"latin" | "arabic">("latin");
  const rows = script === "latin" ? LATIN : ARABIC;

  const press = (key: string) => {
    // Arabic is unicase; shifting it would produce the same character and leave
    // the modifier stuck on.
    const upper = script === "latin" && shift;
    onKey(upper ? key.toUpperCase() : key);
    if (shift) setShift(false);
  };

  /* Every key blocks the default on mousedown. Without it the button takes
     focus on press, the caret leaves the field these keys are meant to fill,
     and the keystroke has no target. */
  const hold = (event: { preventDefault: () => void }) => event.preventDefault();

  return (
    <div className="tkb" role="group" aria-label="Keyboard" dir={script === "arabic" ? "rtl" : "ltr"}>
      {rows.map((row, index) => (
        <div className="tkb-row" key={index}>
          {index === 3 && script === "latin" && (
            <button
              type="button"
              className={`tkb-key tkb-mod${shift ? " is-on" : ""}`}
              aria-pressed={shift}
              aria-label="Shift"
              onMouseDown={hold}
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
              onMouseDown={hold}
              onClick={() => press(key)}
            >
              {script === "latin" && shift ? key.toUpperCase() : key}
            </button>
          ))}
          {index === 3 && (
            <button
              type="button"
              className="tkb-key tkb-mod"
              aria-label="Backspace"
              onMouseDown={hold}
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
          className={`tkb-key tkb-mod tkb-script${script === "arabic" ? " is-on" : ""}`}
          aria-pressed={script === "arabic"}
          aria-label={script === "arabic" ? "Switch to English letters" : "Switch to Arabic letters"}
          onMouseDown={hold}
          onClick={() => setScript(current => (current === "latin" ? "arabic" : "latin"))}
        >
          {script === "arabic" ? "EN" : "ع"}
        </button>
        {PUNCTUATION.map(key => (
          <button type="button" key={key} className="tkb-key tkb-punct" onMouseDown={hold} onClick={() => onKey(key)}>
            {key}
          </button>
        ))}
        <button
          type="button"
          className="tkb-key tkb-space"
          aria-label="Space"
          onMouseDown={hold}
          onClick={() => onKey(" ")}
        >
          space
        </button>
        {onNext && (
          <button
            type="button"
            className="tkb-key tkb-mod tkb-next"
            aria-label={nextLabel}
            onMouseDown={hold}
            onClick={onNext}
          >
            <CornerDownLeft size={15} aria-hidden="true" />
          </button>
        )}
      </div>
    </div>
  );
}
