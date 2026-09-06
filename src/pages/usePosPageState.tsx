import { useCallback, useEffect, useMemo, useState } from "react";
import type { SaleResult } from "../types";
import { DEVICE } from "../types";
import type { ActiveModal, ExchangeCredit } from "../components/pos/posModalState";

import { useLanguage } from "../hooks/useLanguage";
import { posTranslator } from "../i18n/posStrings";
import { useCart } from "../hooks/useCart";
import { useSyncStatus } from "../hooks/useSyncStatus";
import { usePosAlerts } from "../hooks/usePosAlerts";
import { useClockTime } from "../hooks/useClockTime";
import { usePersistedToggle } from "../hooks/usePersistedToggle";
import { usePosReceipt } from "../hooks/usePosReceipt";
import { usePosCartRecovery } from "../hooks/usePosCartRecovery";
import { usePosBarcode } from "../hooks/usePosBarcode";
import { useWhatsAppHealth } from "../hooks/useWhatsAppHealth";
import { usePosRecoveryActions } from "../hooks/usePosRecoveryActions";
import { usePosPaymentActions } from "../hooks/usePosPaymentActions";
import { usePosRegisterActions } from "../hooks/usePosRegisterActions";
import { usePosConfiguration } from "../hooks/usePosConfiguration";
import { useCustomItemSuggestions } from "../hooks/useCustomItemSuggestions";
import { usePosZanAiContext } from "../hooks/usePosZanAiContext";
import { useQuickPosSlots } from "../hooks/useQuickPosSlots";
import { } from "./posLayoutClass";
import { usePosOverlays } from "../hooks/usePosOverlays";
import type { PosPageProps } from "./posPageProps";
import { usePracticeMode } from "./usePracticeMode";
import { usePosSelection } from "./usePosSelection";
import { usePosLineShortcuts } from "./usePosLineShortcuts";
import { usePosShortcutBindings } from "../hooks/usePosShortcutBindings";
import { type ReceiptConfidenceStatus } from "../utils/posConfidence";
import { getExchangeBalance } from "../utils/posExchange";
import "../components/pos/till.css";



/**
 * Everything the till knows, assembled once.
 *
 * PosPage was a single 586-line component: every hook, every handler and the
 * whole render in one file, past the 500-line limit the ship gate enforces.
 * Splitting it by prop-drilling the modal layer was not possible — the four
 * modal groups alone take 53 distinct props — so the cut is the one that
 * actually holds: state and behaviour here, presentation in PosPageView.
 *
 * Returns one object rather than a tuple so the view can spread it; TypeScript
 * then catches anything the render reads that this forgot to hand over.
 */
