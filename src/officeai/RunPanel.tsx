import { Zap, X, RotateCcw, CheckCircle2, XCircle } from "lucide-react";
import type { RunState } from "./officeAiTypes";
import { useLanguage } from "../hooks/useLanguage";
import { officeAiFormat, officeAiTranslator } from "../i18n/officeAiStrings";

interface Props {
  runState: RunState;
  onExecute: () => void;
  onCancel: () => void;
  onUndo: () => void;
}

export default function RunPanel({ runState, onExecute, onCancel, onUndo }: Props) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  const pct =
    runState.count > 0 ? Math.round((runState.done / runState.count) * 100) : 0;

  return (
    <div className={`run-panel run-panel--${runState.phase}`}>
      <div className="run-panel-icon">
        {runState.phase === "done" && <CheckCircle2 size={20} className="run-icon-done" />}
        {runState.phase === "failed" && <XCircle size={20} className="run-icon-failed" />}
        {(runState.phase === "preview" || runState.phase === "executing") && (
          <Zap size={20} className="run-icon-active" />
        )}
      </div>

      <div className="run-panel-body">
        <div className="run-panel-desc">{runState.description}</div>

        {runState.phase === "preview" && (
          <div className="run-panel-count">
            {officeAiFormat(t("productsWillUpdate"), { count: runState.count.toLocaleString() })}
          </div>
        )}

        {runState.phase === "executing" && (
          <>
            <div className="run-progress-bar" role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100}>
              <div className="run-progress-fill" style={{ width: `${pct}%` }} />
            </div>
            <div className="run-panel-count">
              {officeAiFormat(t("doneCount"), {
                done: runState.done.toLocaleString(),
                count: runState.count.toLocaleString(),
              })}
            </div>
          </>
        )}

        {runState.phase === "done" && (
          <div className="run-panel-count">
            {officeAiFormat(t("completedRecords"), { count: runState.count.toLocaleString() })}
          </div>
        )}

        {runState.phase === "failed" && (
          <div className="run-panel-error">{runState.error}</div>
        )}
      </div>

      <div className="run-panel-actions">
        {runState.phase === "preview" && (
          <>
            <button className="run-btn run-btn--primary" onClick={onExecute}>
              <Zap size={14} />
              {officeAiFormat(t("runCount"), { count: runState.count.toLocaleString() })}
            </button>
            <button className="run-btn run-btn--ghost" onClick={onCancel}>
              <X size={14} />
              {t("cancel")}
            </button>
          </>
        )}
        {runState.phase === "done" && runState.opId === "bulk_price_adjust" && (
          <button className="run-btn run-btn--ghost" onClick={onUndo}>
            <RotateCcw size={14} />
            {t("undo")}
          </button>
        )}
      </div>
    </div>
  );
}
