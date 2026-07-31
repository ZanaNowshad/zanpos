import type { ButtonHTMLAttributes, ReactNode } from "react";

export type ButtonVariant = "primary" | "secondary" | "danger" | "ghost";

interface Props extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "className"> {
  variant?: ButtonVariant;
  /** Shows a spinner in place of the label and blocks further clicks. */
  busy?: boolean;
  /** Extra classes for layout only — never for colour or size. */
  layoutClassName?: string;
  children: ReactNode;
}

/**
 * The one button.
 *
 * Replaces the `btn-*` / `sf-*` / `oa-*` triplication, where the same four
 * intentions were re-implemented three times and drifted apart.
 *
 * The busy state is built in rather than left to callers. Every async button
 * in a POS has the same failure mode: the operator taps twice because nothing
 * appeared to happen, and the second tap is a second sale, a second refund, a
 * second publish. `busy` disables the button AND keeps its width fixed, so
 * the layout does not jump under a finger already moving toward it.
 */
export default function Button({
  variant = "secondary",
  busy = false,
  layoutClassName,
  children,
  disabled,
  type = "button",
  ...rest
}: Props) {
  return (
    <button
      {...rest}
      type={type}
      // Width is locked while busy so swapping the label for a spinner cannot
      // resize the control mid-tap.
      style={busy ? { minWidth: "var(--btn-busy-width, 8ch)" } : undefined}
      className={`ui-btn ui-btn-${variant}${busy ? " is-busy" : ""}${layoutClassName ? ` ${layoutClassName}` : ""}`}
      disabled={disabled || busy}
      aria-busy={busy || undefined}
    >
      {busy ? <span className="ui-btn-spinner" aria-hidden="true" /> : children}
      {/* The label stays in the accessibility tree while busy so a screen
          reader still reports what the button does, not just "busy". */}
      {busy && <span className="ui-visually-hidden">{children}</span>}
    </button>
  );
}
