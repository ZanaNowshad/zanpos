import { useEffect, useRef, useState } from "react";

export interface OverflowAction {
  label: string;
  onClick: () => void;
  disabled?: boolean;
}

interface Props {
  actions: OverflowAction[];
  /** Accessible name for the trigger. Defaults to "More actions". */
  label?: string;
}

/**
 * The occasional actions of a page, folded behind one button.
 *
 * Products carried six buttons across the top — new, fetch images,
 * duplicates, labels, import, export — which is 580px of a 880px strip spent
 * on five things nobody does more than once a day. The strip then had no room
 * left for the filters, so they wrapped into a 148px column and pushed the
 * table down. Keeping the primary action in the open and putting the rest in
 * here costs one tap for the rare operation and buys back a third of the
 * screen for the common one.
 *
 * Closes on outside pointer-down and on Escape, and returns focus to the
 * trigger, because a menu you cannot dismiss on a touch screen with no
 * keyboard is a trap.
 */
export default function OverflowMenu({ actions, label = "More actions" }: Props) {
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => {
      if (!wrapRef.current?.contains(e.target as Node)) setOpen(false);
    };
    /* Capture phase, and the event stops here.
       The shell has its own document-level Escape handler that locks the till
       and returns to the register-handoff screen. Closing a menu must not do
       that: bubbling this through cost a whole session in testing. Capture
       runs before the shell's bubble-phase listener, so stopping propagation
       here means Escape closes the topmost thing and nothing else. */
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      setOpen(false);
      triggerRef.current?.focus();
    };
    document.addEventListener("pointerdown", onDown);
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("pointerdown", onDown);
      document.removeEventListener("keydown", onKey, true);
    };
  }, [open]);

  if (actions.length === 0) return null;

  return (
    <div className="oa-overflow" ref={wrapRef}>
      <button
        ref={triggerRef}
        type="button"
        className={`oa-tool-btn oa-overflow-trigger${open ? " oa-overflow-trigger-open" : ""}`}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={label}
        onClick={() => setOpen(v => !v)}
      >
        ⋯
      </button>
      {open && (
        <div className="oa-overflow-menu" role="menu" aria-label={label}>
          {actions.map((a, i) => (
            <button
              key={i}
              type="button"
              role="menuitem"
              className="oa-overflow-item"
              disabled={a.disabled}
              onClick={() => {
                setOpen(false);
                a.onClick();
              }}
            >
              {a.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
