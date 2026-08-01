import { useEffect, useRef } from "react";
import { useLanguage } from "../hooks/useLanguage";
import { operationsTranslator } from "../i18n/operationsStrings";
import { Dialog, DialogContent, DialogClose } from "./ui";

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
  const confirmRef = useRef<HTMLButtonElement>(null);

  useEffect(() => { confirmRef.current?.focus(); }, []);

  return (
    <Dialog open onOpenChange={(open) => { if (!open) onCancel(); }}>
      <DialogContent
        title={title}
        onEscapeKeyDown={onCancel}
        onPointerDownOutside={onCancel}
      >
        <p className="confirm-description">{description}</p>
        <p className="confirm-warning">{t("operatorWorkflowWarning")}</p>
        <div className="modal-actions">
          <DialogClose onClick={onCancel}>
            <button className="btn-secondary" type="button">{t("cancel")}</button>
          </DialogClose>
          <button
            ref={confirmRef}
            className={danger ? "btn-danger" : "btn-primary"}
            onClick={onConfirm}
          >
            {confirmLabel ?? t("confirm")}
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
