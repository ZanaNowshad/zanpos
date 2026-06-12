import type { KpiSnapshot } from "./officeAiTypes";

// ─── KPI Sidebar — "Live Snapshot" beside the fullscreen Assistant ───────────

export function KpiSidebar({ kpi, currencyExp, onRefresh }: {
  kpi: KpiSnapshot;
  currencyExp: number;
  onRefresh: () => void;
}) {
  const fmt = (n: number) => {
    const exp = currencyExp;
    const divisor = Math.pow(10, exp);
    return (n / divisor).toFixed(exp);
  };

  return (
    <aside className="kpi-sidebar">
      <div className="kpi-sidebar-header">
        <span className="kpi-sidebar-title">Live Snapshot</span>
        <button className="kpi-refresh-btn" onClick={onRefresh} disabled={kpi.loading} title="Refresh">
          {kpi.loading ? "⌛" : "↻"}
        </button>
      </div>

      {kpi.error && <div className="kpi-error">{kpi.error}</div>}

      {kpi.today && (
        <>
          <div className="kpi-section-label">Today · {kpi.today.business_date}</div>
          <div className="kpi-card">
            <div className="kpi-card-label">Net Sales</div>
            <div className="kpi-card-value">BHD {fmt(kpi.today.net_total_minor)}</div>
          </div>
          <div className="kpi-card">
            <div className="kpi-card-label">Transactions</div>
            <div className="kpi-card-value kpi-value-neutral">{kpi.today.transaction_count}</div>
          </div>
          <div className="kpi-row-pair">
            <div className="kpi-mini-card">
              <div className="kpi-mini-label">💵 Cash</div>
              <div className="kpi-mini-value">{fmt(kpi.today.cash_total_minor)}</div>
            </div>
            <div className="kpi-mini-card">
              <div className="kpi-mini-label">💳 Card</div>
              <div className="kpi-mini-value">{fmt(kpi.today.card_total_minor)}</div>
            </div>
          </div>
          {kpi.today.refund_count > 0 && (
            <div className="kpi-card kpi-card-warn">
              <div className="kpi-card-label">Refunds</div>
              <div className="kpi-card-value kpi-value-warn">
                {kpi.today.refund_count} · BHD {fmt(kpi.today.refund_total_minor)}
              </div>
            </div>
          )}
        </>
      )}

      {!kpi.loading && kpi.today === null && !kpi.error && (
        <div className="kpi-empty">No data yet</div>
      )}

      {(kpi.lowStockCount > 0 || kpi.outOfStockCount > 0) && (
        <div className="kpi-card kpi-card-alert">
          <div className="kpi-card-label">Stock Alerts</div>
          <div className="kpi-card-value kpi-value-alert">
            {kpi.outOfStockCount > 0 && <span>❌ {kpi.outOfStockCount} out</span>}
            {kpi.lowStockCount > 0 && <span>⚠️ {kpi.lowStockCount} low</span>}
          </div>
        </div>
      )}

      {kpi.sync && (() => {
        const s = kpi.sync;
        // Four explicit states, not a binary "configured / not configured".
        // The previous render collapsed "configured but failing" into "Connected"
        // (green) which made sync outages invisible from the AI page.
        const state: "not_configured" | "online" | "syncing" | "offline" =
          !s.hub_configured ? "not_configured"
          : s.online              ? (s.pending_events > 0 ? "syncing" : "online")
          :                          "offline";

        const cardClass =
          state === "online"        ? "kpi-card-ok"
        : state === "syncing"       ? "kpi-card-info"
        : state === "offline"       ? (s.last_error ? "kpi-card-warn" : "kpi-card-neutral")
        :                              "kpi-card-neutral";

        const valueClass =
          state === "online"        ? "kpi-value-ok"
        : state === "syncing"       ? "kpi-value-info"
        :                              "kpi-value-dim";

        const titleParts: string[] = [];
        if (s.last_error) titleParts.push(`Last error: ${s.last_error}`);
        if (s.days_since_last_sync != null && s.days_since_last_sync > 0) {
          titleParts.push(`Last successful sync: ${s.days_since_last_sync}d ago`);
        }
        if (s.pending_events > 0) titleParts.push(`${s.pending_events} pending events`);

        return (
          <div className={`kpi-card ${cardClass}`} title={titleParts.join(" — ") || "Sync status"}>
            <div className="kpi-card-label">Sync</div>
            <div className="kpi-card-value">
              {state === "not_configured" && <span className={valueClass}>Not configured</span>}
              {state === "online" && <>☁ <span className={valueClass}>Online</span></>}
              {state === "syncing" && <>⟳ <span className={valueClass}>Syncing ({s.pending_events})</span></>}
              {state === "offline" && <>○ <span className={valueClass}>Offline</span>{s.last_error && <span className="kpi-card-warn-dot" title={s.last_error}>!</span>}</>}
            </div>
            {s.last_successful_sync_at && state !== "offline" && (
              <div className="kpi-card-sub">
                Last: {s.last_successful_sync_at.slice(0, 16).replace("T", " ")}
              </div>
            )}
            {state === "offline" && s.days_since_last_sync != null && s.days_since_last_sync > 0 && (
              <div className="kpi-card-sub">{s.days_since_last_sync}d since last sync</div>
            )}
            {state === "offline" && s.last_error && (
              <div className="kpi-card-sub kpi-card-sub-warn" title={s.last_error}>
                {s.last_error.length > 60 ? `${s.last_error.slice(0, 60)}…` : s.last_error}
              </div>
            )}
          </div>
        );
      })()}
    </aside>
  );
}
