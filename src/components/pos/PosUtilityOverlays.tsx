import type { LowStockAlert, SyncStatus } from "../../types";
import StickyNotesPanel from "../StickyNotesPanel";
import SyncConfidenceDrawer from "../SyncConfidenceDrawer";

interface Props {
  showNotes: boolean;
  showSyncDetails: boolean;
  syncStatus: SyncStatus | null;
  sessionToken: string;
  restockAlerts: LowStockAlert[];
  onCloseNotes: () => void;
  onCloseSyncDetails: () => void;
  onDismissRestockAlerts: () => void;
}

export default function PosUtilityOverlays({
  showNotes,
  showSyncDetails,
  syncStatus,
  sessionToken,
  restockAlerts,
  onCloseNotes,
  onCloseSyncDetails,
  onDismissRestockAlerts,
}: Props) {
  return (
    <>
      {showNotes && <StickyNotesPanel onClose={onCloseNotes} />}

      <SyncConfidenceDrawer
        open={showSyncDetails}
        status={syncStatus}
        sessionToken={sessionToken}
        onClose={onCloseSyncDetails}
      />

      {restockAlerts.length > 0 && (
        <div role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}  className="restock-toast-overlay" onClick={onDismissRestockAlerts}>
          {restockAlerts.map(alert => (
            <div key={alert.product_id} className="restock-toast">
              <span className="restock-toast-title">⚠ Low Stock</span>
              <span className="restock-toast-body">
                {alert.product_name}: {alert.quantity_on_hand} left (reorder at {alert.reorder_point})
              </span>
            </div>
          ))}
        </div>
      )}
    </>
  );
}
