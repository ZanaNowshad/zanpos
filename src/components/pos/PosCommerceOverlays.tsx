import { Truck } from "lucide-react";
import type { AiHandoff, ProductPrefill, SessionUser, WaMatchedLine } from "../../types";
import DeliveriesTab from "../DeliveriesTab";
import NotificationModal from "../NotificationModal";
import WhatsAppOrdersModal from "../WhatsAppOrdersModal";
import WhatsAppQRModal from "../WhatsAppQRModal";

interface Props {
  sessionUser: SessionUser;
  showWaQr: boolean;
  showNotifications: boolean;
  showOrders: boolean;
  showDeliveries: boolean;
  onCloseWaQr: () => void;
  onCloseNotifications: () => void;
  onCloseOrders: () => void;
  onCloseDeliveries: () => void;
  onNotificationCountChange: () => void;
  onCreateProduct: (prefill: ProductPrefill) => void;
  onSendToAi: (handoff: AiHandoff) => void;
  onAddOrderLines: (lines: WaMatchedLine[]) => Promise<number>;
}

export default function PosCommerceOverlays({
  sessionUser,
  showWaQr,
  showNotifications,
  showOrders,
  showDeliveries,
  onCloseWaQr,
  onCloseNotifications,
  onCloseOrders,
  onCloseDeliveries,
  onNotificationCountChange,
  onCreateProduct,
  onSendToAi,
  onAddOrderLines,
}: Props) {
  return (
    <>
      {showWaQr && (
        <WhatsAppQRModal onClose={onCloseWaQr} sessionToken={sessionUser.session_token} />
      )}

      {showNotifications && (
        <NotificationModal
          sessionToken={sessionUser.session_token}
          onClose={onCloseNotifications}
          onCountChange={onNotificationCountChange}
          onCreateProduct={onCreateProduct}
          onSendToAi={onSendToAi}
        />
      )}

      {showOrders && (
        <WhatsAppOrdersModal
          sessionToken={sessionUser.session_token}
          onClose={onCloseOrders}
          onAddToCart={onAddOrderLines}
        />
      )}

      {showDeliveries && (
        <div className="dlv-modal-overlay">
          <div className="dlv-modal-shell" role="dialog" aria-modal="true" aria-labelledby="dlv-title">
            <div className="dlv-modal-header">
              <button className="dlv-modal-close" onClick={onCloseDeliveries}>
                <span className="icon-directional" aria-hidden="true">←</span> Close
              </button>
              <span className="dlv-modal-title" id="dlv-title">
                <Truck size={16} aria-hidden="true" /> Deliveries
              </span>
            </div>
            <div className="dlv-modal-body">
              <DeliveriesTab sessionUser={sessionUser} />
            </div>
          </div>
        </div>
      )}
    </>
  );
}
