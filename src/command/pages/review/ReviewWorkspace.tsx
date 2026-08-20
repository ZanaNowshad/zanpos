import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { AlertTriangle, Check, CheckCircle2, Clock, CircleSlash, Loader2 } from "lucide-react";
import type { AiActionSummary, UndoAvailability } from "../../../types";
import { DEVICE } from "../../../types";
import { aiListActions, aiUndoAction, aiUndoAvailability } from "../../../tauri/commands";
import { useLanguage } from "../../../hooks/useLanguage";
import { operationsTranslator } from "../../../i18n/operationsStrings";
import {
  ACTION_STATE, REVIEW_FILTERS, canCancel, canConfirm, displayState,
  expiresInMinutes, hasFailed, toolLabel, type ActionUiState,
} from "./actionLifecycle";
import { ConfirmDialog, DataTable, EmptyState, PageTemplate } from "../../../components/templates";
import HistoryPanel from "./HistoryPanel";
import type { Column } from "../../../components/templates";
import "./review.css";

const STATE_ICON: Record<ActionUiState, typeof CheckCircle2> = {
  prepared: Clock,
  executing: Loader2,
  executed: CheckCircle2,
  cancelled: CircleSlash,
  expired: CircleSlash,
};

interface Props {
  sessionToken: string;
  actorUserId: string;
  canApprove: boolean;
  /** Rendered inside the Conflicts section — existing, source-backed component. */
  conflictsSlot: ReactNode;
  /** Executes a persisted action by identity. Returns true on confirmed success. */
  onConfirm: (actionId: string) => Promise<boolean>;
  onCancelAction: (actionId: string) => Promise<boolean>;
}

/**
 * Review — the authoritative queue of persisted AI actions.
 *
 * Source of truth is `ai_list_actions`, not chat state: a manager opening
 * Review in a fresh session sees everything still awaiting a decision, which
 * session-local state could never show.
 */
