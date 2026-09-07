import { AlertTriangle, CheckCircle2, Database, Monitor, RefreshCw, Server, ShieldAlert } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import type { HealthFinding, SessionToken, SyncDiagnostics, SystemHealthReport } from "../types";
import {
  syncDiagnostics,
  syncResetStuck,
  syncTriggerNow,
  systemHealthApplyFix,
  systemHealthCheck,
  whatsappStatus,
} from "../tauri/commands";
import AppConfirmModal from "../components/AppConfirmModal";
import { useLanguage } from "../hooks/useLanguage";
import { officeAiFormat, officeAiTranslator, type OfficeAiTranslator } from "../i18n/officeAiStrings";

interface Props {
  sessionToken: SessionToken;
  initialReport: SystemHealthReport | null;
  onReport: (report: SystemHealthReport) => void;
}

function severityRank(f: HealthFinding): number {
  return f.severity === "critical" ? 0 : f.severity === "warning" ? 1 : f.severity === "info" ? 2 : 3;
}

export function buildSystemCommandCenterModel(
  report: SystemHealthReport | null,
  sync: SyncDiagnostics | null,
  t: OfficeAiTranslator,
) {
  const findings = report?.findings ?? [];
  const criticalCount = findings.filter(f => f.severity === "critical").length;
  const warningCount = findings.filter(f => f.severity === "warning").length;
  const hubSideFindings = findings.filter(f =>
    f.fix_action === null && /run this fix on the hub terminal/i.test(f.detail),
  );
  const stuckSyncRows = sync?.stuck_events ?? report?.summary.stuck_sync_rows ?? 0;
  const pendingSyncRows = sync?.pending_events ?? report?.summary.pending_sync_rows ?? 0;
  const syncStateLabel = !sync
    ? t("unknown")
    : sync.online
      ? pendingSyncRows > 0 ? t("syncing") : t("online")
      : t("offline");

  return {
    criticalCount,
    warningCount,
    hubSideFindings,
    stuckSyncRows,
    pendingSyncRows,
    hasLocalStuckFix: stuckSyncRows > 0,
    syncStateLabel,
    topTables: [...(sync?.tables ?? report?.tables ?? [])]
      .sort((a, b) => (b.stuck + b.pending) - (a.stuck + a.pending))
      .slice(0, 6),
  };
}

