import type { KpiSnapshot } from "./officeAiTypes";
import { useLanguage } from "../hooks/useLanguage";
import { officeAiTranslator } from "../i18n/officeAiStrings";

// ─── KPI Sidebar — "Live Snapshot" beside the fullscreen Assistant ───────────

const SEVERITY_CLASS: Record<string, string> = {
  critical: "kpi-alert-critical",
  warning: "kpi-alert-warning",
  info: "kpi-alert-info",
};

export function KpiSidebar({ kpi, currencyExp, onRefresh, onDismissAlert }: {
  kpi: KpiSnapshot;
  currencyExp: number;
  onRefresh: () => void;
  onDismissAlert?: (alertId: string) => void;
}) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  const fmt = (n: number) => {
    const exp = currencyExp;
    const divisor = Math.pow(10, exp);
    return (n / divisor).toFixed(exp);
  };

  return (
    <aside className="kpi-sidebar">
      <div className="kpi-sidebar-header">
        <span className="kpi-sidebar-title">{t("liveSnapshot")}</span>
        <button className="kpi-refresh-btn" onClick={onRefresh} disabled={kpi.loading} title={t("refresh")}>
          {kpi.loading ? "⌛" : "↻"}
        </button>
      </div>

      {kpi.error && <div className="kpi-error">{kpi.error}</div>}

      {kpi.loading && (
        <>
          <div className="skeleton-card" style={{ height: 56 }} />
          <div className="skeleton-card" style={{ height: 56 }} />
          <div className="skeleton-card" style={{ height: 56 }} />
        </>
      )}

      {!kpi.loading && kpi.today && (
        <>
          <div className="kpi-section-label">Today · {kpi.today.business_date}</div>
          <div className="kpi-card">
            <div className="kpi-card-label">{t("netSales")}</div>
            <div className="kpi-card-value">BHD {fmt(kpi.today.net_total_minor)}</div>
          </div>
          <div className="kpi-card">
            <div className="kpi-card-label">{t("transactions")}</div>
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
              <div className="kpi-card-label">{t("refunds")}</div>
              <div className="kpi-card-value kpi-value-warn">
                {kpi.today.refund_count} · BHD {fmt(kpi.today.refund_total_minor)}
              </div>
            </div>
          )}
        </>
      )}

      {!kpi.loading && kpi.today === null && !kpi.error && (
        <div className="kpi-empty">{t("noDataYet")}</div>
      )}

      {!kpi.loading && (kpi.lowStockCount > 0 || kpi.outOfStockCount > 0) && (
        <div className="kpi-card kpi-card-alert">
          <div className="kpi-card-label">{t("stockAlerts")}</div>
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
            <div className="kpi-card-label">{t("sync")}</div>
            <div className="kpi-card-value">
              {state === "not_configured" && <span className={valueClass}>{t("notConfigured")}</span>}
              {state === "online" && <>☁ <span className={valueClass}>{t("online")}</span></>}
              {state === "syncing" && <>⟳ <span className={valueClass}>Syncing ({s.pending_events})</span></>}
              {state === "offline" && <>○ <span className={valueClass}>{t("offline")}</span>{s.last_error && <span className="kpi-card-warn-dot" title={s.last_error}>!</span>}</>}
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

      {kpi.alerts.length > 0 && (
        <div className="kpi-section">
          <div className="kpi-section-label">{t("proactiveAlerts")}</div>
          {kpi.alerts.map(a => (
            <div key={a.alert_id} className={`kpi-card ${SEVERITY_CLASS[a.severity] || ""}`}>
              <div className="kpi-alert-header">
                <span className="kpi-alert-severity">
                  {a.severity === "critical" ? "🔴" : a.severity === "warning" ? "🟡" : "🔵"}
                </span>
                <span className="kpi-card-label">{a.title}</span>
                {onDismissAlert && (
                  <button
                    className="kpi-alert-dismiss"
                    onClick={() => onDismissAlert(a.alert_id)}
                    title={t("dismiss")}
                  >✕</button>
                )}
              </div>
              <div className="kpi-card-sub">{a.description}</div>
            </div>
          ))}
        </div>
      )}
    </aside>
  );
}
