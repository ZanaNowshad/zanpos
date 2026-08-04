import { useEffect, useMemo, useRef, useState } from "react";
import type { BatchPendingAction, ToolPreview } from "../types";
import { useFocusTrap } from "../hooks/useFocusTrap";
import { useLanguage } from "../hooks/useLanguage";
import { modalTranslator } from "../i18n/modalStrings";

interface Props {
  preview?: ToolPreview;
  previews?: BatchPendingAction[];
  expiresAt?: string;
  onConfirm: () => void;
  onCancel: () => void;
}

function useCountdown(expiresAt?: string) {
  const [remaining, setRemaining] = useState<number | null>(null);
  useEffect(() => {
    if (!expiresAt) return;
    const tick = () => {
      const r = new Date(expiresAt).getTime() - Date.now();
      if (r <= 0) { setRemaining(0); return; }
      setRemaining(r);
    };
    tick();
    const id = setInterval(tick, 1000);
    return () => clearInterval(id);
  }, [expiresAt]);
  if (remaining === null || remaining <= 0) return null;
  const m = Math.floor(remaining / 60000);
  const s = Math.floor((remaining % 60000) / 1000);
  return `${m}m ${s}s`;
}

export default function ConfirmActionModal({ preview, previews, expiresAt, onConfirm, onCancel }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => modalTranslator(language), [language]);
  const modalRef   = useRef<HTMLDivElement>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);
  useFocusTrap(modalRef, onCancel);
  const countdown = useCountdown(expiresAt);
  const isBatch = previews && previews.length > 1;

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
    <button className="modal-overlay" type="button" onClick={onCancel}>
   <div tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }} 
        ref={modalRef}
        className="modal confirm-action-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="confirm-title"
        aria-describedby="confirm-desc"
        onClick={e => e.stopPropagation()}
      >
        {isBatch ? (
          <>
            <h2 id="confirm-title">{t("confirmChanges")} ({previews.length})</h2>
            <p id="confirm-desc" className="confirm-description">
              {t("aiChangesReview")}
            </p>
            {previews.map((action, idx) => (
              <div key={action.action_id} className="confirm-batch-item">
                <div className="confirm-batch-item-header">
                  {idx + 1}. {action.preview.description}
                </div>
                {action.preview.fields.length > 0 && (
                  <div className="confirm-fields">
                    {action.preview.fields.map((f, i) => (
                      <div key={i} className="confirm-field-row">
                        <span className="confirm-field-label">{f.label}</span>
                        <span className="confirm-field-value">{f.value}</span>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            ))}
          </>
        ) : (
          <>
            <h2 id="confirm-title">{t("confirmChange")}</h2>
            <p id="confirm-desc" className="confirm-description">
              {preview?.description ?? previews?.[0]?.preview.description}
            </p>
            <div className="confirm-fields">
              {(preview?.fields ?? previews?.[0]?.preview.fields ?? []).map((f, i) => (
                <div key={i} className="confirm-field-row">
                  <span className="confirm-field-label">{f.label}</span>
                  <span className="confirm-field-value">{f.value}</span>
                </div>
              ))}
            </div>
          </>
        )}

        <p className="confirm-warning">{t("changeLogged")}</p>
        {countdown && <p className="confirm-countdown">{t("expiresIn")} {countdown}</p>}

        <div className="modal-actions">
          <button className="btn-secondary" onClick={onCancel}>{t("cancel")}</button>
          <button ref={confirmRef} className="btn-primary" onClick={onConfirm}>
            {isBatch ? `${t("confirmAll")} ${previews.length}` : t("confirm")}
          </button>
        </div>
      </div>
    </button>
  );
}
