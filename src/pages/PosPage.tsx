import { useCallback, useEffect, useMemo, useState } from "react";
import type { AiHandoff, Cart, PaymentInput, ProductPrefill, SaleResult, SessionUser, Shift } from "../types";
import { type Theme } from "../hooks/useTheme";
import { DEVICE } from "../types";
import PosSidebar from "../components/pos/PosSidebar";
import PosActionBar from "../components/pos/PosActionBar";
import PosSecondaryOverlays from "../components/pos/PosSecondaryOverlays";
import PosCartModals from "../components/pos/PosCartModals";
import PosOperationsModals from "../components/pos/PosOperationsModals";
import PosTenderOverlays from "../components/pos/PosTenderOverlays";
import PosTopBar from "../components/pos/PosTopBar";
import PosStatusBanners from "../components/pos/PosStatusBanners";
import PosCartColumn from "../components/pos/PosCartColumn";
import PosNumpadPanel from "../components/pos/PosNumpadPanel";
import type { ActiveModal, ExchangeCredit } from "../components/pos/posModalState";

import { buildTrainingSale } from "../utils/trainingSale";
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
import { usePosNumpad } from "../hooks/usePosNumpad";
import { usePosShortcutBindings } from "../hooks/usePosShortcutBindings";
import { type ReceiptConfidenceStatus } from "../utils/posConfidence";
import { getExchangeBalance } from "../utils/posExchange";

interface Props {
  sessionUser: SessionUser;
  shift: Shift;
  onLogout: () => void;
  onLock?: () => void;
  onShiftClose: (closed: boolean) => void;
  onOpenOfficeAI?: (prefill?: ProductPrefill) => void;
  /** Open OfficeAI's assistant and send this message (and image) to the AI. */
  onAskOfficeAI?: (handoff: AiHandoff) => void;
  theme?: Theme;
  onToggleTheme?: () => void;
}

