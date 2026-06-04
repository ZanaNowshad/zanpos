import type { ToolPreview } from "../types";

interface Props {
  preview: ToolPreview;
  onConfirm: () => void;
  onCancel: () => void;
}

export default function ConfirmActionModal({ preview, onConfirm, onCancel }: Props) {
  return (
    <div className="modal-overlay">
      <div className="modal confirm-action-modal" role="dialog" aria-modal="true" aria-labelledby="confirm-title">
        <h2 id="confirm-title">Confirm Change</h2>
        <p className="confirm-description">{preview.description}</p>

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
          <button className="btn-primary" onClick={onConfirm}>Confirm</button>
        </div>
      </div>
    </div>
  );
}
