import { useEffect, useRef } from "react";
import type { ToolPreview } from "../types";
import { useFocusTrap } from "../hooks/useFocusTrap";

interface Props {
  preview: ToolPreview;
  onConfirm: () => void;
  onCancel: () => void;
}

export default function ConfirmActionModal({ preview, onConfirm, onCancel }: Props) {
  const modalRef   = useRef<HTMLDivElement>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);
  useFocusTrap(modalRef, onCancel);

  // T33: auto-focus Confirm so Enter immediately confirms; Escape cancels via overlay click.
  useEffect(() => { confirmRef.current?.focus(); }, []);

  // T33: Enter key confirms; Escape is handled by useFocusTrap calling onCancel.
  useEffect(() => {
    const handle = (e: KeyboardEvent) => {
      if (e.key === "Enter") { e.preventDefault(); onConfirm(); }
    };
    document.addEventListener("keydown", handle);
    return () => document.removeEventListener("keydown", handle);
  }, [onConfirm]);

  return (
    <div className="modal-overlay" onClick={onCancel}>
      <div
        ref={modalRef}
        className="modal confirm-action-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="confirm-title"
        aria-describedby="confirm-desc"
        onClick={e => e.stopPropagation()}
      >
        <h2 id="confirm-title">Confirm Change</h2>
        <p id="confirm-desc" className="confirm-description">{preview.description}</p>

        <div className="confirm-fields">
          {preview.fields.map((f, i) => (
            <div key={i} className="confirm-field-row">
              <span className="confirm-field-label">{f.label}</span>
              <span className="confirm-field-value">{f.value}</span>
            </div>
          ))}
        </div>

        <p className="confirm-warning">This action will be logged and can be undone afterwards.</p>

        <div className="modal-actions">
          <button className="btn-secondary" onClick={onCancel}>Cancel</button>
          <button ref={confirmRef} className="btn-primary" onClick={onConfirm}>Confirm</button>
        </div>
      </div>
    </div>
  );
}
