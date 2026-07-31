import { AlertTriangle, DatabaseZap, RefreshCw, RotateCcw, ShieldCheck, Wrench } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import AppConfirmModal from "../components/AppConfirmModal";
import {
  syncConflictResolve,
  syncConflictsList,
  syncStockDriftReconcile,
  syncStockDriftReport,
} from "../tauri/commands";
import type { StockDriftRow, SyncConflictRow } from "../types";
import { useLanguage } from "../hooks/useLanguage";
import {
  officeAiFormat,
  officeAiTranslator,
  type OfficeAiStringKey,
} from "../i18n/officeAiStrings";

type ConflictAction = "retry" | "pull_hub_truth" | "reconcile_stock" | "dismiss";

export function conflictActionsFor(conflictType: string, tableName: string): ConflictAction[] {
  const actions: ConflictAction[] = ["retry", "pull_hub_truth"];
  if (conflictType.includes("stock") || tableName === "stock_levels" || tableName === "stock_movements") {
    actions.push("reconcile_stock");
  }
  actions.push("dismiss");
  return actions;
}

const ACTION_LABEL_KEY: Record<ConflictAction, OfficeAiStringKey> = {
  retry: "retryRow",
  pull_hub_truth: "pullHubTruth",
  reconcile_stock: "reconcileStock",
  dismiss: "dismiss",
};

interface Props {
  actorUserId: string;
}

