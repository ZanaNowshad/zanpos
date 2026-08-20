import { useCallback, useEffect, useMemo, useState } from "react";
import type { Cart, PaymentInput, SaleResult } from "../types";
import { DEVICE } from "../types";
import PosSidebar from "../components/pos/PosSidebar";
import PosLineActions from "../components/pos/PosLineActions";
import PosMoreDrawer from "../components/pos/PosMoreDrawer";
import PosQtyPad from "../components/pos/PosQtyPad";
import PosSecondaryOverlays from "../components/pos/PosSecondaryOverlays";
import PosCartModals from "../components/pos/PosCartModals";
import PosOperationsModals from "../components/pos/PosOperationsModals";
import PosTenderOverlays from "../components/pos/PosTenderOverlays";
import PosTopBar from "../components/pos/PosTopBar";
import PosStatusBanners from "../components/pos/PosStatusBanners";
import PosCartColumn from "../components/pos/PosCartColumn";
import PosTotalsPanel from "../components/pos/PosTotalsPanel";
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
import { useCustomItemSuggestions } from "../hooks/useCustomItemSuggestions";
import { usePosZanAiContext } from "../hooks/usePosZanAiContext";
import { useQuickPosSlots } from "../hooks/useQuickPosSlots";
import { posLayoutClass } from "./posLayoutClass";
import { usePosOverlays } from "../hooks/usePosOverlays";
import type { PosPageProps } from "./posPageProps";
import { usePosNumpad } from "../hooks/usePosNumpad";
import { usePosShortcutBindings } from "../hooks/usePosShortcutBindings";
import { type ReceiptConfidenceStatus } from "../utils/posConfidence";
import { getExchangeBalance } from "../utils/posExchange";
import PosZanAiWidget from "../zanai/PosZanAiWidget";
import "../components/pos/till.css";


