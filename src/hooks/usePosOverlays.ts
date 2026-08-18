import { useState } from "react";

/**
 * The till's secondary overlays — WhatsApp QR, sync detail, notes, alerts,
 * orders, deliveries.
 *
 * Grouped because they share one rule: none of them is a tender surface, so
 * unlike the payment modal they do not gate selling and are tracked separately
 * from `activeModal`. Keeping them out of that union is what stops an open
 * notes panel from being mistaken for a sale in progress.
 */
export function usePosOverlays() {
  const [showWaQR, setShowWaQR] = useState(false);
  const [showSyncDetails, setShowSyncDetails] = useState(false);
  const [showNotes, setShowNotes] = useState(false);
  const [showNotifications, setShowNotifications] = useState(false);
  const [showOrders, setShowOrders] = useState(false);
  const [showDeliveries, setShowDeliveries] = useState(false);

  return {
    showWaQR, setShowWaQR,
    showSyncDetails, setShowSyncDetails,
    showNotes, setShowNotes,
    showNotifications, setShowNotifications,
    showOrders, setShowOrders,
    showDeliveries, setShowDeliveries,
  };
}
