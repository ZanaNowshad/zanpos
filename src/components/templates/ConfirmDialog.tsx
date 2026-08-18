import { useEffect, useRef } from "react";
import { AlertTriangle, X } from "lucide-react";
import type { ReactNode } from "react";

export type ConfirmSeverity = "danger" | "warning" | "info";

interface Props {
  open: boolean;
  title: string;
  message: string | ReactNode;
  severity?: ConfirmSeverity;
  confirmLabel?: string;
  cancelLabel?: string;
  loading?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

const SEVERITY_ICON: Record<ConfirmSeverity, ReactNode> = {
  danger: <AlertTriangle size={18} strokeWidth={1.75} aria-hidden="true" />,
  warning: <AlertTriangle size={18} strokeWidth={1.75} aria-hidden="true" />,
  info: null,
};

const SEVERITY_CLASS: Record<ConfirmSeverity, string> = {
  danger: "oa-confirm-danger",
  warning: "oa-confirm-warning",
  info: "",
};

/**
 * Lightweight confirmation dialog for generic admin actions (delete, archive, etc.).
 *
 * For AI action approval with countdown + batch previews, use the existing
 * `ConfirmActionModal` in `src/components/ConfirmActionModal.tsx`.
 */
export default function ConfirmDialog({
  open,
  title,
  message,
  severity = "danger",
  confirmLabel = "Confirm",
  cancelLabel = "Cancel",
  loading = false,
  onConfirm,
  onCancel,
}: Props) {
  const confirmRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (open) confirmRef.current?.focus();
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open, onCancel]);

  if (!open) return null;

  return (
    <div className="modal-overlay" role="presentation">
      {/* Click-outside-to-dismiss is a pointer convenience only: Escape (above)
          and the close button already provide the keyboard paths. Per the ARIA
          APG a dialog backdrop is presentational, so this stays out of the tab
          order and the accessibility tree rather than becoming a large unlabelled
          focus stop. */}
      <button
        type="button"
        className="modal-overlay-dismiss"
        onClick={onCancel}
        tabIndex={-1}
        aria-hidden="true"
      />
      <div
        className={`modal confirm-dialog ${SEVERITY_CLASS[severity]}`}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="confirm-dialog-title"
      >
        <button className="modal-close" onClick={onCancel} aria-label={cancelLabel}>
          <X size={16} />
        </button>

        <div className="confirm-dialog-body">
          {SEVERITY_ICON[severity] && (
            <div className="confirm-dialog-icon">{SEVERITY_ICON[severity]}</div>
          )}
          <h2 id="confirm-dialog-title">{title}</h2>
          <div className="confirm-dialog-message">{message}</div>
        </div>

        <div className="confirm-dialog-actions">
          <button className="btn-secondary" onClick={onCancel} disabled={loading}>
            {cancelLabel}
          </button>
          <button
            ref={confirmRef}
            className={`btn-primary${severity === "danger" ? " btn-danger" : ""}`}
            onClick={onConfirm}
            disabled={loading}
          >
            {loading ? "…" : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
