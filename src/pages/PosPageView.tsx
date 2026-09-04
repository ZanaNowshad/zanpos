import type { } from "../types";
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

import { } from "../hooks/useLanguage";
import { } from "../i18n/posStrings";
import { } from "../hooks/useCart";
import { } from "../hooks/useSyncStatus";
import { } from "../hooks/usePosAlerts";
import { } from "../hooks/useClockTime";
import { } from "../hooks/usePersistedToggle";
import { } from "../hooks/usePosReceipt";
import { } from "../hooks/usePosCartRecovery";
import { } from "../hooks/usePosBarcode";
import { } from "../hooks/useWhatsAppHealth";
import { } from "../hooks/usePosRecoveryActions";
import { } from "../hooks/usePosPaymentActions";
import { } from "../hooks/usePosRegisterActions";
import { } from "../hooks/usePosConfiguration";
import { } from "../hooks/useCustomItemSuggestions";
import { } from "../hooks/usePosZanAiContext";
import { } from "../hooks/useQuickPosSlots";
import { posLayoutClass } from "./posLayoutClass";
import { } from "../hooks/usePosOverlays";
import type { } from "./posPageProps";
import { usePosPageState } from "./usePosPageState";
import { } from "./usePracticeMode";
import { } from "./usePosSelection";
import { } from "./usePosLineShortcuts";
import { } from "../hooks/usePosShortcutBindings";
import { } from "../utils/posConfidence";
import { } from "../utils/posExchange";
import PosZanAiWidget from "../zanai/PosZanAiWidget";
import "../components/pos/till.css";



export type PosPageViewProps = ReturnType<typeof usePosPageState>;

/** The till, rendered. All state and behaviour arrives from usePosPageState. */
export default function PosPageView({
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
}: PosPageViewProps) {
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
        sessionToken={sessionUser.session_token}
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
              currentPriceMinor: line.unit_price_minor
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
              currentPriceMinor: selectedLine.unit_price_minor
            })}
            onHold={handleOpenHold}
            onDiscount={() => selectedLine && setActiveModal({
              kind: "discount", lineId: selectedLine.cart_line_id
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
        selectedLineName={selectedLine?.product_name ?? null}
        onClose={() => { setShowMore(false); focusBarcode(); }}
        onClearCart={handleClearCartRequest}
        onVoidLine={() => { if (selectedLine) { removeLine(selectedLine.cart_line_id); focusBarcode(); } }}
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
