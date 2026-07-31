import { useMemo } from "react";
import type { Dispatch, SetStateAction } from "react";
import type { ActiveModal } from "../components/pos/posModalState";
import type { useCart } from "./useCart";
import { usePosShortcuts } from "./usePosShortcuts";

interface Options {
  noModalOpen: boolean;
  lineCount: number;
  recentLineId: string | null;
  lastReceiptNumber: string | null;
  canRefund: boolean;
  canViewXReport: boolean;
  setActiveModal: Dispatch<SetStateAction<ActiveModal>>;
  focusBarcode: () => void;
  openHold: () => void;
  openPay: () => void;
  payFast: () => void;
  clearCart: () => void;
  reprintLast: () => void;
  noSale: () => void;
  bumpRecentQty: ReturnType<typeof useCart>["bumpRecentQty"];
  removeRecentLine: ReturnType<typeof useCart>["removeRecentLine"];
  onLock?: () => void;
  onLogout: () => void;
}

export function usePosShortcutBindings({
  noModalOpen, lineCount, recentLineId, lastReceiptNumber, canRefund,
  canViewXReport, setActiveModal, focusBarcode, openHold, openPay, payFast,
  clearCart, reprintLast, noSale, bumpRecentQty, removeRecentLine, onLock, onLogout,
}: Options) {
  const handlers = useMemo(() => ({
    noModalOpen,
    lineCount,
    hasRecentLine: recentLineId !== null,
    lastReceiptNumber,
    onFocusBarcode: focusBarcode,
    onHold: openHold,
    onResumeHeld: openHold,
    onPay: openPay,
    onPayFast: payFast,
    onDiscount: () => setActiveModal({ kind: "discount" }),
    onLineDiscount: () => {
      if (recentLineId) setActiveModal({ kind: "lineDiscount", lineId: recentLineId });
    },
    onRefund: () => canRefund && setActiveModal({ kind: "refund" }),
    onClearCart: clearCart,
    onReprintLast: reprintLast,
    onNoSale: noSale,
    onXReport: canViewXReport
      ? () => setActiveModal({ kind: "xReport" as const })
      : undefined,
    onIncrementRecent: () => bumpRecentQty(1),
    onDecrementRecent: () => bumpRecentQty(-1),
    onRemoveRecent: removeRecentLine,
    onLock: onLock ?? onLogout,
    onReport: () => setActiveModal({ kind: "report" }),
    onCustomItem: () => setActiveModal({ kind: "customItem" }),
    onHelp: () => setActiveModal({ kind: "help" }),
  }), [
    bumpRecentQty, canRefund, canViewXReport, clearCart, focusBarcode,
    lastReceiptNumber, lineCount, noModalOpen, noSale, onLock, onLogout,
    openHold, openPay, payFast, recentLineId, removeRecentLine, reprintLast,
    setActiveModal,
  ]);

  usePosShortcuts(handlers);
}