export default function OfficeAIConflictInbox({ actorUserId }: Props) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  const [conflicts, setConflicts] = useState<SyncConflictRow[]>([]);
  const [drift, setDrift] = useState<StockDriftRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [workingId, setWorkingId] = useState<string | null>(null);
  const [filter, setFilter] = useState<"all" | "critical" | "warning">("all");
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<{
    conflict: SyncConflictRow;
    action: ConflictAction;
  } | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [nextConflicts, nextDrift] = await Promise.all([
        syncConflictsList(actorUserId),
        syncStockDriftReport(actorUserId),
      ]);
      setConflicts(nextConflicts);
      setDrift(nextDrift);
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
    } finally {
      setLoading(false);
    }
  }, [actorUserId]);

  useEffect(() => { void load(); }, [load]);

  const visible = useMemo(
    () => filter === "all" ? conflicts : conflicts.filter(item => item.severity === filter),
    [conflicts, filter],
  );

  const resolve = async (conflict: SyncConflictRow, action: ConflictAction) => {
    setWorkingId(conflict.conflict_id);
    setError(null);
    setMessage(null);
    try {
      setMessage(await syncConflictResolve(actorUserId, conflict.conflict_id, action));
      await load();
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
    } finally {
      setWorkingId(null);
    }
  };

  const reconcileAll = async () => {
    setWorkingId("stock-drift");
    setError(null);
    try {
      const count = await syncStockDriftReconcile(actorUserId);
      setMessage(officeAiFormat(t("reconciledBalances"), { count }));
      await load();
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
    } finally {
      setWorkingId(null);
    }
  };

  return (
    <div className="oa-conflicts">
      {confirm && (
        <AppConfirmModal
          title={t(ACTION_LABEL_KEY[confirm.action])}
          description={`${confirm.conflict.title}. ${confirm.conflict.detail}`}
          confirmLabel={t(ACTION_LABEL_KEY[confirm.action])}
          danger={confirm.action === "dismiss"}
          onCancel={() => setConfirm(null)}
          onConfirm={() => {
            const next = confirm;
            setConfirm(null);
            void resolve(next.conflict, next.action);
          }}
        />
      )}

      <section className="oa-workspace-lead">
        <div>
          <span className="oa-eyebrow">{t("crossDeviceControl")}</span>
          <h2>{t("conflictInbox")}</h2>
          <p>{t("conflictIntro")}</p>
        </div>
        <button className="oa-tool-btn" onClick={load} disabled={loading}>
          <RefreshCw size={15} className={loading ? "oa-spin" : ""} /> {t("refresh")}
        </button>
      </section>

      {error && <div className="oa-inline-warning"><AlertTriangle size={16} />{error}</div>}
      {message && <div className="oa-inline-success"><ShieldCheck size={16} />{message}</div>}

      <section className="oa-metric-grid oa-conflict-metrics">
        <div className="oa-metric-card"><span className="oa-card-label">{t("openConflicts")}</span><strong className="oa-big-number">{conflicts.length}</strong></div>
        <div className="oa-metric-card"><span className="oa-card-label">{t("critical")}</span><strong className="oa-big-number">{conflicts.filter(item => item.severity === "critical").length}</strong></div>
        <div className="oa-metric-card"><span className="oa-card-label">{t("stockDrift")}</span><strong className="oa-big-number">{drift.length}</strong></div>
      </section>

      {drift.length > 0 && (
        <section className="oa-panel oa-stock-drift-panel">
          <div className="oa-panel-header">
            <div><h2>{t("stockReconciliation")}</h2><span>{t("movementTruthHint")}</span></div>
            <button onClick={reconcileAll} disabled={workingId !== null}><Wrench size={14} /> {t("reconcileAll")}</button>
          </div>
          <div className="oa-table oa-conflict-table">
            <div className="oa-table-head"><span>{t("product")}</span><span>{t("terminalCache")}</span><span>{t("movementBalance")}</span><span>{t("branch")}</span></div>
            {drift.map(item => (
              <div className="oa-table-row" key={`${item.product_id}-${item.branch_id}`}>
                <span><strong>{item.product_name}</strong><small>{item.product_id}</small></span>
                <span>{item.cached_quantity}</span><span>{item.expected_quantity}</span><span>{item.branch_id}</span>
              </div>
            ))}
          </div>
        </section>
      )}

      <section className="oa-panel">
        <div className="oa-panel-header">
          <div><h2>{t("openSyncConflicts")}</h2><span>{visible.length} {t("shown")}</span></div>
          <div className="oa-segmented" role="group" aria-label={t("severityFilter")}>
            {(["all", "critical", "warning"] as const).map(value => (
              <button key={value} className={filter === value ? "active" : ""} onClick={() => setFilter(value)}>
                {t(value === "all" ? "all" : value)}
              </button>
            ))}
          </div>
        </div>
        {loading ? (
          <div className="oa-empty-state"><RefreshCw className="oa-spin" size={18} /> {t("loadingConflicts")}</div>
        ) : visible.length === 0 ? (
          <div className="oa-empty-state"><ShieldCheck size={20} /><strong>{t("noUnresolvedConflicts")}</strong><span>{t("noUnresolvedConflictsHint")}</span></div>
        ) : (
          <div className="oa-conflict-list-full">
            {visible.map(conflict => (
              <article className={`oa-conflict-item oa-sev-${conflict.severity}`} key={conflict.conflict_id}>
                <DatabaseZap size={17} />
                <div className="oa-conflict-copy">
                  <div><strong>{conflict.title}</strong><span>{conflict.table_name}{conflict.entity_id ? ` · ${conflict.entity_id}` : ""}</span></div>
                  <p>{conflict.detail}</p>
                  <small>{new Date(conflict.created_at).toLocaleString()}</small>
                </div>
                <div className="oa-action-buttons">
                  {conflictActionsFor(conflict.conflict_type, conflict.table_name).map(action => (
                    <button
                      key={action}
                      className={action === "retry" ? "oa-primary-mini" : "oa-ghost-mini"}
                      onClick={() => setConfirm({ conflict, action })}
                      disabled={workingId !== null}
                    >
                      {action === "retry" ? <RotateCcw size={13} /> : action === "reconcile_stock" ? <Wrench size={13} /> : null}
                      {t(ACTION_LABEL_KEY[action])}
                    </button>
                  ))}
                </div>
              </article>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}
