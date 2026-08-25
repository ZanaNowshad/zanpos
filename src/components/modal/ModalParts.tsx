import { useId, type ReactNode } from "react";
import { TriangleAlert } from "lucide-react";

/**
 * The pieces every admin form is assembled from.
 *
 * Kept small and few on purpose. The old dialogs each invented their own field
 * markup — `.bo-form-field` here, a bare `<div>` there, centred labels in the
 * product form — so the same question looked different depending on which
 * screen asked it. One set of parts means a manager learns the shape once.
 */

/**
 * A labelled group of fields.
 *
 * The product form used to be twelve fields in one flat scroll: name, category,
 * price, cost, markup, tax, sku, barcode, extra barcodes, two checkboxes and a
 * reorder point, in that order, with nothing saying where one subject ended and
 * the next began. Grouping is most of what makes a long form readable.
 */
export function ModalSection({
  title, hint, children, columns = 1,
}: {
  title: string;
  hint?: string;
  children: ReactNode;
  /** Fields per row. Two is right for short paired values like cost and markup. */
  columns?: 1 | 2 | 3;
}) {
  return (
    <section className="zmodal-section">
      <div className="zmodal-section-head">
        <h3>{title}</h3>
        {hint && <span>{hint}</span>}
      </div>
      <div className={`zmodal-grid zmodal-grid-${columns}`}>{children}</div>
    </section>
  );
}

interface FieldProps {
  label: string;
  required?: boolean;
  /** Sits under the input, in the space the error would take. */
  hint?: string;
  error?: string | null;
  /** Makes the field span the full row in a multi-column section. */
  wide?: boolean;
  /** Receives the generated id, so the label always points at the right input. */
  children: (id: string) => ReactNode;
}

/**
 * One question and its answer.
 *
 * The id is generated rather than written by hand. The old dialogs used literal
 * `a11y-input-1`, `a11y-input-2` … which repeat across files — two dialogs open
 * at once, or one dialog rendered twice, and a `<label for>` points at whichever
 * element the document happens to reach first. `useId` makes that impossible.
 */
export function Field({ label, required, hint, error, wide, children }: FieldProps) {
  const id = useId();
  return (
    <div className={`zmodal-field${wide ? " is-wide" : ""}${error ? " is-error" : ""}`}>
      <label htmlFor={id}>
        {label}
        {required && <span className="zmodal-req" aria-hidden="true">*</span>}
      </label>
      {children(id)}
      {/* One line, always present when there is anything to say, so a field
          growing an error does not shove the rest of the form downward. */}
      {(error || hint) && (
        <small className={error ? "zmodal-field-error" : "zmodal-field-hint"}>
          {error ?? hint}
        </small>
      )}
    </div>
  );
}

/**
 * Splits a long form across the dialog's width.
 *
 * Two children, each a column. Used where a single column would make the body
 * scroll — a manager who cannot see the whole form presses Save without knowing
 * what is below the fold.
 */
export function ModalColumns({ children }: { children: ReactNode }) {
  return <div className="zmodal-cols">{children}</div>;
}

export function ModalColumn({ children }: { children: ReactNode }) {
  return <div className="zmodal-col">{children}</div>;
}

/**
 * Where a wizard is up to.
 *
 * The import dialog had three screens and no indication that there were three,
 * so a manager who dropped a file in was surprised by a preview and surprised
 * again by a result. Saying "1 of 3" up front costs one line.
 */
export function ModalSteps({ steps, current }: { steps: string[]; current: number }) {
  return (
    <ol className="zmodal-steps">
      {steps.map((label, index) => (
        <li
          key={label}
          className={index === current ? "is-current" : index < current ? "is-done" : undefined}
          aria-current={index === current ? "step" : undefined}
        >
          <span>{index + 1}</span>
          {label}
        </li>
      ))}
    </ol>
  );
}

/** A checkbox and its explanation, sized as a tap target rather than a tick. */
export function ModalToggle({
  label, hint, checked, onChange, disabled,
}: {
  label: string;
  hint?: string;
  checked: boolean;
  onChange: (next: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <label className={`zmodal-toggle${checked ? " is-on" : ""}`}>
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={event => onChange(event.target.checked)}
      />
      <span>
        <strong>{label}</strong>
        {hint && <small>{hint}</small>}
      </span>
    </label>
  );
}

/**
 * What is stopping the form being saved, above the actions where the eye
 * finishes rather than at the top where it started.
 */
export function ModalError({ message }: { message: string | null | undefined }) {
  if (!message) return null;
  return (
    <div className="zmodal-error" role="alert">
      <TriangleAlert size={15} aria-hidden="true" />
      <span>{message}</span>
    </div>
  );
}

/**
 * The footer's two halves: a note on the left, the actions on the right.
 *
 * Save is last in the DOM as well as on screen, so tabbing through a form
 * arrives at the primary action rather than passing it.
 */
export function ModalActions({
  note, children,
}: { note?: ReactNode; children: ReactNode }) {
  return (
    <>
      <div className="zmodal-foot-note">{note}</div>
      <div className="zmodal-foot-actions">{children}</div>
    </>
  );
}