export default function PosPage({
  sessionUser, shift, onLogout, onLock, onShiftClose, onOpenOfficeAI, onAskOfficeAI, theme, onToggleTheme,
}: Props) {
  const session = useMemo(() => ({
    branch_id: DEVICE.branch_id,
    device_id: DEVICE.device_id,
    shift_id: shift.shift_id,
    cashier_user_id: sessionUser.user_id,
  }), [shift.shift_id, sessionUser.user_id]);

  const [showSidebar, toggleSidebar] = usePersistedToggle("zanpos_sidebar");

  const [activeModal, setActiveModal] = useState<ActiveModal>({ kind: "none" });
  const [bannerResult, setBannerResult] = useState<SaleResult | null>(null);
  const [showSaleDetails, setShowSaleDetails] = useState(false);
  const [lastReceiptNumber, setLastReceiptNumber] = useState<string | null>(null);
  const [receiptStatus, setReceiptStatus] = useState<ReceiptConfidenceStatus>("not_ready");
  const [exchangeCredit, setExchangeCredit] = useState<ExchangeCredit | null>(null);
  const [showWaQR, setShowWaQR]             = useState(false);
  const [showSyncDetails, setShowSyncDetails] = useState(false);
  const [showNotes, setShowNotes]           = useState(false);
  const [showNotifications, setShowNotifications] = useState(false);
  const [showOrders, setShowOrders] = useState(false);
  const [showDeliveries, setShowDeliveries] = useState(false);

  const {
    businessFlags: bizFlags,
    setBusinessFlags: setBizFlags,
    commerceEnabled,
  } = usePosConfiguration(sessionUser.user_id);
  // Saved custom-item suggestions — loaded from localStorage, refreshed after modal closes
  const [suggestions, setSuggestions] = useState<{ id: string; name: string; price: string }[]>(() => {
    try { return JSON.parse(localStorage.getItem("zanpos_custom_suggestions") || "[]"); } catch { return []; }
  });
  const refreshSuggestions = useCallback(() => {
    try { setSuggestions(JSON.parse(localStorage.getItem("zanpos_custom_suggestions") || "[]")); } catch { setSuggestions([]); }
  }, []);

  const canOpenBackOffice = ["owner", "manager"].includes(sessionUser.role_name);
  const canViewXReport    = canOpenBackOffice;
  const canRefund         = ["owner", "manager", "cashier"].includes(sessionUser.role_name);

  // Sidebar badges: alerts bell + unfulfilled orders, one shared 8s poll.
  const { notifCount, orderCount, refreshNotifications } = usePosAlerts({
    userId: sessionUser.user_id,
    canOpenBackOffice,
    commerceEnabled,
  });

  // M21: App.tsx already owns an idle timer (App-level, 60s warning → logout).
  // PosPage previously ran a second independent idle timer that called onLogout
  // directly, bypassing the warning. Removed here — App's timer handles logout.

  const syncStatus = useSyncStatus(15_000, sessionUser.user_id);

  // Till strings. `dir` is applied to <html> by the hook, so RTL flips at the
  // layout level rather than being re-implemented per component.
  const { language, toggle: toggleLanguage } = useLanguage();
  const t = useMemo(() => posTranslator(language), [language]);

  // ── Training mode ───────────────────────────────────────────────────────────
  // A rehearsal till for a cashier's first day. Sales built here are never sent
  // to the backend, so they cannot reach reports, EOD, stock or the receipt
  // sequence — a structural guarantee rather than a filter every future report
  // query has to remember. Everything else, including the real print, is live.
  const [trainingMode, setTrainingMode] = useState(false);
  const buildTrainingResult = useMemo(
    () => trainingMode
      ? (cart: Cart, payments: PaymentInput[]) =>
          buildTrainingSale(cart, payments, sessionUser, DEVICE.branch_name, DEVICE.currency)
      : null,
    [trainingMode, sessionUser],
  );

  const {
    cart, loading, error, clearError, setError,
    recentLineId,
    addByBarcode, addProduct, addCustomItem,
    updateQuantity, removeLine, removeRecentLine, bumpRecentQty, bumpLine,
    applyBillDiscount, applyLineDiscount, setLinePrice, setLineNote,
    finalizeSale, clearCart, replaceCart,
    netTotal, taxTotal, lineCount,
  } = useCart(session, buildTrainingResult);

  const { numpadValue, setNumpadValue, handleNumpadKey } =
    usePosNumpad(recentLineId, updateQuantity);

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
    refreshNotifications,
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
    focusBarcode,
  });

  const { printSaleNow } = usePosReceipt({
    userId: sessionUser.user_id,
    setBizFlags,
    setReceiptStatus,
    setError,
    focusBarcode,
  });

  // ── noModalOpen — stable boolean for shortcut guard ───────────────────────────
  // Because only one ActiveModal variant can be active at a time, this is now
  // trivially derived from the union.  Adding a new blocking modal? Just add its
  // variant to the ActiveModal union — no need to update this expression.
  const noModalOpen = activeModal.kind === "none";

  const {
    payFastLoading, restockAlerts, dismissRestockAlerts, handlePayFast,
    openPayDirect, openPaySplit, openPay, handleConfirmPayment,
    handleCompleteCoveredExchange,
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
    openWhatsAppQr: () => setShowWaQR(true),
  });

  const {
    reprintReceiptNow, handleReprintLast, handleNoSale, handleEditSale,
    handleClearCartRequest,
  } = usePosRegisterActions({
    shift,
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
    focusBarcode,
  });


  // ── Hold / Resume ─────────────────────────────────────────────────────────────
  const handleOpenHold = useCallback(() => setActiveModal({ kind: "hold" }), []);

  usePosShortcutBindings({
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
    onLogout,
  });

  // ── Focus barcode after any modal closes ──────────────────────────────────────
  useEffect(() => {
    if (noModalOpen) focusBarcode();
  }, [noModalOpen, focusBarcode]);

  const clockTime = useClockTime();

  // ── Sync status helpers ───────────────────────────────────────────────────────
  const isOnline      = syncStatus?.online ?? false;

  // Line-discount target — hoisted out of the JSX so no render-time IIFE wraps
  // the modal's handlers (react-hooks/refs flags ref-reading closures inside one).
  const discountLine = activeModal.kind === "lineDiscount"
    ? cart.lines.find(l => l.cart_line_id === activeModal.lineId && !l.voided)
    : undefined;

  const { recoveryActions, handleRecoveryAction } = usePosRecoveryActions({
    error,
    bannerResult,
    receiptStatus,
    syncStatus,
    lineCount,
    canOpenBackOffice,
    userId: sessionUser.user_id,
    printSaleNow,
    setReceiptStatus,
    setError,
    clearError,
    focusBarcode,
    openHold: handleOpenHold,
    openSyncDetails: () => setShowSyncDetails(true),
    openWhatsAppQr: () => setShowWaQR(true),
    onAskOfficeAI,
    onOpenOfficeAI,
  });

  return (
    <div className={`pos-layout ${lineCount > 0 ? "pos-has-cart" : "pos-idle"} ${activeModal.kind === "payment" || payFastLoading ? "pos-payment-started" : ""} ${!isOnline ? "pos-offline" : "pos-online"}`}>
      <PosTopBar
        showSidebar={showSidebar}
        cashierName={sessionUser.display_name}
        theme={theme}
        language={language}
        clockTime={clockTime}
        syncStatus={syncStatus}
        userId={sessionUser.user_id}
        commerceEnabled={commerceEnabled}
        waConnected={waConnected}
        waStale={waStale}
        lastReceiptNumber={lastReceiptNumber}
        onToggleSidebar={toggleSidebar}
        onHome={() => { setActiveModal({ kind: "none" }); setBannerResult(null); focusBarcode(); }}
        onToggleTheme={onToggleTheme}
        onToggleLanguage={toggleLanguage}
        onOpenSyncDetails={() => setShowSyncDetails(true)}
        onReprintLast={handleReprintLast}
      />

      <PosStatusBanners
        trainingMode={trainingMode}
        t={t}
        recoverable={recoverable}
        error={error}
        recoveryActions={recoveryActions}
        onExitTraining={() => { clearCart(); setTrainingMode(false); focusBarcode(); }}
        onRecoverCart={recoverCart}
        onDismissRecovery={dismissRecovery}
        onRecoveryAction={handleRecoveryAction}
        onDismissError={() => { clearError(); focusBarcode(); }}
      />

      {/* ── Main area ── */}
      <div className="pos-main" style={{ gridTemplateColumns: showSidebar ? "76px minmax(0, 1fr) 344px" : "0px minmax(0, 1fr) 344px" }}>
        <PosSidebar
          visible={showSidebar}
          t={t}
          trainingMode={trainingMode}
          canViewXReport={canViewXReport}
          canOpenBackOffice={canOpenBackOffice}
          commerceEnabled={commerceEnabled}
          notifCount={notifCount}
          orderCount={orderCount}
          onStartTraining={() => { clearCart(); setTrainingMode(true); focusBarcode(); }}
          onOpenReport={() => setActiveModal({ kind: "report" })}
          onOpenXReport={() => setActiveModal({ kind: "xReport" })}
          onOpenOfficeAI={() => {
            // Opening the back office mid-sale would strand the cart behind a
            // full-screen surface, so the cashier is told to resolve it first.
            if (lineCount > 0) {
              setError("Hold or complete the current sale before opening OfficeAI");
              return;
            }
            if (onOpenOfficeAI) onOpenOfficeAI();
          }}
          onOpenNotes={() => setShowNotes(true)}
          onOpenOrders={() => setShowOrders(true)}
          onOpenNotifications={() => setShowNotifications(true)}
          onCloseShift={() => setActiveModal({ kind: "shiftClose" })}
          onLogout={onLogout}
        />

        <PosCartColumn
          barcodeRef={barcodeRef}
          actorUserId={sessionUser.user_id}
          cart={cart}
          netTotal={netTotal}
          taxTotal={taxTotal}
          numpadValue={numpadValue}
          suggestions={suggestions}
          loading={loading}
          payFastLoading={payFastLoading}
          paymentStarted={activeModal.kind === "payment"}
          recentLineId={recentLineId}
          setNumpadValue={setNumpadValue}
          setActiveModal={setActiveModal}
          setError={setError}
          addProduct={addProduct}
          addCustomItem={addCustomItem}
          updateQuantity={updateQuantity}
          removeLine={removeLine}
          applyLineDiscount={applyLineDiscount}
          setLineNote={setLineNote}
          bumpLine={bumpLine}
          onBarcode={handleBarcode}
          onPaySplit={openPaySplit}
          onPayFast={handlePayFast}
          onPayDirect={openPayDirect}
          focusBarcode={focusBarcode}
        />

        <PosNumpadPanel
          cart={cart}
          numpadValue={numpadValue}
          recentLineId={recentLineId}
          taxTotal={taxTotal}
          payableTotal={payableTotal}
          exchangeCredit={exchangeCredit}
          exchangeBalance={exchangeBalance}
          payFastLoading={payFastLoading}
          paymentStarted={activeModal.kind === "payment"}
          onNumpadKey={handleNumpadKey}
          onCancelExchange={() => setExchangeCredit(null)}
          onCompleteCoveredExchange={handleCompleteCoveredExchange}
          onPayFast={handlePayFast}
          onPayDirect={openPayDirect}
          onPaySplit={openPaySplit}
        />
      </div>

      {/* ── Action bar ── */}
      <PosActionBar
        lineCount={lineCount}
        canDiscount={canOpenBackOffice || bizFlags.cashier_can_discount}
        canRefund={canRefund}
        lastReceiptNumber={lastReceiptNumber}
        onClearCart={handleClearCartRequest}
        onOpenHold={handleOpenHold}
        onOpenDiscount={() => setActiveModal({ kind: "discount" })}
        onOpenRefund={() => setActiveModal({ kind: "refund" })}
        onOpenCashEvent={() => setActiveModal({ kind: "cashEvent" })}
        onOpenDeliveries={() => setShowDeliveries(true)}
        onOpenRecent={() => setActiveModal({ kind: "recent" })}
        onReprintLast={handleReprintLast}
      />

      <PosCartModals
        activeModal={activeModal}
        setActiveModal={setActiveModal}
        cart={cart}
        lineCount={lineCount}
        discountLine={discountLine}
        addCustomItem={addCustomItem}
        setLinePrice={setLinePrice}
        applyBillDiscount={applyBillDiscount}
        applyLineDiscount={applyLineDiscount}
        clearCart={clearCart}
        refreshSuggestions={refreshSuggestions}
        resetNumpad={() => setNumpadValue("1")}
        focusBarcode={focusBarcode}
      />

      <PosTenderOverlays
        activeModal={activeModal}
        payableTotal={payableTotal}
        loading={loading}
        sessionUserId={sessionUser.user_id}
        bannerResult={bannerResult}
        receiptStatus={receiptStatus}
        showSaleDetails={showSaleDetails}
        onConfirmPayment={handleConfirmPayment}
        onClosePayment={() => { setActiveModal({ kind: "none" }); focusBarcode(); }}
        onPrint={sale => { void printSaleNow(sale, false, "manual"); }}
        onNewSale={() => {
          setBannerResult(null);
          setShowSaleDetails(false);
          focusBarcode();
        }}
        onViewDetails={() => setShowSaleDetails(true)}
        onCloseDetails={() => {
          setShowSaleDetails(false);
          focusBarcode();
        }}
      />

      <PosOperationsModals
        activeModal={activeModal}
        setActiveModal={setActiveModal}
        sessionUser={sessionUser}
        shift={shift}
        cart={cart}
        lineCount={lineCount}
        netTotal={netTotal}
        clearCart={clearCart}
        replaceCart={replaceCart}
        setExchangeCredit={setExchangeCredit}
        onShiftClose={onShiftClose}
        onReprintReceipt={reprintReceiptNow}
        onEditSale={handleEditSale}
        focusBarcode={focusBarcode}
      />

      <PosSecondaryOverlays
        sessionUser={sessionUser}
        showWaQr={showWaQR}
        showNotifications={showNotifications}
        showOrders={showOrders}
        showDeliveries={showDeliveries}
        showNotes={showNotes}
        showSyncDetails={showSyncDetails}
        syncStatus={syncStatus}
        restockAlerts={restockAlerts}
        lineCount={lineCount}
        setError={setError}
        addByBarcode={addByBarcode}
        refreshNotifications={refreshNotifications}
        focusBarcode={focusBarcode}
        onCloseWaQr={() => setShowWaQR(false)}
        onCloseNotifications={() => setShowNotifications(false)}
        onCloseOrders={() => { setShowOrders(false); focusBarcode(); }}
        onCloseDeliveries={() => { setShowDeliveries(false); focusBarcode(); }}
        onCloseNotes={() => setShowNotes(false)}
        onCloseSyncDetails={() => { setShowSyncDetails(false); focusBarcode(); }}
        onDismissRestockAlerts={dismissRestockAlerts}
        onOpenOfficeAI={onOpenOfficeAI}
        onAskOfficeAI={onAskOfficeAI}
      />
    </div>
  );
}
