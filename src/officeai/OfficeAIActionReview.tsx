import { AlertTriangle, CheckCircle2, RotateCcw, ShieldCheck, XCircle, Zap } from "lucide-react";
import type { OfficeAiActionQueueItem } from "./officeAiTypes";
import type { ChatController } from "./useChatController";
import { officeAiAuditTimeline } from "./officeAiData";
import { useLanguage } from "../hooks/useLanguage";
import { officeAiTranslator } from "../i18n/officeAiStrings";

interface Props {
  items: OfficeAiActionQueueItem[];
  ctrl: ChatController;
  canApprove: boolean;
}

export default function OfficeAIActionReview({ items, ctrl, canApprove }: Props) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  const timeline = officeAiAuditTimeline(ctrl, t);
  const pendingFields =
    ctrl.pendingAction?.preview.fields ??
    ctrl.pendingBatchActions?.flatMap(a => a.preview.fields) ??
    [];

  return (
    <div className="oa-action-review">
      <section className="oa-panel oa-action-hero">
        <div>
          <h2>{t("aiActionReview")}</h2>
          <p>{t("actionReviewIntro")}</p>
        </div>
        <div className="oa-action-count">
          <strong>{items.filter(i => i.status === "pending").length}</strong>
          <span>{t("pending")}</span>
        </div>
      </section>

      {!canApprove && (
        <div className="oa-inline-warning">
          <AlertTriangle size={16} />
          <span>{t("managerApprovalOnly")}</span>
        </div>
      )}

      {items.length === 0 ? (
        <div className="oa-empty-state oa-empty-large">
          <ShieldCheck size={24} />
          <strong>{t("noAiChanges")}</strong>
          <span>{t("noAiChangesHint")}</span>
        </div>
      ) : (
        <div className="oa-action-grid">
          <div className="oa-panel">
            <div className="oa-panel-header"><h2>{t("queue")}</h2></div>
            <div className="oa-list">
              {items.map(item => (
                <div key={item.id} className={`oa-action-row oa-action-${item.status}`}>
                  <div className="oa-action-icon">
                    {item.status === "pending" && <AlertTriangle size={17} />}
                    {item.status === "running" && <Zap size={17} />}
                    {item.status === "done" && <CheckCircle2 size={17} />}
                    {item.status === "failed" && <XCircle size={17} />}
                  </div>
                  <div className="oa-action-body">
                    <strong>{item.title}</strong>
                    <span>{item.detail || t("noExtraDetails")}</span>
                    {item.count != null && <small>{item.count} {t(item.count === 1 ? "item" : "items")}</small>}
                  </div>
                  <div className="oa-action-buttons">
                    {item.canConfirm && canApprove && (
                      <button className="oa-primary-mini" onClick={item.kind === "run" ? ctrl.handleRunExecute : ctrl.handleConfirm}>
                        {t("confirm")}
                      </button>
                    )}
                    {item.canCancel && (
                      <button className="oa-ghost-mini" onClick={item.kind === "run" ? ctrl.handleRunCancel : ctrl.handleCancel}>
                        {t("cancel")}
                      </button>
                    )}
                    {item.canUndo && (
                      <button
                        className="oa-ghost-mini"
                        onClick={() => item.kind === "run" ? ctrl.handleRunUndo() : ctrl.handleUndo(item.id, item.messageId ?? item.id)}
                      >
                        <RotateCcw size={13} /> {t("undo")}
                      </button>
                    )}
                  </div>
                </div>
              ))}
            </div>
          </div>

          <div className="oa-panel">
            <div className="oa-panel-header"><h2>{t("previewDetails")}</h2></div>
            {pendingFields.length > 0 ? (
              <div className="oa-field-review">
                {pendingFields.slice(0, 18).map((field, idx) => (
                  <div key={`${field.label}-${idx}`} className="oa-field-row">
                    <span>{field.label}</span>
                    <strong>{field.value}</strong>
                  </div>
                ))}
              </div>
            ) : ctrl.runState ? (
              <div className="oa-run-review">
                <strong>{ctrl.runState.description}</strong>
                <span>{ctrl.runState.done} {t("of")} {ctrl.runState.count} {t("complete")}</span>
                <div className="oa-progress"><i style={{ width: `${ctrl.runState.count ? (ctrl.runState.done / ctrl.runState.count) * 100 : 0}%` }} /></div>
              </div>
            ) : (
              <div className="oa-empty-state">{t("selectActionHint")}</div>
            )}
          </div>

          <div className="oa-panel">
            <div className="oa-panel-header"><h2>{t("auditTimeline")}</h2><span>{timeline.length} {t("events")}</span></div>
            {timeline.length ? (
              <div className="oa-list">
                {timeline.map(event => (
                  <div key={event.id} className={`oa-list-row oa-action-${event.status === "failed" ? "failed" : event.status === "proposed" ? "pending" : "done"}`}>
                    <ShieldCheck size={16} />
                    <span>
                      <strong>{event.label}</strong>
                      <small>{event.detail}</small>
                      <small>{event.status} - {event.timestamp.slice(0, 16).replace("T", " ")}</small>
                    </span>
                  </div>
                ))}
              </div>
            ) : (
              <div className="oa-empty-state">{t("noAuditEvents")}</div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
