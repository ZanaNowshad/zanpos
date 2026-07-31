import type {
  AiHandoff,
  LowStockAlert,
  ProductPrefill,
  SessionUser,
  WaMatchedLine,
} from "../../types";
import type { useCart } from "../../hooks/useCart";
import type { useSyncStatus } from "../../hooks/useSyncStatus";
import PosCommerceOverlays from "./PosCommerceOverlays";
import PosUtilityOverlays from "./PosUtilityOverlays";

interface Props {
  sessionUser: SessionUser;
  showWaQr: boolean;
  showNotifications: boolean;
  showOrders: boolean;
  showDeliveries: boolean;
  showNotes: boolean;
  showSyncDetails: boolean;
  syncStatus: ReturnType<typeof useSyncStatus>;
  restockAlerts: LowStockAlert[];
  lineCount: number;
  setError: ReturnType<typeof useCart>["setError"];
  addByBarcode: ReturnType<typeof useCart>["addByBarcode"];
  refreshNotifications: () => void;
  focusBarcode: () => void;
  onCloseWaQr: () => void;
  onCloseNotifications: () => void;
  onCloseOrders: () => void;
  onCloseDeliveries: () => void;
  onCloseNotes: () => void;
  onCloseSyncDetails: () => void;
  onDismissRestockAlerts: () => void;
  onOpenOfficeAI?: (prefill?: ProductPrefill) => void;
  onAskOfficeAI?: (handoff: AiHandoff) => void;
}

export default function PosSecondaryOverlays({
  sessionUser, showWaQr, showNotifications, showOrders, showDeliveries,
  showNotes, showSyncDetails, syncStatus, restockAlerts, lineCount, setError,
  addByBarcode, refreshNotifications, focusBarcode, onCloseWaQr,
  onCloseNotifications, onCloseOrders, onCloseDeliveries, onCloseNotes,
  onCloseSyncDetails, onDismissRestockAlerts, onOpenOfficeAI, onAskOfficeAI,
}: Props) {
  return (
    <>
      <PosCommerceOverlays
        sessionUser={sessionUser}
        showWaQr={showWaQr}
        showNotifications={showNotifications}
        showOrders={showOrders}
        showDeliveries={showDeliveries}
        onCloseWaQr={onCloseWaQr}
        onCloseNotifications={onCloseNotifications}
        onCloseOrders={onCloseOrders}
        onCloseDeliveries={onCloseDeliveries}
        onNotificationCountChange={refreshNotifications}
        onCreateProduct={prefill => {
          onCloseNotifications();
          if (lineCount > 0) {
            setError("Hold or complete the current sale before adding a product");
            return;
          }
          onOpenOfficeAI?.(prefill);
        }}
        onSendToAi={handoff => {
          onCloseNotifications();
          if (lineCount > 0) {
            setError("Hold or complete the current sale before opening the assistant");
            return;
          }
          onAskOfficeAI?.(handoff);
        }}
        onAddOrderLines={async (lines: WaMatchedLine[]) => {
          let added = 0;
          for (const line of lines) {
            if (!line.barcode) continue;
            try {
              await addByBarcode(line.barcode, line.quantity);
              added++;
            } catch {
              // Skip unmatched or failed lines; the cashier can keep working.
            }
          }
          if (added > 0) {
            onCloseOrders();
            focusBarcode();
          }
          return added;
        }}
      />

      <PosUtilityOverlays
        showNotes={showNotes}
        showSyncDetails={showSyncDetails}
        syncStatus={syncStatus}
        actorUserId={sessionUser.user_id}
        restockAlerts={restockAlerts}
        onCloseNotes={onCloseNotes}
        onCloseSyncDetails={onCloseSyncDetails}
        onDismissRestockAlerts={onDismissRestockAlerts}
      />
    </>
  );
}