export default function ReviewWorkspace({
  sessionToken, actorUserId, canApprove, conflictsSlot, onConfirm, onCancelAction,
}: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => operationsTranslator(language), [language]);

  /** Actions | Conflicts | History — one workspace, three sections. */
  const [section, setSection] = useState<"actions" | "conflicts" | "history">("actions");
  const [filterId, setFilterId] = useState("pending");
  const [undo, setUndo] = useState<UndoAvailability | null>(null);
  const [undoConfirm, setUndoConfirm] = useState(false);
  /** Pending row-level approval awaiting its confirm step. */
  const [approveConfirm, setApproveConfirm] = useState<AiActionSummary | null>(null);
  const [actions, setActions] = useState<AiActionSummary[]>([]);
  const [selected, setSelected] = useState<AiActionSummary | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);

  const filter = REVIEW_FILTERS.find(f => f.id === filterId) ?? REVIEW_FILTERS[0];

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const rows = await aiListActions(sessionToken, filter.statuses, 100, 0);
      setActions(rows);
      // Keep the selection pointing at fresh data, or drop it if it is gone.
      setSelected(prev => (prev ? rows.find(r => r.action_id === prev.action_id) ?? null : null));
    } catch (e) {
      setError(typeof e === "string" ? e : t("reviewLoadFailed"));
      setActions([]);
    } finally {
      setLoading(false);
    }
  }, [sessionToken, filter.statuses, t]);

  useEffect(() => { void load(); }, [load]);

  /**
   * Undo is offered only when the backend says a record exists AND is still
   * available. Anything else — no record, already used — shows nothing.
   */
  useEffect(() => {
    let cancelled = false;
    setUndo(null);
    if (!selected || displayState(selected) !== "executed") return;
    aiUndoAvailability(sessionToken, selected.action_id)
      .then(r => { if (!cancelled) setUndo(r); })
      .catch(() => { if (!cancelled) setUndo(null); });
    return () => { cancelled = true; };
  }, [selected, sessionToken]);

  /** Single-flight: the button is disabled while a decision is in flight. */
  async function decide(action: AiActionSummary, kind: "confirm" | "cancel") {
    if (busyId) return;
    setBusyId(action.action_id);
    setError(null);
    try {
      const ok = kind === "confirm"
        ? await onConfirm(action.action_id)
        : await onCancelAction(action.action_id);
      // Never assume the outcome — re-read the persisted row either way.
      await load();
      if (!ok) setError(t("actionFailed"));
    } finally {
      setBusyId(null);
    }
  }

  /**
   * Undo runs against the persisted undo record. History is never optimistically
   * updated: availability and the action row are both re-read afterwards.
   */
  async function runUndo() {
    if (!undo?.available || busyId) return;
    setUndoConfirm(false);
    setBusyId(undo.action_id);
    setError(null);
    try {
      await aiUndoAction(sessionToken, undo.undo_id, DEVICE.currency_exponent);
      const refreshed = selected
        ? await aiUndoAvailability(sessionToken, selected.action_id).catch(() => null)
        : null;
      setUndo(refreshed);
      await load();
    } catch (e) {
      setError(typeof e === "string" ? e : t("actionFailed"));
    } finally {
      setBusyId(null);
    }
  }

  const columns: Column<AiActionSummary>[] = [
    {
      id: "action",
      header: t("action"),
      cell: a => (
        <>
          <span className="zp-cell-primary">{toolLabel(a.tool_name)}</span>
          <span className="zp-cell-sub">{a.preview_text}</span>
        </>
      ),
    },
    {
      id: "status",
      header: t("status"),
      width: "150px",
      cell: a => {
        const state = displayState(a);
        const meta = ACTION_STATE[state];
        const Icon = hasFailed(a) ? AlertTriangle : STATE_ICON[state];
        const mins = expiresInMinutes(a);
        return (
          <>
            <span className={`zp-status zp-status-${hasFailed(a) ? "danger" : meta.tone}`}>
              <Icon size={13} aria-hidden="true" />
              {hasFailed(a) ? t("actionFailed") : t(meta.labelKey as never)}
            </span>
            {/* The countdown rides the status rather than sitting in its own
                column. It had priority 3, which made the most time-critical
                field on the page the first one dropped — on the till, where
                the table is narrowest, an action with three minutes left
                looked exactly like one with eleven. */}
            {mins !== null && state === "prepared" && (
              <span className={`zp-cell-sub${mins < 5 ? " zp-status-warn" : ""}`}>
                {mins <= 0 ? t("actionExpired") : `${mins} min left`}
              </span>
            )}
          </>
        );
      },
    },
    {
      id: "prepared",
      header: t("prepared"),
      width: "150px",
      priority: 2,
      cell: a => new Date(a.prepared_at).toLocaleString(),
    },
  ];

  return (
    <PageTemplate
      contentFlat
      header={{
        title: t("review"),
        secondaryActions: [{ label: t("refresh"), onClick: () => void load(), disabled: loading }],
      }}
      toolbar={
        <div className="zp-review-tabs" role="tablist" aria-label={t("review")}>
          {(["actions", "conflicts", "history"] as const).map(id => (
            <button
              key={id}
              role="tab"
              type="button"
              aria-selected={section === id}
              className={`zp-review-tab${section === id ? " is-on" : ""}`}
              onClick={() => { setSection(id); setSelected(null); }}
            >
              {t(id === "actions" ? "actions" : id === "conflicts" ? "conflicts" : "history")}
            </button>
          ))}
        </div>
      }
    >
      {section === "conflicts" && conflictsSlot}

      {section === "history" && <HistoryPanel actorUserId={actorUserId} />}

      {section === "actions" && (
        <>
          <div className="zp-toolbar">
            {REVIEW_FILTERS.map(f => (
              <button
                key={f.id}
                type="button"
                className={`zp-chip-clear${f.id === filterId ? " is-on" : ""}`}
                aria-pressed={f.id === filterId}
                onClick={() => { setFilterId(f.id); setSelected(null); }}
              >
                {t(f.labelKey as never)}
              </button>
            ))}
            <span className="zp-toolbar-spacer" />
            <span className="zp-toolbar-count" aria-live="polite">{actions.length}</span>
          </div>
      <div className={`zp-review-workspace${selected ? " has-selection" : ""}`}>
            <div>
              {loading && actions.length === 0 ? (
                <EmptyState variant="first-use" title={t("loading")} />
              ) : error ? (
                <EmptyState
                  variant="degraded"
                  title={t("reviewLoadFailed")}
                  description={error}
                  stillWorks={t("sellingUnaffectedShort")}
                  actions={[{ label: t("retry"), onClick: () => void load(), primary: true }]}
                />
              ) : actions.length === 0 ? (
                // "Nothing needs review" — not "no actions exist". History may well
                // hold executed or cancelled records.
                <EmptyState
                  variant="first-use"
                  icon={<CheckCircle2 size={32} strokeWidth={1.5} />}
                  title={filterId === "pending" ? t("nothingNeedsReview") : t("noMatchingActions")}
                  description={filterId === "pending" ? t("nothingNeedsReviewHint") : undefined}
                  actions={filterId === "pending"
                    ? undefined
                    : [{ label: t("reviewPending"), onClick: () => setFilterId("pending"), primary: true }]}
                />
              ) : (
                <DataTable
                  caption={t("review")}
                  rows={actions}
                  rowKey={a => a.action_id}
                  columns={columns}
                  onRowClick={setSelected}
                  isRowActive={a => selected?.action_id === a.action_id}
                  isRowMuted={a => ACTION_STATE[displayState(a)].terminal}
                  /* Reviewing is the whole job of this page, so the decision
                     belongs on the row. It used to require selecting an action
                     first and finding the buttons in the detail panel — three
                     interactions to approve something that expires in minutes.
                     Only offered while the action is still pending and only to
                     someone who may approve; everyone else opens the detail. */
                  rowAction={a => canConfirm(a) && canApprove ? (
                    <button
                      type="button"
                      className="btn-primary zp-row-action"
                      disabled={busyId === a.action_id}
                      onClick={() => setApproveConfirm(a)}
                      aria-label={`${t("approve")} ${toolLabel(a.tool_name)}`}
                    >
                      <span className="zp-action-label">{t("approve")}</span>
                      <Check size={14} aria-hidden="true" />
                    </button>
                  ) : null}
                />
              )}
            </div>

            {selected && (
              <aside className="zp-review-detail" aria-label={t("actionDetail")}>
                <header className="zp-review-detail-head">
                  <span className="zp-review-detail-title">{toolLabel(selected.tool_name)}</span>
                  <button type="button" className="zp-po-detail-close" onClick={() => setSelected(null)}>
                    {t("close")}
                  </button>
                </header>

                {/* The prepared description, never raw JSON. */}
                <p className="zp-review-preview">{selected.preview_text}</p>

                <dl className="zp-po-facts">
                  <div><dt>{t("status")}</dt><dd>{t(ACTION_STATE[displayState(selected)].labelKey as never)}</dd></div>
                  <div><dt>{t("prepared")}</dt><dd>{new Date(selected.prepared_at).toLocaleString()}</dd></div>
                  {selected.executed_at && (
                    <div><dt>{t("executed")}</dt><dd>{new Date(selected.executed_at).toLocaleString()}</dd></div>
                  )}
                  <div>
                    <dt>{t("expires")}</dt>
                    <dd>{expiresInMinutes(selected) === null
                      ? "—"
                      : `${Math.max(0, expiresInMinutes(selected)!)} min`}</dd>
                  </div>
                </dl>

                {selected.error_message && (
                  <div className="zp-po-exceptions" role="alert">
                    <AlertTriangle size={15} aria-hidden="true" />
                    <span>{selected.error_message}</span>
                  </div>
                )}

                {selected.result_json && (
                  <details className="zp-review-detail-tech">
                    <summary>{t("technicalDetail")}</summary>
                    <code>{selected.result_json}</code>
                  </details>
                )}

                {/* Terminal actions render read-only. */}
                {/* Undo appears only when the backend reports an available record. */}
            {undo?.available && (
              <div className="zp-po-detail-actions">
                <button
                  type="button"
                  className="oa-tool-btn"
                  disabled={!canApprove || busyId === selected.action_id}
                  onClick={() => setUndoConfirm(true)}
                >
                  {t("undoAction")} {toolLabel(selected.tool_name).toLowerCase()}
                </button>
              </div>
            )}
            {undo && !undo.available && (
              <p className="zp-review-readonly">{t("undoUsed")}</p>
            )}

            {canConfirm(selected) && (
                  <div className="zp-po-detail-actions">
                    <button
                      type="button"
                      className="oa-primary-mini"
                      disabled={!canApprove || busyId === selected.action_id}
                      title={canApprove ? undefined : t("managerApprovalOnly")}
                      onClick={() => void decide(selected, "confirm")}
                    >
                      {busyId === selected.action_id ? t("executing") : toolLabel(selected.tool_name)}
                    </button>
                    {canCancel(selected) && (
                      <button
                        type="button"
                        className="oa-tool-btn"
                        disabled={!canApprove || busyId === selected.action_id}
                        onClick={() => void decide(selected, "cancel")}
                      >
                        {t("cancel")}
                      </button>
                    )}
                  </div>
                )}
              </aside>
            )}
          </div>
        </>
      )}

      {/* The row button is one tap on a touch screen and some of these actions
          are bulk writes — "raise the selling price of 412 products" is not
          something to do by brushing a checkmark. The detail-panel button
          keeps its direct path, because getting there is already deliberate;
          this one restates what will happen first. */}
      {approveConfirm && (
        <ConfirmDialog
          open
          title={toolLabel(approveConfirm.tool_name)}
          message={approveConfirm.preview_text}
          confirmLabel={t("approve")}
          cancelLabel={t("cancel")}
          severity="warning"
          onConfirm={() => {
            const action = approveConfirm;
            setApproveConfirm(null);
            void decide(action, "confirm");
          }}
          onCancel={() => setApproveConfirm(null)}
        />
      )}

      {undoConfirm && undo && selected && (
        <ConfirmDialog
          open
          title={t("undoConfirmTitle")}
          // States what is reversed and on which record, not a bare "Are you sure?".
          message={`${t("undoWillReverse")} ${undo.entity_type} ${undo.entity_id}. ${t("undoImmediate")}`}
          confirmLabel={`${t("undoAction")} ${toolLabel(selected.tool_name).toLowerCase()}`}
          cancelLabel={t("cancel")}
          severity="warning"
          onConfirm={() => void runUndo()}
          onCancel={() => setUndoConfirm(false)}
        />
      )}
    </PageTemplate>
  );
}
