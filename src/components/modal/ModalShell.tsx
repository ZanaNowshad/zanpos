import { useEffect, useRef, type ReactNode } from "react";
import { X } from "lucide-react";
import { useFocusTrap } from "../../hooks/useFocusTrap";
import "./modal.css";

export type ModalSize = "sm" | "md" | "lg" | "xl";

interface Props {
  /** Short uppercase context above the title — "Catalogue", "Stock". */
  kicker?: string;
  title: string;
  /** One line saying what this dialog is for, when the title cannot. */
  subtitle?: string;
  size?: ModalSize;
  onClose: () => void;
  /** Pinned below the body. Actions live here, never inline with the fields. */
  footer?: ReactNode;
  /** Extra class on the dialog. Only for dialogs the print stylesheet names. */
  className?: string;
  children: ReactNode;
}

/**
 * The one dialog shell every admin popup is built on.
 *
 * There were three conventions before this — `.modal`, `.bo-form-modal`, and a
 * couple of one-offs — so a manager moving between Products, Categories and
 * Import met three different header treatments, three close buttons and three
 * ideas about where the Save button lives. That is the visible half of the
 * problem. The invisible half was worse and is fixed here too:
 *
 * - The overlay was a `<button>` wrapping the dialog, so every control inside
 *   was a nested interactive element and Space anywhere the caret was not
 *   dismissed the form, losing whatever had been typed.
 * - Those overlays carried an `onKeyDown` that turned Enter and Space into a
 *   synthetic click on the focused element, which meant a space could not be
 *   typed into a product or category name at all.
 * - Escape closed some of them and not others.
 *
 * Structure is fixed on purpose: header, scrolling body, pinned footer. A
 * dialog whose actions scroll away with the fields is one where a manager
 * fills in a long form and cannot find Save.
 */
export default function ModalShell({
  kicker, title, subtitle, size = "md", onClose, footer, className, children,
}: Props) {
  const dialogRef = useRef<HTMLDivElement>(null);
  useFocusTrap(dialogRef, onClose);

  /* Escape, in the capture phase and with propagation stopped. Several of
     these dialogs open over the till, which binds Escape to "put the caret
     back in the barcode field" — a bubbling handler would fire second and the
     manager would lose the dialog and the caret together. */
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      onClose();
    };
    document.addEventListener("keydown", onKey, true);
    return () => document.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  return (
    <div className="modal-overlay zmodal-overlay" role="presentation">
      {/* Click-outside is a pointer convenience only. Escape and the close
          button already provide the keyboard paths, so per the ARIA APG this
          backdrop stays out of the tab order and the accessibility tree. */}
      <button
        type="button"
        className="modal-overlay-dismiss"
        onClick={onClose}
        tabIndex={-1}
        aria-hidden="true"
      />
      <div
        ref={dialogRef}
        className={`zmodal zmodal-${size}${className ? ` ${className}` : ""}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby="zmodal-title"
      >
        <header className="zmodal-head">
          <div className="zmodal-heading">
            {kicker && <span className="zmodal-kicker">{kicker}</span>}
            <h2 id="zmodal-title">{title}</h2>
            {subtitle && <p className="zmodal-sub">{subtitle}</p>}
          </div>
          <button type="button" className="zmodal-close" onClick={onClose} aria-label="Close">
            <X size={18} aria-hidden="true" />
          </button>
        </header>

        <div className="zmodal-body">{children}</div>

        {footer && <footer className="zmodal-foot">{footer}</footer>}
      </div>
    </div>
  );
}
