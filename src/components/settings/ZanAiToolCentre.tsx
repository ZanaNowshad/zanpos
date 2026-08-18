import { Search, ShieldCheck, Wrench } from "lucide-react";
import { useMemo, useState } from "react";

export type ZanAiToolKind = "read" | "mutation";
export type ZanAiRisk = "low" | "medium" | "high" | "critical";

export interface ZanAiToolCentreRow {
  name: string;
  description: string;
  kind: ZanAiToolKind;
  execution: "read" | "action" | "run";
  permission: "cashier" | "manager" | "owner";
  risk: ZanAiRisk;
  confirmation: "automatic" | "risk_based" | "required";
  enabled: boolean;
  feature: string | null;
  undo: "none" | "action" | "run";
}

type RiskFilter = "all" | ZanAiRisk;

export function filterToolCentreRows(
  rows: ZanAiToolCentreRow[],
  query: string,
  risk: RiskFilter,
): ZanAiToolCentreRow[] {
  const needle = query.trim().toLocaleLowerCase();
  return rows.filter(row => {
    const matchesRisk = risk === "all" || row.risk === risk;
    const matchesQuery = !needle
      || row.name.toLocaleLowerCase().includes(needle)
      || row.description.toLocaleLowerCase().includes(needle)
      || row.permission.includes(needle)
      || row.kind.includes(needle)
      || row.execution.includes(needle);
    return matchesRisk && matchesQuery;
  });
}

interface Props {
  rows: ZanAiToolCentreRow[];
  loading: boolean;
  error?: string | null;
  savingName?: string | null;
  onEnabledChange: (row: ZanAiToolCentreRow, enabled: boolean) => void;
}

export function ZanAiToolCentre({ rows, loading, error, savingName, onEnabledChange }: Props) {
  const [query, setQuery] = useState("");
  const [risk, setRisk] = useState<RiskFilter>("all");
  const visible = useMemo(() => filterToolCentreRows(rows, query, risk), [rows, query, risk]);
  const enabled = rows.filter(row => row.enabled).length;
  const protectedCount = rows.filter(row => row.risk === "critical").length;
  const mutationCount = rows.filter(row => row.kind === "mutation").length;

  return (
    <section className="zan-tool-centre" aria-labelledby="zan-tool-centre-title">
      <header className="zan-tool-centre__header">
        <div>
          <span className="zan-tool-centre__eyebrow"><Wrench size={14} aria-hidden="true" /> Registry control</span>
          <h3 id="zan-tool-centre-title" className="settings-page-title">ZanAI Tool Centre</h3>
          <p className="settings-hint">Search every capability and control whether ZanAI may advertise or execute it.</p>
        </div>
        <div className="zan-tool-centre__counts" aria-label="Tool registry summary">
          <span><strong>{rows.length}</strong> registered</span>
          <span aria-label={`${enabled} enabled`}><strong>{enabled}</strong> enabled</span>
          <span><strong>{mutationCount}</strong> mutations</span>
          <span aria-label={`${protectedCount} protected`}><ShieldCheck size={14} aria-hidden="true" /><strong>{protectedCount}</strong> protected</span>
        </div>
      </header>

      <div className="zan-tool-centre__controls">
        <label className="zan-tool-centre__search">
          <Search size={16} aria-hidden="true" />
          <span className="sr-only">Search ZanAI tools</span>
          <input value={query} onChange={event => setQuery(event.target.value)} placeholder="Search name, purpose, or permission…" />
        </label>
        <label className="zan-tool-centre__risk-filter">
          <span>Risk</span>
          <select value={risk} onChange={event => setRisk(event.target.value as RiskFilter)}>
            <option value="all">All levels</option>
            <option value="critical">Critical</option>
            <option value="high">High</option>
            <option value="medium">Medium</option>
            <option value="low">Low</option>
          </select>
        </label>
      </div>

      {error && <div className="zan-tool-centre__error" role="alert">{error}</div>}
      {loading ? (
        <div className="zan-tool-centre__empty">Loading the generated registry…</div>
      ) : visible.length === 0 ? (
        <div className="zan-tool-centre__empty">No tools match this search.</div>
      ) : (
        <div className="zan-tool-centre__ledger">
          <div className="zan-tool-centre__column-head" aria-hidden="true">
            <span>Tool</span><span>Policy</span><span>State</span>
          </div>
          {visible.map(row => (
            <article key={row.name} className={`zan-tool-row zan-tool-row--${row.risk}`}>
              <div className="zan-tool-row__identity">
                <code>{row.name}</code>
                <p>{row.description}</p>
              </div>
              <div className="zan-tool-row__policy">
                <span className={`zan-tool-badge zan-tool-badge--${row.risk}`}>{row.risk}</span>
                <span className="zan-tool-badge">{row.permission}</span>
                <span className="zan-tool-badge">{row.kind}</span>
                <small>{row.execution} · {row.confirmation.replace("_", " ")} · undo {row.undo}</small>
              </div>
              <label className="zan-tool-row__switch">
                <span>{row.enabled ? "Enabled" : "Disabled"}</span>
                <input
                  type="checkbox"
                  checked={row.enabled}
                  disabled={savingName === row.name}
                  onChange={event => onEnabledChange(row, event.target.checked)}
                  aria-label={`${row.enabled ? "Disable" : "Enable"} ${row.name}`}
                />
              </label>
            </article>
          ))}
        </div>
      )}
    </section>
  );
}
