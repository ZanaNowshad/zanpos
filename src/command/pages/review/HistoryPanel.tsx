import { useCallback, useEffect, useMemo, useState } from "react";
import { History } from "lucide-react";
import { auditLogList } from "../../../tauri/commands";
import { useLanguage } from "../../../hooks/useLanguage";
import { operationsTranslator } from "../../../i18n/operationsStrings";
import { DataTable, EmptyState } from "../../../components/templates";
import type { Column } from "../../../components/templates";
import "./review.css";

/** Row shape returned by `audit_log_list`. Only these fields exist. */
interface AuditRow {
  audit_log_id: string;
  event_type: string;
  entity_type: string;
  entity_id: string | null;
  actor_user_id: string | null;
  created_at: string;
}

function isoDaysAgo(days: number): string {
  return new Date(Date.now() - days * 86_400_000).toISOString().slice(0, 10);
}

/**
 * History — chronological evidence, not a work queue.
 *
 * Read-only by construction: no confirmation controls appear here, because
 * `audit_log_list` returns records of things that already happened. Fields are
 * limited to what the command actually returns; actor display names are not
 * fabricated when only a user id exists.
 */
interface Props {
  actorUserId: string;
  /** Days of history to request. */
  rangeDays?: number;
}

export default function HistoryPanel({ actorUserId, rangeDays = 30 }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => operationsTranslator(language), [language]);

  const [rows, setRows] = useState<AuditRow[]>([]);
  const [selected, setSelected] = useState<AuditRow | null>(null);
  const [entityFilter, setEntityFilter] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await auditLogList(isoDaysAgo(rangeDays), isoDaysAgo(0), 1, actorUserId);
      setRows(result);
    } catch (e) {
      setError(typeof e === "string" ? e : t("historyLoadFailed"));
      setRows([]);
    } finally {
      setLoading(false);
    }
  }, [actorUserId, rangeDays, t]);

  useEffect(() => { void load(); }, [load]);

  const entityTypes = useMemo(
    () => [...new Set(rows.map(r => r.entity_type).filter(Boolean))].sort(),
    [rows],
  );
  const visible = useMemo(
    () => (entityFilter ? rows.filter(r => r.entity_type === entityFilter) : rows),
    [rows, entityFilter],
  );

  const columns: Column<AuditRow>[] = [
    {
      id: "event",
      header: t("event"),
      cell: r => (
        <>
          <span className="zp-cell-primary">{r.event_type.replace(/[_-]+/g, " ")}</span>
          <span className="zp-cell-sub">
            {r.entity_type}{r.entity_id ? ` · ${r.entity_id.slice(0, 12)}` : ""}
          </span>
        </>
      ),
    },
    {
      id: "actor",
      header: t("actor"),
      width: "160px",
      priority: 2,
      // Only a user id exists on this record; inventing a display name here
      // would be guessing at identity.
      cell: r => r.actor_user_id ?? <span className="zp-status-muted">{t("system")}</span>,
    },
    {
      id: "when",
      header: t("when"),
      width: "180px",
      cell: r => new Date(r.created_at).toLocaleString(),
    },
  ];

  return (
    <>
      <div className="zp-toolbar">
        <select
          className="zp-filter"
          value={entityFilter}
          aria-label={t("entityType")}
          onChange={e => { setEntityFilter(e.target.value); setSelected(null); }}
        >
          <option value="">{t("allEntities")}</option>
          {entityTypes.map(x => <option key={x} value={x}>{x}</option>)}
        </select>
        <span className="zp-toolbar-spacer" />
        <span className="zp-toolbar-count" aria-live="polite">{visible.length}</span>
      </div>

      <div className={`zp-review-workspace${selected ? " has-selection" : ""}`}>
        <div>
          {loading && rows.length === 0 ? (
            <EmptyState variant="first-use" title={t("loading")} />
          ) : error ? (
            <EmptyState
              variant="degraded"
              title={t("historyLoadFailed")}
              description={error}
              stillWorks={t("sellingUnaffectedShort")}
              actions={[{ label: t("retry"), onClick: () => void load(), primary: true }]}
            />
          ) : visible.length === 0 && entityFilter ? (
            <EmptyState
              variant="no-results"
              title={t("noMatchingHistory")}
              actions={[{ label: t("clearFilters"), onClick: () => setEntityFilter(""), primary: true }]}
            />
          ) : visible.length === 0 ? (
            // Distinct from "nothing needs review": this is an absence of past
            // events, not an empty work queue.
            <EmptyState
              variant="first-use"
              icon={<History size={32} strokeWidth={1.5} />}
              title={t("noHistoryYet")}
              description={t("noHistoryYetHint")}
            />
          ) : (
            <DataTable
              caption={t("history")}
              rows={visible}
              rowKey={r => r.audit_log_id}
              columns={columns}
              onRowClick={setSelected}
              isRowActive={r => selected?.audit_log_id === r.audit_log_id}
            />
          )}
        </div>

        {selected && (
          <aside className="zp-review-detail" aria-label={t("historyDetail")}>
            <header className="zp-review-detail-head">
              <span className="zp-review-detail-title">
                {selected.event_type.replace(/[_-]+/g, " ")}
              </span>
              <button type="button" className="zp-po-detail-close" onClick={() => setSelected(null)}>
                {t("close")}
              </button>
            </header>

            <dl className="zp-po-facts">
              <div><dt>{t("when")}</dt><dd>{new Date(selected.created_at).toLocaleString()}</dd></div>
              <div><dt>{t("actor")}</dt><dd>{selected.actor_user_id ?? t("system")}</dd></div>
              <div><dt>{t("entityType")}</dt><dd>{selected.entity_type}</dd></div>
              <div><dt>{t("reference")}</dt><dd>{selected.entity_id ?? "—"}</dd></div>
            </dl>

            <p className="zp-review-readonly">{t("historyReadOnly")}</p>
          </aside>
        )}
      </div>
    </>
  );
}
