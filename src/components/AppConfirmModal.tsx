import { useEffect, useRef } from "react";
import { useFocusTrap } from "../hooks/useFocusTrap";
import { useLanguage } from "../hooks/useLanguage";
import { operationsTranslator } from "../i18n/operationsStrings";

interface Props {
  title: string;
  description: string;
  confirmLabel?: string;
  danger?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export default function AppConfirmModal({
  title,
  description,
  confirmLabel,
  danger = false,
  onConfirm,
  onCancel,
}: Props) {
  const { language } = useLanguage();
  const t = operationsTranslator(language);
  const modalRef = useRef<HTMLDivElement>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);
  useFocusTrap(modalRef, onCancel);

  useEffect(() => { confirmRef.current?.focus(); }, []);

  return (
    <div className="modal-overlay" onClick={onCancel}>
      <div
        ref={modalRef}
        className="modal confirm-action-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="app-confirm-title"
        aria-describedby="app-confirm-desc"
        onClick={e => e.stopPropagation()}
      >
        <h2 id="app-confirm-title">{title}</h2>
        <p id="app-confirm-desc" className="confirm-description">{description}</p>
        <p className="confirm-warning">{t("operatorWorkflowWarning")}</p>
        <div className="modal-actions">
          <button className="btn-secondary" onClick={onCancel}>{t("cancel")}</button>
          <button
            ref={confirmRef}
            className={danger ? "btn-danger" : "btn-primary"}
            onClick={onConfirm}
          >
            {confirmLabel ?? t("confirm")}
          </button>
        </div>
      </div>
    </div>
  );
}