export function usePosPageState({
  sessionUser, shift, onLogout, onLock, onShiftClose, onOpenOfficeAI, onAskOfficeAI, theme, onToggleTheme
}: PosPageProps) {
  const session = useMemo(() => ({
    branch_id: DEVICE.branch_id,
    device_id: DEVICE.device_id,
    shift_id: shift.shift_id,
    cashier_user_id: sessionUser.user_id,
    session_token: sessionUser.session_token
  }), [shift.shift_id, sessionUser.user_id, sessionUser.session_token]);

  const [showSidebar, toggleSidebar] = usePersistedToggle("zanpos_sidebar");

  const [activeModal, setActiveModal] = useState<ActiveModal>({ kind: "none" });
  const [bannerResult, setBannerResult] = useState<SaleResult | null>(null);
  const [showSaleDetails, setShowSaleDetails] = useState(false);
  const [lastReceiptNumber, setLastReceiptNumber] = useState<string | null>(null);
  const [receiptStatus, setReceiptStatus] = useState<ReceiptConfidenceStatus>("not_ready");
  const [exchangeCredit, setExchangeCredit] = useState<ExchangeCredit | null>(null);
  /* The line the contextual action strip acts on. Follows the scan by default —
     the item just added is nearly always the one being corrected — but a tap on
     any row takes over until the next scan. */
  const [pickedLineId, setPickedLineId] = useState<string | null>(null);
  const [showMore, setShowMore] = useState(false);
  const [qtyPadLineId, setQtyPadLineId] = useState<string | null>(null);
  const {
    showWaQR, setShowWaQR, showSyncDetails, setShowSyncDetails,
    showNotes, setShowNotes, showNotifications, setShowNotifications,
    showOrders, setShowOrders, showDeliveries, setShowDeliveries
  } = usePosOverlays();

  const {
    businessFlags: bizFlags,
    setBusinessFlags: setBizFlags,
    commerceEnabled
  } = usePosConfiguration(sessionUser.user_id);
  const { suggestions, refreshSuggestions } = useCustomItemSuggestions();
  const { slots: quickSlots } = useQuickPosSlots(sessionUser.user_id);

  const canOpenBackOffice = ["owner", "manager"].includes(sessionUser.role_name);
  const canViewXReport    = canOpenBackOffice;
  const canRefund         = ["owner", "manager", "cashier"].includes(sessionUser.role_name);

  // Sidebar badges: alerts bell + unfulfilled orders, one shared 8s poll.
  const { notifCount, orderCount, refreshNotifications } = usePosAlerts({
    userId: sessionUser.user_id,
    canOpenBackOffice,
    commerceEnabled
  });

  // M21: App.tsx already owns an idle timer (App-level, 60s warning → logout).
  // PosPage previously ran a second independent idle timer that called onLogout
  // directly, bypassing the warning. Removed here — App's timer handles logout.

  const syncStatus = useSyncStatus(15_000, sessionUser.session_token);

  // Till strings. `dir` is applied to <html> by the hook, so RTL flips at the
  // layout level rather than being re-implemented per component.
  const { language, toggle: toggleLanguage } = useLanguage();
  const t = useMemo(() => posTranslator(language), [language]);

  // ── Training mode ───────────────────────────────────────────────────────────
  // A rehearsal till for a cashier's first day. Sales built here are never sent
  // to the backend, so they cannot reach reports, EOD, stock or the receipt
  // sequence — a structural guarantee rather than a filter every future report
  // query has to remember. Everything else, including the real print, is live.
  const { trainingMode, setTrainingMode, buildTrainingResult } = usePracticeMode(sessionUser);

  const {
    cart, loading, error, clearError, setError,
    recentLineId,
    addByBarcode, addProduct, addProductById, addCustomItem,
    updateQuantity, removeLine, removeRecentLine, bumpRecentQty, bumpLine,
    applyBillDiscount, applyLineDiscount, setLinePrice,
    finalizeSale, clearCart, replaceCart,
    netTotal, taxTotal, lineCount
  } = useCart(session, buildTrainingResult);

  const { selectedLineId, selectedLine, qtyPadLine, numpadValue, setNumpadValue, handleNumpadKey } =
    usePosSelection(cart, pickedLineId, setPickedLineId, recentLineId, qtyPadLineId, updateQuantity);


  const exchangeBalance = useMemo(
    () => exchangeCredit ? getExchangeBalance(exchangeCredit.creditMinor, netTotal) : null,
    [exchangeCredit, netTotal],
  );
  const payableTotal = exchangeBalance ? exchangeBalance.amountDueMinor : netTotal;
  // ── Barcode input ref for programmatic focus ──────────────────────────────────
  const { barcodeRef, focusBarcode, handleBarcode } = usePosBarcode({
    userId: sessionUser.user_id,
    numpadValue,
    addByBarcode,
    setError,
    resetNumpad: () => setNumpadValue("1"),
    refreshNotifications
  });

  // Recover a cart lost to a power cut. Training carts are excluded: a
  // rehearsal must never come back as a real sale after a restart.
  // WhatsApp liveness for the header pill. Polled on its own slow cadence —
  // the sidecar check is a network call and must not ride the 8s badge poll
  // that already runs on every till.
  const { connected: waConnected, stale: waStale } =
    useWhatsAppHealth(commerceEnabled, sessionUser.user_id);

  const { recoverable, dismissRecovery, recoverCart } = usePosCartRecovery({
    cart,
    trainingMode,
    shiftId: shift.shift_id,
    userId: sessionUser.user_id,
    replaceCart,
    focusBarcode
  });

  /* A quick tile is the same add as a scan: one command, one error surface, so
     an out-of-stock or unpriced item behaves identically either way. Focus
     returns to the scan field, because the tap was a detour from scanning. */
  const handleQuickAdd = useCallback(async (productId: string) => {
    try {
      await addProductById(productId);
    } catch { /* useCart already surfaced it in the banner */ }
    focusBarcode();
  }, [addProductById, focusBarcode]);

  const { printSaleNow } = usePosReceipt({
    userId: sessionUser.user_id,
    setBizFlags,
    setReceiptStatus,
    setError,
    focusBarcode
  });

  // ── noModalOpen — stable boolean for shortcut guard ───────────────────────────
  // Because only one ActiveModal variant can be active at a time, this is now
  // trivially derived from the union.  Adding a new blocking modal? Just add its
  // variant to the ActiveModal union — no need to update this expression.
  const noModalOpen = activeModal.kind === "none";

  const {
    // `openPayDirect` (straight to cash/card/wallet) stays on the hook but has
    // no caller here: its buttons lived behind CartPanel's `!compact` branch,
    // and the till has always rendered that panel compact, so they were already
    // unreachable before the rearrangement. Method choice happens in the
    // payment modal each journey opens.
    payFastLoading, restockAlerts, dismissRestockAlerts, handlePayFast,
    openPaySplit, openPay, openPaymentJourney, handleConfirmPayment,
    handleCompleteCoveredExchange
  } = usePosPaymentActions({
    cart,
    exchangeCredit,
    exchangeBalance,
    netTotal,
    payableTotal,
    lineCount,
    noModalOpen,
    sessionUser,
    finalizeSale,
    printSaleNow,
    setActiveModal,
    setBannerResult,
    setLastReceiptNumber,
    setReceiptStatus,
    setExchangeCredit,
    focusBarcode,
    openWhatsAppQr: () => setShowWaQR(true)
  });

  const {
    reprintReceiptNow, handleReprintLast, handleNoSale, handleEditSale,
    handleClearCartRequest
  } = usePosRegisterActions({
    shift,
    sessionToken: sessionUser.session_token,
    userId: sessionUser.user_id,
    lineCount,
    lastReceiptNumber,
    printSaleNow,
    clearCart,
    replaceCart,
    setError,
    setReceiptStatus,
    setLastReceiptNumber,
    setActiveModal,
    focusBarcode
  });


  // ── Hold / Resume ─────────────────────────────────────────────────────────────
  const handleOpenHold = useCallback(() => setActiveModal({ kind: "hold" }), []);

  const lineShortcuts = usePosLineShortcuts({
    selectedLine, lineCount, setActiveModal, setQtyPadLineId, setShowMore,
    removeLine, focusBarcode, openPaymentJourney
  });

  usePosShortcutBindings({
    ...lineShortcuts,
    noModalOpen,
    lineCount,
    recentLineId,
    lastReceiptNumber,
    canRefund,
    canViewXReport,
    setActiveModal,
    focusBarcode,
    openHold: handleOpenHold,
    openPay,
    payFast: handlePayFast,
    clearCart: handleClearCartRequest,
    reprintLast: handleReprintLast,
    noSale: handleNoSale,
    bumpRecentQty,
    removeRecentLine,
    onLock,
    onLogout
  });

  // ── Focus barcode after any modal closes ──────────────────────────────────────
  useEffect(() => {
    if (noModalOpen) focusBarcode();
  }, [noModalOpen, focusBarcode]);

  const clockTime = useClockTime();

  // ── Sync status helpers ───────────────────────────────────────────────────────
  const isOnline      = syncStatus?.online ?? false;

  const { recoveryActions, handleRecoveryAction } = usePosRecoveryActions({
    error,
    bannerResult,
    receiptStatus,
    syncStatus,
    lineCount,
    canOpenBackOffice,
    sessionToken: sessionUser.session_token,
    cart,
    setCart: replaceCart,
    printSaleNow,
    setReceiptStatus,
    setError,
    clearError,
    focusBarcode,
    openHold: handleOpenHold,
    openSyncDetails: () => setShowSyncDetails(true),
    openWhatsAppQr: () => setShowWaQR(true),
    onAskOfficeAI,
    onOpenOfficeAI
  });

  const { buildZanAiContext, zanAiSuppressed } = usePosZanAiContext({
    cart, shift, sessionUser, netTotal, taxTotal, syncStatus,
    modalOpen: activeModal.kind !== "none", payFastLoading, showSaleDetails
  });


  return {
    payFastLoading, activeModal, addByBarcode, addCustomItem, addProduct, applyBillDiscount, applyLineDiscount,
    bannerResult, barcodeRef, bizFlags, buildZanAiContext, bumpLine, canOpenBackOffice, canRefund,
    cart, clearCart, clearError, clockTime, commerceEnabled, dismissRecovery, dismissRestockAlerts,
    error, exchangeBalance, exchangeCredit, focusBarcode, handleBarcode, handleClearCartRequest,
    handleCompleteCoveredExchange, handleConfirmPayment, handleEditSale, handleNoSale,
    handleNumpadKey, handleOpenHold, handlePayFast, handleQuickAdd, handleRecoveryAction,
    handleReprintLast, isOnline, language, lastReceiptNumber, lineCount, loading, netTotal,
    notifCount, numpadValue, onAskOfficeAI, onLogout, onOpenOfficeAI, onShiftClose, onToggleTheme,
    openPaySplit, openPaymentJourney, orderCount, payableTotal, printSaleNow, qtyPadLine,
    quickSlots, receiptStatus, recoverCart, recoverable, recoveryActions, refreshNotifications,
    refreshSuggestions, removeLine, replaceCart, reprintReceiptNow, restockAlerts, selectedLine,
    selectedLineId, sessionUser, setActiveModal, setBannerResult, setError, setExchangeCredit,
    setLinePrice, setNumpadValue, setPickedLineId, setQtyPadLineId, setShowDeliveries, setShowMore,
    setShowNotes, setShowNotifications, setShowOrders, setShowSaleDetails, setShowSyncDetails,
    setShowWaQR, setTrainingMode, shift, showDeliveries, showMore, showNotes, showNotifications,
    showOrders, showSaleDetails, showSidebar, showSyncDetails, showWaQR, suggestions, syncStatus, t,
    taxTotal, theme, toggleLanguage, toggleSidebar, trainingMode, waConnected, waStale,
    zanAiSuppressed
  };
}