export default function OfficeAISystemHealth({ sessionToken, initialReport, onReport }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => officeAiTranslator(language), [language]);
  const [report, setReport] = useState<SystemHealthReport | null>(initialReport);
  const [syncDiag, setSyncDiag] = useState<SyncDiagnostics | null>(null);
  const [loading, setLoading] = useState(false);
  const [fixing, setFixing] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [waStatus, setWaStatus] = useState<string>(() => t("notChecked"));
  const [confirm, setConfirm] = useState<{
    title: string;
    description: string;
    confirmLabel: string;
    danger?: boolean;
    run: () => Promise<void>;
  } | null>(null);

  useEffect(() => setReport(initialReport), [initialReport]);

  const run = useCallback(async () => {
    setLoading(true);
    setError(null);
    setMessage(null);
    try {
      const [next, wa] = await Promise.all([
        systemHealthCheck(sessionToken),
        whatsappStatus(sessionToken).catch(() => null),
      ]);
      setReport(next);
      onReport(next);
      setWaStatus(wa ? (wa.connected ? t("connected") : wa.qr ? t("loginNeeded") : t("waitingForSidecar")) : t("unavailable"));
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
    } finally {
      setLoading(false);
    }
  }, [sessionToken, onReport, t]);

  const loadSyncDiagnostics = useCallback(async () => {
    try {
      setSyncDiag(await syncDiagnostics(sessionToken));
    } catch {
      setSyncDiag(null);
    }
  }, [sessionToken]);

  useEffect(() => { void loadSyncDiagnostics(); }, [loadSyncDiagnostics]);

  const refreshAll = useCallback(async () => {
    await Promise.all([run(), loadSyncDiagnostics()]);
  }, [run, loadSyncDiagnostics]);

  const applyFix = (fixAction: string) => {
    setConfirm({
      title: t("applyHealthFix"),
      description: officeAiFormat(t("applySystemFix"), { action: fixAction }),
      confirmLabel: t("applyFix"),
      run: async () => {
        setFixing(fixAction);
        setError(null);
        setMessage(null);
        try {
          const result = await systemHealthApplyFix(sessionToken, fixAction);
          setMessage(result.message);
          await run();
          await loadSyncDiagnostics();
        } catch (e) {
          setError(typeof e === "string" ? e : String(e));
        } finally {
          setFixing(null);
        }
      },
    });
  };

  const triggerSync = async () => {
    setFixing("trigger_sync_now");
    setError(null);
    setMessage(null);
    try {
      setMessage(await syncTriggerNow(sessionToken));
      await Promise.all([run(), loadSyncDiagnostics()]);
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
    } finally {
      setFixing(null);
    }
  };

  const resetStuckRows = () => {
    setConfirm({
      title: t("resetStuckRows"),
      description: t("resetStuckRowsDescription"),
      confirmLabel: t("resetRows"),
      danger: true,
      run: async () => {
        setFixing("reset_stuck_sync");
        setError(null);
        setMessage(null);
        try {
          setMessage(await syncResetStuck(sessionToken));
          await Promise.all([run(), loadSyncDiagnostics()]);
        } catch (e) {
          setError(typeof e === "string" ? e : String(e));
        } finally {
          setFixing(null);
        }
      },
    });
  };

  const findings = [...(report?.findings ?? [])].sort((a, b) => severityRank(a) - severityRank(b));
  const critical = findings.filter(f => f.severity === "critical").length;
  const warning = findings.filter(f => f.severity === "warning").length;
  const consoleModel = buildSystemCommandCenterModel(report, syncDiag, t);

  return (
    <div className="oa-health">
      {confirm && (
        <AppConfirmModal
          title={confirm.title}
          description={confirm.description}
          confirmLabel={confirm.confirmLabel}
          danger={confirm.danger}
          onCancel={() => setConfirm(null)}
          onConfirm={() => {
            const action = confirm.run;
            setConfirm(null);
            void action();
          }}
        />
      )}
      <section className="oa-command-strip">
        <div className="oa-health-summary">
          <span className={`oa-health-orb ${report?.summary.ok ? "ok" : critical ? "critical" : "warning"}`} />
          <div>
            <strong>{t(report?.summary.ok ? "systemHealthy" : "attentionNeeded")}</strong>
            <span>{critical} critical · {warning} warnings · checked {report?.summary.checked_at ? report.summary.checked_at.slice(0, 16).replace("T", " ") : "not yet"}</span>
          </div>
        </div>
        <button className="oa-command-tile oa-command-primary" onClick={refreshAll} disabled={loading}>
          <RefreshCw size={20} className={loading ? "oa-spin" : ""} />
          <span><strong>{t(loading ? "checkingSystem" : "runCompleteCheck")}</strong><small>{t("systemCheckScope")}</small></span>
        </button>
      </section>

      {error && <div className="oa-inline-warning"><ShieldAlert size={16} /><span>{error}</span></div>}
      {message && <div className="oa-inline-success"><CheckCircle2 size={16} /><span>{message}</span></div>}

      <section className="oa-action-grid">
        <div className="oa-panel">
          <div className="oa-panel-header">
            <h2>{t("systemCommandCenter")}</h2>
            <span>{consoleModel.syncStateLabel}</span>
          </div>
          <div className="oa-system-actions">
            <button className="oa-command-tile" onClick={triggerSync} disabled={fixing !== null}>
              <RefreshCw size={18} className={fixing === "trigger_sync_now" ? "oa-spin" : ""} />
              <span><strong>{t("syncNow")}</strong><small>{consoleModel.pendingSyncRows} {t("pendingRows")}</small></span>
            </button>
            <button className="oa-command-tile" onClick={resetStuckRows} disabled={fixing !== null || !consoleModel.hasLocalStuckFix}>
              <ShieldAlert size={18} />
              <span><strong>{t("resetStuckSync")}</strong><small>{consoleModel.stuckSyncRows} {t("stuckRowsOnTerminal")}</small></span>
            </button>
            <button className="oa-command-tile" onClick={refreshAll} disabled={loading}>
              <Database size={18} />
              <span><strong>{t("refreshDiagnostics")}</strong><small>{consoleModel.criticalCount} {t("critical")} · {consoleModel.warningCount} {t("warnings")}</small></span>
            </button>
          </div>
          {consoleModel.hubSideFindings.length > 0 && (
            <div className="oa-inline-warning">
              <Server size={16} />
              <span>{consoleModel.hubSideFindings.length} hub-side issue(s) must be fixed from the hub terminal.</span>
            </div>
          )}
        </div>

        <div className="oa-panel">
          <div className="oa-panel-header">
            <h2>{t("syncDrilldown")}</h2>
            <span>{t(syncDiag?.last_error ? "lastErrorPresent" : "noSyncError")}</span>
          </div>
          {consoleModel.topTables.length ? (
            <div className="oa-table">
              <div className="oa-table-head"><span>{t("table")}</span><span>{t("pending")}</span><span>{t("stuck")}</span><span>{t("attempts")}</span></div>
              {consoleModel.topTables.map(t => (
                <div key={t.table} className="oa-table-row">
                  <span>{t.table}</span><span>{t.pending}</span><span>{t.stuck}</span><span>{t.max_attempts}</span>
                </div>
              ))}
            </div>
          ) : (
            <div className="oa-empty-state">{t("noPendingSyncRows")}</div>
          )}
        </div>
      </section>

      <section className="oa-metric-grid">
        <div className="oa-metric-card">
          <div className="oa-card-label"><Database size={15} /> {t("database")}</div>
          <div className="oa-status-line">{report?.summary.db_integrity ?? t("notChecked")}</div>
          <div className="oa-card-sub">{officeAiFormat(t("migrationsRecorded"), { count: report?.summary.migration_count ?? 0 })}</div>
        </div>
        <div className="oa-metric-card">
          <div className="oa-card-label"><Server size={15} /> {t("hub")}</div>
          <div className="oa-status-line">{report?.summary.hub_mode ?? "unknown"}</div>
          <div className="oa-card-sub">{officeAiFormat(t("devicesVisible"), { count: report?.summary.device_count ?? 0 })}</div>
        </div>
        <div className="oa-metric-card">
          <div className="oa-card-label"><RefreshCw size={15} /> {t("syncQueue")}</div>
          <div className="oa-status-line">{report?.summary.pending_sync_rows ?? 0} pending</div>
          <div className="oa-card-sub">{report?.summary.stuck_sync_rows ?? 0} stuck rows</div>
        </div>
        <div className="oa-metric-card">
          <div className="oa-card-label"><Monitor size={15} /> {t("whatsappSidecar")}</div>
          <div className="oa-status-line">{waStatus}</div>
          <div className="oa-card-sub">{t("checkedByHealthWorkflow")}</div>
        </div>
      </section>

      <section className="oa-action-grid">
        <div className="oa-panel">
          <div className="oa-panel-header"><h2>{t("findings")}</h2></div>
          {findings.length === 0 ? (
            <div className="oa-empty-state">{t("noFindings")}</div>
          ) : (
            <div className="oa-list">
              {findings.map(f => (
                <div key={`${f.code}-${f.title}`} className={`oa-health-row oa-sev-${f.severity}`}>
                  <AlertTriangle size={16} />
                  <span>
                    <strong>{f.area}: {f.title}</strong>
                    <small>{f.detail}</small>
                  </span>
                  {f.fix_action ? (
                    <button className="oa-primary-mini" onClick={() => applyFix(f.fix_action!)} disabled={fixing !== null}>
                      {t(fixing === f.fix_action ? "fixing" : "fix")}
                    </button>
                  ) : (
                    <em>{t("manualReview")}</em>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="oa-panel">
          <div className="oa-panel-header"><h2>{t("hubDevices")}</h2></div>
          {report?.devices.length ? (
            <div className="oa-list">
              {report.devices.map(d => (
                <div key={`${d.device_id}-${d.ip ?? ""}`} className="oa-device-row">
                  <Monitor size={16} />
                  <span>
                    <strong>{d.label}</strong>
                    <small>{d.role} · {d.status} · {d.ip ?? "no IP"} · {d.last_seen ?? "not seen"}</small>
                  </span>
                </div>
              ))}
            </div>
          ) : (
            <div className="oa-empty-state">{t("noConnectedDevices")}</div>
          )}
        </div>
      </section>

      {report?.tables.length ? (
        <section className="oa-panel">
          <div className="oa-panel-header"><h2>{t("syncTables")}</h2></div>
          <div className="oa-table">
            <div className="oa-table-head"><span>{t("table")}</span><span>{t("pending")}</span><span>{t("stuck")}</span><span>{t("maxAttempts")}</span></div>
            {report.tables.map(t => (
              <div key={t.table} className="oa-table-row">
                <span>{t.table}</span><span>{t.pending}</span><span>{t.stuck}</span><span>{t.max_attempts}</span>
              </div>
            ))}
          </div>
        </section>
      ) : null}
    </div>
  );
}
