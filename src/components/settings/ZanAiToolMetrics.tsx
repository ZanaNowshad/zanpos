import { Activity, Gauge, TriangleAlert } from "lucide-react";

export interface ZanAiToolMetricRow {
  tool_name: string;
  invocation_count: number;
  success_count: number;
  failure_count: number;
  average_latency_ms: number;
  last_latency_ms: number;
  estimated_tokens: number;
  last_error_at: string | null;
  updated_at: string;
}

interface Props {
  rows: ZanAiToolMetricRow[];
  loading: boolean;
  error?: string | null;
}

export function ZanAiToolMetrics({ rows, loading, error }: Props) {
  return (
    <section className="zan-tool-metrics" aria-labelledby="zan-tool-metrics-title">
      <header>
        <div>
          <span className="zan-tool-centre__eyebrow"><Activity size={14} aria-hidden="true" /> Runtime evidence</span>
          <h3 id="zan-tool-metrics-title" className="settings-page-title">Tool performance</h3>
          <p className="settings-hint">Local operational totals only. Tool inputs and business records are never captured.</p>
        </div>
      </header>
      {error ? (
        <div className="zan-tool-centre__error" role="alert">{error}</div>
      ) : loading ? (
        <div className="zan-tool-centre__empty">Loading tool telemetry…</div>
      ) : rows.length === 0 ? (
        <div className="zan-tool-centre__empty">Metrics will appear after ZanAI uses a tool.</div>
      ) : (
        <div className="zan-tool-metrics__table-wrap">
          <table className="zan-tool-metrics__table">
            <thead><tr><th>Tool</th><th>Audit volume</th><th>Average latency</th><th>Failure rate</th><th>Estimated tokens</th></tr></thead>
            <tbody>
              {rows.map(row => {
                const failureRate = row.invocation_count > 0
                  ? (row.failure_count / row.invocation_count) * 100
                  : 0;
                return (
                  <tr key={row.tool_name}>
                    <td><code>{row.tool_name}</code></td>
                    <td aria-label={`${row.invocation_count} calls`}><strong>{row.invocation_count}</strong> calls</td>
                    <td><Gauge size={14} aria-hidden="true" /> {row.average_latency_ms} ms</td>
                    <td className={failureRate > 5 ? "is-attention" : ""}>
                      {failureRate > 5 && <TriangleAlert size={14} aria-hidden="true" />}{failureRate.toFixed(1)}%
                    </td>
                    <td>{row.estimated_tokens.toLocaleString()}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
