import type { LowStockAlert, SyncStatus } from "../../types";
import StickyNotesPanel from "../StickyNotesPanel";
import SyncConfidenceDrawer from "../SyncConfidenceDrawer";

interface Props {
  showNotes: boolean;
  showSyncDetails: boolean;
  syncStatus: SyncStatus | null;
  actorUserId: string;
  restockAlerts: LowStockAlert[];
  onCloseNotes: () => void;
  onCloseSyncDetails: () => void;
  onDismissRestockAlerts: () => void;
}

export default function PosUtilityOverlays({
  showNotes,
  showSyncDetails,
  syncStatus,
  actorUserId,
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
        actorUserId={actorUserId}
        onClose={onCloseSyncDetails}
      />

      {restockAlerts.length > 0 && (
        <div className="restock-toast-overlay" onClick={onDismissRestockAlerts}>
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
