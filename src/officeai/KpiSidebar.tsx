import { useCallback, useEffect, useState } from "react";
import type { KpiSnapshot } from "./officeAiTypes";
import type { SessionUser, SyncStatus, TodaySummary } from "../types";
import { reportToday, inventoryGetLowStock, syncStatus } from "../tauri/commands";

export function useKpi(sessionUser: SessionUser): { kpi: KpiSnapshot; fetchKpi: () => Promise<KpiSnapshot> } {
  const [kpi, setKpi] = useState<KpiSnapshot>({
    loading: true, error: null, today: null, lowStockCount: 0, outOfStockCount: 0, sync: null,
  });

  const fetchKpi = useCallback(async (): Promise<KpiSnapshot> => {
    let today: TodaySummary | null = null;
    let lowCount = 0;
    let outCount = 0;
    let sync: SyncStatus | null = null;
    let error: string | null = null;
    try {
      today = await reportToday(sessionUser.user_id, "");
    } catch { error = "Failed to load summary"; }
    // Sequential await — WebView2 memory comment
    try {
      const low = await inventoryGetLowStock(sessionUser.user_id, "");
      lowCount = low.filter((l: { is_low_stock: boolean; is_out_of_stock: boolean }) => l.is_low_stock).length;
      outCount = low.filter((l: { is_out_of_stock: boolean }) => l.is_out_of_stock).length;
    } catch { /* non-fatal */ }
    try { sync = await syncStatus(sessionUser.user_id); } catch { /* non-fatal */ }
    const snap: KpiSnapshot = { loading: false, error, today, lowStockCount: lowCount, outOfStockCount: outCount, sync };
    setKpi(snap);
    return snap;
  }, [sessionUser.user_id]);

  useEffect(() => { fetchKpi(); }, [fetchKpi]);
  return { kpi, fetchKpi };
}

export function KpiSidebar({ kpi }: { kpi: KpiSnapshot }) {
  const today = kpi.today;
  const sync = kpi.sync;
  return (
    <div className="kpi-sidebar">
      <h4 className="kpi-title">Today</h4>
      {kpi.loading ? <div className="kpi-loading">Loading…</div> :
       kpi.error ? <div className="kpi-error">{kpi.error}</div> :
       <div className="kpi-cards">
         {today && <div className="kpi-card"><span className="kpi-val">{today.transaction_count}</span><span className="kpi-lbl">Sales</span></div>}
         {today && <div className="kpi-card"><span className="kpi-val">{today.net_total_formatted}</span><span className="kpi-lbl">Revenue</span></div>}
         <div className="kpi-card"><span className="kpi-val">{kpi.lowStockCount}</span><span className="kpi-lbl">Low Stock</span></div>
         {sync && <div className={`kpi-card ${sync.online ? "kpi-ok" : "kpi-warn"}`}><span className="kpi-val">{sync.online ? "●" : "○"}</span><span className="kpi-lbl">Sync</span></div>}
       </div>}
    </div>
  );
}
