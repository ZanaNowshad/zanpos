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
    <div className="dialpad">
      {ROWS.map((row, ri) => (
        <div key={ri} className="dialpad-row">
          {row.map(k => (
            <button
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
        <button className="dialpad-key dialpad-key-wide" onMouseDown={e => { e.preventDefault(); onKey("."); }}>.</button>
        <button className="dialpad-key dialpad-key-wide" onMouseDown={e => { e.preventDefault(); onKey("00"); }}>00</button>
      </div>
    </div>
  );
}