export default function PosPage({
  sessionUser, shift, onLogout, onLock, onShiftClose, onOpenOfficeAI, onAskOfficeAI, theme, onToggleTheme,
}: PosPageProps) {
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
  /* The line the contextual action strip acts on. Follows the scan by default —
     the item just added is nearly always the one being corrected — but a tap on
     any row takes over until the next scan. */
  const [pickedLineId, setPickedLineId] = useState<string | null>(null);
  const [showMore, setShowMore] = useState(false);
  const [qtyPadLineId, setQtyPadLineId] = useState<string | null>(null);
  const {
    showWaQR, setShowWaQR, showSyncDetails, setShowSyncDetails,
    showNotes, setShowNotes, showNotifications, setShowNotifications,
    showOrders, setShowOrders, showDeliveries, setShowDeliveries,
  } = usePosOverlays();

  const {
    businessFlags: bizFlags,
    setBusinessFlags: setBizFlags,
    commerceEnabled,
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
  const [trainingMode, setTrainingMode] = useState(() => {
    try {
      const requested = sessionStorage.getItem("zanpos:start-practice") === "1";
      sessionStorage.removeItem("zanpos:start-practice");
      return requested;
    } catch {
      return false;
    }
  });
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
    addByBarcode, addProduct, addProductById, addCustomItem,
    updateQuantity, removeLine, removeRecentLine, bumpRecentQty, bumpLine,
    applyBillDiscount, applyLineDiscount, setLinePrice,
    finalizeSale, clearCart, replaceCart,
    netTotal, taxTotal, lineCount,
  } = useCart(session, buildTrainingResult);

  /* A scan always takes the selection back — the cashier's attention is on the
     item that just landed, not the row they inspected three items ago. */
  const selectedLineId = pickedLineId ?? recentLineId;
  const selectedLine = cart.lines.find(
    line => line.cart_line_id === selectedLineId && !line.voided,
  ) ?? null;

  /* Keyed to the selected line rather than the scanned one. Identical
     behaviour while nothing is picked — the two are the same line — and the
     quantity pad now also works on a row the cashier tapped further up. */
  const { numpadValue, setNumpadValue, handleNumpadKey } =
    usePosNumpad(selectedLineId, updateQuantity);
  const qtyPadLine = cart.lines.find(
    line => line.cart_line_id === qtyPadLineId && !line.voided,
  ) ?? null;

  useEffect(() => { setPickedLineId(null); }, [recentLineId]);


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
    focusBarcode,
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
    onLineQty: () => { if (selectedLine) setQtyPadLineId(selectedLine.cart_line_id); },
    onLinePrice: () => {
      if (!selectedLine) return;
      setActiveModal({
        kind: "priceInput",
        mode: "setExisting",
        lineId: selectedLine.cart_line_id,
        productName: selectedLine.product_name,
        currentPriceMinor: selectedLine.unit_price_minor,
      });
    },
    onVoidLine: () => { if (selectedLine) { removeLine(selectedLine.cart_line_id); focusBarcode(); } },
    onMoreOptions: () => setShowMore(true),
    onJourney: journey => { if (lineCount > 0) openPaymentJourney(journey); },
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

  const { buildZanAiContext, zanAiSuppressed } = usePosZanAiContext({
    cart, shift, sessionUser, netTotal, taxTotal, syncStatus,
    modalOpen: activeModal.kind !== "none", payFastLoading, showSaleDetails,
  });

  return (
    <div className={posLayoutClass({ hasCart: lineCount > 0, online: isOnline,
      paymentStarted: activeModal.kind === "payment" || payFastLoading })}>
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
      <div
        className="pos-main till-layout"
        style={{ gridTemplateColumns: showSidebar ? "76px minmax(0, 1fr) 322px" : "0px minmax(0, 1fr) 322px" }}
      >
        <PosSidebar
          visible={showSidebar}
          t={t}
          canOpenBackOffice={canOpenBackOffice}
          commerceEnabled={commerceEnabled}
          notifCount={notifCount}
          orderCount={orderCount}
          onOpenReport={() => setActiveModal({ kind: "report" })}
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

        <div className="till-centre">
          <PosCartColumn
            barcodeRef={barcodeRef}
            actorUserId={sessionUser.user_id}
            cart={cart}
            numpadValue={numpadValue}
            suggestions={suggestions}
            loading={loading}
            payFastLoading={payFastLoading}
            selectedLineId={selectedLineId}
            quickSlots={quickSlots}
            setNumpadValue={setNumpadValue}
            setActiveModal={setActiveModal}
            setError={setError}
            addProduct={addProduct}
            addCustomItem={addCustomItem}
            onBarcode={handleBarcode}
            onQuickAdd={handleQuickAdd}
            onSelectLine={setPickedLineId}
            onBumpQty={bumpLine}
            onEditPrice={line => setActiveModal({
              kind: "priceInput",
              mode: "setExisting",
              lineId: line.cart_line_id,
              productName: line.product_name,
              currentPriceMinor: line.unit_price_minor,
            })}
            focusBarcode={focusBarcode}
          />

          <PosLineActions
            line={selectedLine}
            scannedAt={null}
            canDiscount={canOpenBackOffice || bizFlags.cashier_can_discount}
            onQty={() => selectedLine && setQtyPadLineId(selectedLine.cart_line_id)}
            onPrice={() => selectedLine && setActiveModal({
              kind: "priceInput",
              mode: "setExisting",
              lineId: selectedLine.cart_line_id,
              productName: selectedLine.product_name,
              currentPriceMinor: selectedLine.unit_price_minor,
            })}
            onVoid={() => { if (selectedLine) { removeLine(selectedLine.cart_line_id); focusBarcode(); } }}
            onDiscount={() => selectedLine && setActiveModal({
              kind: "discount", lineId: selectedLine.cart_line_id,
            })}
          />
        </div>

        <PosTotalsPanel
          cart={cart}
          taxTotal={taxTotal}
          payableTotal={payableTotal}
          exchangeCredit={exchangeCredit}
          exchangeBalance={exchangeBalance}
          payFastLoading={payFastLoading}
          paymentStarted={activeModal.kind === "payment"}
          onCancelExchange={() => setExchangeCredit(null)}
          onCompleteCoveredExchange={handleCompleteCoveredExchange}
          onPayFast={handlePayFast}
          onOpenPaymentJourney={openPaymentJourney}
          onOpenMore={() => setShowMore(true)}
        />
      </div>

      <PosMoreDrawer
        open={showMore}
        lineCount={lineCount}
        canDiscount={canOpenBackOffice || bizFlags.cashier_can_discount}
        canRefund={canRefund}
        lastReceiptNumber={lastReceiptNumber}
        onClose={() => { setShowMore(false); focusBarcode(); }}
        onClearCart={handleClearCartRequest}
        onOpenHold={handleOpenHold}
        onOpenDiscount={() => setActiveModal({ kind: "discount" })}
        onOpenRefund={() => setActiveModal({ kind: "refund" })}
        onOpenCashEvent={() => setActiveModal({ kind: "cashEvent" })}
        onOpenDeliveries={() => setShowDeliveries(true)}
        onOpenRecent={() => setActiveModal({ kind: "recent" })}
        onReprintLast={handleReprintLast}
        onCustomItem={() => setActiveModal({ kind: "customItem" })}
        onNoSale={handleNoSale}
        onPaySplit={openPaySplit}
        onHelp={() => setActiveModal({ kind: "help" })}
      />

      <PosQtyPad
        line={qtyPadLine}
        value={numpadValue}
        onKey={handleNumpadKey}
        onBump={delta => { if (qtyPadLine) bumpLine(qtyPadLine.cart_line_id, delta); }}
        onClose={() => { setQtyPadLineId(null); setNumpadValue("1"); focusBarcode(); }}
      />

      <PosZanAiWidget
        sessionUser={sessionUser}
        branchName={DEVICE.branch_name}
        suppressed={zanAiSuppressed}
        buildContext={buildZanAiContext}
        focusBarcode={focusBarcode}
      />

      <PosCartModals
        activeModal={activeModal}
        setActiveModal={setActiveModal}
        cart={cart}
        lineCount={lineCount}
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
        defaultPrintReceipt={bizFlags.auto_print_receipt}
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
