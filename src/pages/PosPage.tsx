import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { LowStockAlert, PaymentInput, ProductWithPrice, SaleResult, SessionUser, Shift } from "../types";
import { DEVICE } from "../types";
import { cashNoSale, productListAll, receiptReprint } from "../tauri/commands";
import { useCart } from "../hooks/useCart";
import { useSyncStatus } from "../hooks/useSyncStatus";
import { usePosShortcuts } from "../hooks/usePosShortcuts";
import BarcodeInput, { type BarcodeInputHandle } from "../components/BarcodeInput";
import ProductGrid from "../components/ProductGrid";
import CartPanel from "../components/CartPanel";
import BackOfficeModal from "../components/BackOfficeModal";
import DiscountModal from "../components/DiscountModal";
import PaymentModal from "../components/PaymentModal";
import ReceiptPreview from "../components/ReceiptPreview";
import SyncChip from "../components/SyncChip";
import ShiftModal from "../components/ShiftModal";
import HoldModal from "../components/HoldModal";
import RefundModal from "../components/RefundModal";
import TodayReportModal from "../components/TodayReportModal";
import CustomItemModal from "../components/CustomItemModal";
import CashEventModal from "../components/CashEventModal";
import XReportModal from "../components/XReportModal";

interface Props {
  sessionUser: SessionUser;
  shift: Shift;
  onLogout: () => void;
  onShiftClose: (closed: boolean) => void;
  onOpenAdminChat?: () => void;
  theme?: "dark" | "light";
  onToggleTheme?: () => void;
}

export default function PosPage({
  sessionUser, shift, onLogout, onShiftClose, onOpenAdminChat, theme, onToggleTheme,
}: Props) {
  const session = {
    branch_id: DEVICE.branch_id,
    device_id: DEVICE.device_id,
    shift_id: shift.shift_id,
    cashier_user_id: sessionUser.user_id,
  };

  // ── Products ─────────────────────────────────────────────────────────────────
  const [allProducts, setAllProducts] = useState<ProductWithPrice[]>([]);
  const [productLoading, setProductLoading] = useState(true);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedCategory, setSelectedCategory] = useState<string | null>(null);

  // ── Modal state ───────────────────────────────────────────────────────────────
  const [showPayment, setShowPayment]     = useState(false);
  const [paymentMethod, setPaymentMethod] = useState<PaymentInput["method"] | undefined>(undefined);
  const [paymentSplit, setPaymentSplit]   = useState(false);
  const [saleResult, setSaleResult]       = useState<SaleResult | null>(null);
  const [lastReceiptNumber, setLastReceiptNumber] = useState<string | null>(null);
  const [lastSaleStatus, setLastSaleStatus]       = useState<string | null>(null);
  const [showShiftClose, setShowShiftClose] = useState(false);
  const [showHold, setShowHold]             = useState(false);
  const [showRefund, setShowRefund]         = useState(false);
  const [showReport, setShowReport]         = useState(false);
  const [showDiscount, setShowDiscount]     = useState(false);
  const [showBackOffice, setShowBackOffice] = useState(false);
  const [showCustomItem, setShowCustomItem] = useState(false);
  const [showCashEvent, setShowCashEvent]   = useState(false);
  const [showXReport, setShowXReport]       = useState(false);
  const [showClearConfirm, setShowClearConfirm] = useState(false);
  const [payFastLoading, setPayFastLoading] = useState(false);
  const [restockAlerts, setRestockAlerts]   = useState<LowStockAlert[]>([]);
  const restockTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const canOpenBackOffice = ["owner", "manager"].includes(sessionUser.role_name);
  const canViewXReport    = canOpenBackOffice;
  const canRefund         = ["owner", "manager", "cashier"].includes(sessionUser.role_name);

  const syncStatus = useSyncStatus(15_000);

  const {
    cart, loading, error, clearError,
    recentLineId,
    addByBarcode, addProduct, addCustomItem,
    updateQuantity, removeLine, removeRecentLine, bumpRecentQty,
    applyBillDiscount, applyLineDiscount, setLineNote,
    finalizeSale, clearCart, replaceCart,
    netTotal, taxTotal, lineCount,
  } = useCart(session);

  // ── Barcode input ref for programmatic focus ──────────────────────────────────
  const barcodeRef = useRef<BarcodeInputHandle>(null);
  const focusBarcode = useCallback(() => barcodeRef.current?.focus(), []);

  // ── noModalOpen — stable boolean for shortcut guard ───────────────────────────
  const noModalOpen = useMemo(() =>
    !showPayment && !saleResult && !showShiftClose &&
    !showHold && !showRefund && !showReport &&
    !showDiscount && !showBackOffice && !showCustomItem &&
    !showCashEvent && !showXReport && !showClearConfirm,
    [showPayment, saleResult, showShiftClose, showHold, showRefund,
     showReport, showDiscount, showBackOffice, showCustomItem,
     showCashEvent, showXReport, showClearConfirm]
  );

  // ── Pay Fast ─────────────────────────────────────────────────────────────────
  const handlePayFast = useCallback(async () => {
    if (lineCount === 0 || payFastLoading || !noModalOpen) return;
    setPayFastLoading(true);
    try {
      const payment: PaymentInput = {
        method: "cash",
        amount_minor: netTotal,
        tendered_minor: netTotal,
      };
      const result = await finalizeSale([payment]);
      setLastReceiptNumber(result.receipt_number);
      setLastSaleStatus(`✓ #${result.receipt_number} · Cash · ${DEVICE.currency} ${(netTotal / Math.pow(10, DEVICE.currency_exponent)).toFixed(DEVICE.currency_exponent)}`);
      // No ReceiptPreview — cart already cleared in finalizeSale
      focusBarcode();
      if (result.low_stock_alerts.length > 0) {
        if (restockTimerRef.current) clearTimeout(restockTimerRef.current);
        setRestockAlerts(result.low_stock_alerts);
        restockTimerRef.current = setTimeout(() => setRestockAlerts([]), 6000);
      }
    } catch {
      // error is set in useCart; cart remains intact
    } finally {
      setPayFastLoading(false);
    }
  }, [lineCount, payFastLoading, noModalOpen, netTotal, finalizeSale, focusBarcode]);

  // ── Direct payment (opens modal pre-configured to method) ─────────────────────
  const openPayDirect = useCallback((method: PaymentInput["method"]) => {
    if (lineCount === 0) return;
    setPaymentMethod(method);
    setPaymentSplit(false);
    setShowPayment(true);
  }, [lineCount]);

  const openPaySplit = useCallback(() => {
    if (lineCount === 0) return;
    setPaymentMethod(undefined);
    setPaymentSplit(true);
    setShowPayment(true);
  }, [lineCount]);

  const openPay = useCallback(() => {
    if (lineCount === 0) return;
    setPaymentMethod(undefined);
    setPaymentSplit(false);
    setShowPayment(true);
  }, [lineCount]);

  // ── Confirm payment ───────────────────────────────────────────────────────────
  const handleConfirmPayment = async (payments: PaymentInput[], customerId?: string) => {
    try {
      const result = await finalizeSale(payments, customerId);
      setShowPayment(false);
      setSaleResult(result);
      setLastReceiptNumber(result.receipt_number);
      setLastSaleStatus(`✓ #${result.receipt_number}`);
      if (result.low_stock_alerts.length > 0) {
        if (restockTimerRef.current) clearTimeout(restockTimerRef.current);
        setRestockAlerts(result.low_stock_alerts);
        restockTimerRef.current = setTimeout(() => setRestockAlerts([]), 6000);
      }
    } catch {
      // error is set in useCart; modal stays open
    }
  };

  const handleNewSale = () => {
    setSaleResult(null);
    clearCart();
    focusBarcode();
  };

  const handleReprintLast = async () => {
    if (!lastReceiptNumber) return;
    try {
      const reprinted = await receiptReprint(lastReceiptNumber);
      setSaleResult(reprinted);
    } catch (e: unknown) {
      console.error("Reprint failed", e);
    }
  };

  // ── Clear cart with confirmation ──────────────────────────────────────────────
  const handleClearCartRequest = useCallback(() => {
    if (lineCount === 0) return;
    setShowClearConfirm(true);
  }, [lineCount]);

  const handleClearConfirmed = useCallback(() => {
    clearCart();
    setShowClearConfirm(false);
    focusBarcode();
  }, [clearCart, focusBarcode]);

  // ── Recent item controls ──────────────────────────────────────────────────────
  const handleIncrementRecent = useCallback(() => bumpRecentQty(1),  [bumpRecentQty]);
  const handleDecrementRecent = useCallback(() => bumpRecentQty(-1), [bumpRecentQty]);

  // ── Hold / Resume ─────────────────────────────────────────────────────────────
  const handleOpenHold = useCallback(() => setShowHold(true), []);

  // ── Barcode scan handler ──────────────────────────────────────────────────────
  const handleBarcode = useCallback(async (barcode: string, qty?: number) => {
    await addByBarcode(barcode, qty);
    // BarcodeInput clears and stays focused automatically
  }, [addByBarcode]);

  const handleSearch = useCallback((query: string) => {
    setSearchQuery(query);
  }, []);

  // ── Shortcut manager ─────────────────────────────────────────────────────────
  usePosShortcuts({
    noModalOpen,
    lineCount,
    hasRecentLine: recentLineId !== null,
    lastReceiptNumber,
    onFocusBarcode:      focusBarcode,
    onHold:              handleOpenHold,
    onResumeHeld:        handleOpenHold,
    onPay:               openPay,
    onPayFast:           handlePayFast,
    onDiscount:          () => setShowDiscount(true),
    onRefund:            () => canRefund && setShowRefund(true),
    onClearCart:         handleClearCartRequest,
    onReprintLast:       handleReprintLast,
    onNoSale:            async () => {
      try { await cashNoSale(shift.shift_id, sessionUser.user_id); }
      catch (e) { console.error("No-sale audit failed:", e); }
    },
    onIncrementRecent:   handleIncrementRecent,
    onDecrementRecent:   handleDecrementRecent,
    onRemoveRecent:      removeRecentLine,
    onLock:              onLogout,
    onReport:            () => setShowReport(true),
    onCustomItem:        () => setShowCustomItem(true),
  });

  // ── Focus barcode after any modal closes ──────────────────────────────────────
  useEffect(() => {
    if (noModalOpen) focusBarcode();
  }, [noModalOpen, focusBarcode]);

  // ── Product list ──────────────────────────────────────────────────────────────
  useEffect(() => {
    setProductLoading(true);
    productListAll()
      .then(setAllProducts)
      .catch(e => console.error("Failed to load products", e))
      .finally(() => setProductLoading(false));
  }, []);

  const categories = useMemo(() => {
    const seen = new Map<string, string>();
    for (const p of allProducts) {
      if (!seen.has(p.category_id)) seen.set(p.category_id, p.category_name);
    }
    return Array.from(seen.entries()).map(([id, name]) => ({ id, name }));
  }, [allProducts]);

  const displayProducts = useMemo(() => {
    let products = allProducts;
    if (selectedCategory) products = products.filter(p => p.category_id === selectedCategory);
    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase();
      products = products.filter(p =>
        p.name.toLowerCase().includes(q) ||
        p.sku?.toLowerCase().includes(q) ||
        p.barcode?.includes(q)
      );
    }
    return products;
  }, [allProducts, selectedCategory, searchQuery]);

  // ── Clock ─────────────────────────────────────────────────────────────────────
  const [clockTime, setClockTime] = useState(() =>
    new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
  );
  useEffect(() => {
    const t = setInterval(() =>
      setClockTime(new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })),
      10_000
    );
    return () => clearInterval(t);
  }, []);

  // ── Product select (from grid tap) ────────────────────────────────────────────
  const handleProductSelect = useCallback(async (p: ProductWithPrice) => {
    await addProduct(p);
    focusBarcode();
  }, [addProduct, focusBarcode]);

  // ── Sync status helpers ───────────────────────────────────────────────────────
  const isOnline      = syncStatus?.online ?? false;
  const pendingEvents = syncStatus?.pending_events ?? 0;

  return (
    <div className="pos-layout">
      {/* ── Top bar ── */}
      <div className="top-bar">
        <span className="top-bar-logo">ZAN<span>POS</span></span>
        <span className="top-bar-sep">·</span>
        <span className="top-bar-branch">{DEVICE.branch_name}</span>
        <span className="top-bar-cashier">{sessionUser.display_name}</span>
        <SyncChip status={syncStatus} />
        <span className="top-bar-spacer" />
        <span className="top-bar-time">{clockTime}</span>

        <button className="top-bar-btn" onClick={() => setShowReport(true)} title="Today's sales report">
          Report
        </button>
        {canViewXReport && (
          <button className="top-bar-btn" onClick={() => setShowXReport(true)} title="X-Report — mid-shift drawer check">
            X-Report
          </button>
        )}
        {lastReceiptNumber && (
          <button className="top-bar-btn" onClick={handleReprintLast} title={`Reprint #${lastReceiptNumber} (Ctrl+P)`}>
            Reprint
          </button>
        )}
        {canOpenBackOffice && (
          <button className="top-bar-btn" onClick={() => setShowBackOffice(true)}>
            Back Office
          </button>
        )}
        {onOpenAdminChat && (
          <button className="top-bar-btn top-bar-admin" onClick={onOpenAdminChat}>
            AI Admin
          </button>
        )}
        {onToggleTheme && (
          <button
            className="top-bar-btn top-bar-theme"
            onClick={onToggleTheme}
            title={theme === "dark" ? "Switch to Light Mode" : "Switch to Dark Mode"}
          >
            {theme === "dark" ? "☀" : "🌙"}
          </button>
        )}
        <button className="top-bar-btn top-bar-btn-danger" onClick={() => setShowShiftClose(true)}>
          Close Shift
        </button>
        <button className="top-bar-btn top-bar-logout" onClick={onLogout} title="Ctrl+L">
          Logout
        </button>
      </div>

      {/* ── Error banner ── */}
      {error && (
        <div className="error-banner" onClick={clearError} role="alert">
          ⚠ {error}
          <button className="error-banner-action" onClick={(e) => { e.stopPropagation(); clearError(); focusBarcode(); }}>
            Dismiss & focus scan
          </button>
          <span className="error-dismiss" onClick={clearError}>✕</span>
        </div>
      )}

      {/* ── Main area ── */}
      <div className="pos-main">
        {/* Icon sidebar */}
        <div className="pos-sidebar">
          <button className="pos-sidebar-item active" title="Quick Sale">
            <span className="pos-sidebar-icon">⚡</span>
            <span>Sale</span>
          </button>
          <button className="pos-sidebar-item" title="Products" onClick={() => setShowBackOffice(true)}>
            <span className="pos-sidebar-icon">📦</span>
            <span>Products</span>
          </button>
          <button className="pos-sidebar-item" title="Customers">
            <span className="pos-sidebar-icon">👤</span>
            <span>Customers</span>
          </button>
          <div className="pos-sidebar-divider" />
          <button className="pos-sidebar-item" title="Today's Report" onClick={() => setShowReport(true)}>
            <span className="pos-sidebar-icon">📊</span>
            <span>Reports</span>
          </button>
          {canOpenBackOffice && (
            <button className="pos-sidebar-item" title="Back Office" onClick={() => setShowBackOffice(true)}>
              <span className="pos-sidebar-icon">⚙</span>
              <span>Settings</span>
            </button>
          )}
          <div className="pos-sidebar-spacer" />
          {onOpenAdminChat && (
            <button className="pos-sidebar-item" title="AI Admin" onClick={onOpenAdminChat}>
              <span className="pos-sidebar-icon">✦</span>
              <span>AI</span>
            </button>
          )}
        </div>

        {/* Product area */}
        <div className="product-area">
          <BarcodeInput
            ref={barcodeRef}
            onBarcode={handleBarcode}
            onSearch={handleSearch}
            onEscape={() => { setSearchQuery(""); setSelectedCategory(null); }}
            disabled={loading || payFastLoading}
          />

          {categories.length > 0 && (
            <div className="category-tabs">
              <button
                className={`cat-tab ${selectedCategory === null ? "cat-tab-active" : ""}`}
                onClick={() => setSelectedCategory(null)}
              >
                All
              </button>
              {categories.map(c => (
                <button
                  key={c.id}
                  className={`cat-tab ${selectedCategory === c.id ? "cat-tab-active" : ""}`}
                  onClick={() => setSelectedCategory(c.id)}
                >
                  {c.name}
                </button>
              ))}
            </div>
          )}

          <ProductGrid
            products={displayProducts}
            onSelect={handleProductSelect}
            loading={productLoading}
          />
        </div>

        {/* Cart area */}
        <CartPanel
          cart={cart}
          netTotal={netTotal}
          taxTotal={taxTotal}
          onUpdateQty={updateQuantity}
          onRemove={removeLine}
          onApplyLineDiscount={applyLineDiscount}
          onSetLineNote={setLineNote}
          onPay={openPay}
          recentLineId={recentLineId}
          onIncrementRecent={handleIncrementRecent}
          onDecrementRecent={handleDecrementRecent}
          onPayDirect={openPayDirect}
          onPaySplit={openPaySplit}
          onPayFast={handlePayFast}
          payFastLoading={payFastLoading}
        />
      </div>

      {/* ── Action bar ── */}
      <div className="action-bar">
        <button
          className="action-btn action-btn-danger"
          onClick={handleClearCartRequest}
          disabled={lineCount === 0}
          title="Clear all items from cart (Ctrl+Delete)"
        >
          🗑 Clear
        </button>
        <button
          className="action-btn"
          onClick={() => setShowHold(true)}
          title="Hold cart and resume later (F6 · Ctrl+H)"
        >
          ⏸ Hold
        </button>
        <button
          className="action-btn"
          onClick={() => setShowCustomItem(true)}
          title="Add a custom item with any price"
        >
          ✦ Custom
        </button>
        <button
          className="action-btn"
          onClick={() => setShowDiscount(true)}
          disabled={lineCount === 0}
          title="Apply a bill-level discount (F8 · Ctrl+D)"
        >
          % Discount
        </button>
        <button
          className="action-btn"
          onClick={() => setShowCashEvent(true)}
          title="Paid In / Paid Out / Safe Drop"
        >
          💵 Cash Event
        </button>
        <button
          className="action-btn"
          onClick={async () => {
            try { await cashNoSale(shift.shift_id, sessionUser.user_id); }
            catch (e) { console.error("No-sale audit failed:", e); }
          }}
          title="Open drawer without a sale — audited in the system (F11)"
        >
          🔓 No Sale
        </button>
        {canRefund && (
          <button
            className="action-btn"
            onClick={() => setShowRefund(true)}
            title="Process a return or refund (Ctrl+R)"
          >
            ↩ Refund
          </button>
        )}
        <button
          className="action-btn action-btn-pay"
          onClick={openPay}
          disabled={lineCount === 0}
          title="Collect payment (F9)"
        >
          💳 Pay  <kbd>F9</kbd>
        </button>
      </div>

      {/* ── Status bar ── */}
      <div className="pos-status-bar">
        <span className="status-cashier">👤 {sessionUser.display_name}</span>
        <span className="status-sep">·</span>
        <span className="status-branch">{DEVICE.branch_name}</span>
        <span className="status-sep">·</span>
        <span className="status-device" title={`Device: ${DEVICE.device_id}`}>
          {DEVICE.device_id.substring(0, 8)}…
        </span>
        <span className="status-sep">·</span>
        <span className="status-shift-open">⬤ Shift Open</span>
        <span className="status-sep">·</span>
        <span className={`status-sync ${isOnline ? "status-online" : "status-offline"}`}>
          {isOnline ? "⬤ Online" : "⬤ Offline"}
          {pendingEvents > 0 && <span className="status-pending"> · {pendingEvents} pending</span>}
        </span>
        {lastSaleStatus && (
          <>
            <span className="status-sep">·</span>
            <span className="status-last-sale">{lastSaleStatus}</span>
          </>
        )}
        <span className="status-spacer" />
        <span className="status-shortcuts">F2 Scan · F9 Pay · F12 Pay Fast · F6 Hold · +/− Qty</span>
      </div>

      {/* ── Clear cart confirmation ── */}
      {showClearConfirm && (
        <div className="modal-overlay" onClick={() => { setShowClearConfirm(false); focusBarcode(); }}>
          <div className="modal clear-confirm-modal" onClick={e => e.stopPropagation()}>
            <div className="modal-header">
              <span className="modal-title">Clear Cart?</span>
            </div>
            <p className="clear-confirm-body">
              Remove all {lineCount} item{lineCount !== 1 ? "s" : ""} from the cart?
              This will be recorded as a pre-tender void.
            </p>
            <div className="modal-actions">
              <button className="btn-secondary" onClick={() => { setShowClearConfirm(false); focusBarcode(); }}>
                Cancel
              </button>
              <button className="btn-danger" onClick={handleClearConfirmed}>
                🗑 Clear Cart
              </button>
            </div>
          </div>
        </div>
      )}

      {/* ── Modals ── */}
      {showCustomItem && (
        <CustomItemModal
          onAdd={async (name, price, qty) => {
            await addCustomItem(name, price, qty);
            setShowCustomItem(false);
            focusBarcode();
          }}
          onCancel={() => { setShowCustomItem(false); focusBarcode(); }}
        />
      )}

      {showCashEvent && (
        <CashEventModal
          shiftId={shift.shift_id}
          userId={sessionUser.user_id}
          onDone={() => { setShowCashEvent(false); focusBarcode(); }}
          onCancel={() => { setShowCashEvent(false); focusBarcode(); }}
        />
      )}

      {showDiscount && (
        <DiscountModal
          grossMinor={cart.lines.filter(l => !l.voided).reduce((s, l) => s + l.line_total_minor, 0)}
          currentDiscountMinor={cart.bill_discount_minor}
          onApply={async (discount_minor) => {
            await applyBillDiscount(discount_minor);
            setShowDiscount(false);
            focusBarcode();
          }}
          onCancel={() => { setShowDiscount(false); focusBarcode(); }}
        />
      )}

      {showPayment && (
        <PaymentModal
          netTotal={netTotal}
          initialMethod={paymentMethod}
          splitMode={paymentSplit}
          onConfirm={handleConfirmPayment}
          onCancel={() => { setShowPayment(false); focusBarcode(); }}
          loading={loading}
        />
      )}

      {saleResult && (
        <ReceiptPreview sale={saleResult} onNewSale={handleNewSale} />
      )}

      {showShiftClose && (
        <ShiftModal
          mode="close"
          user={sessionUser}
          shift={shift}
          onShiftOpened={() => {}}
          onShiftClosed={() => { setShowShiftClose(false); onShiftClose(true); }}
          onCancel={() => { setShowShiftClose(false); focusBarcode(); }}
        />
      )}

      {showHold && (
        <HoldModal
          cart={cart}
          lineCount={lineCount}
          netTotal={netTotal}
          onHeld={() => { setShowHold(false); clearCart(); focusBarcode(); }}
          onResume={(resumed) => { setShowHold(false); replaceCart(resumed); focusBarcode(); }}
          onClose={() => { setShowHold(false); focusBarcode(); }}
        />
      )}

      {showRefund && (
        <RefundModal
          cashierUserId={sessionUser.user_id}
          onClose={() => { setShowRefund(false); focusBarcode(); }}
        />
      )}

      {showReport && (
        <TodayReportModal onClose={() => { setShowReport(false); focusBarcode(); }} />
      )}

      {showBackOffice && (
        <BackOfficeModal
          sessionUser={sessionUser}
          onClose={() => { setShowBackOffice(false); focusBarcode(); }}
        />
      )}

      {showXReport && (
        <XReportModal
          shiftId={shift.shift_id}
          actorUserId={sessionUser.user_id}
          onClose={() => { setShowXReport(false); focusBarcode(); }}
        />
      )}

      {/* ── Restock alerts toast ── */}
      {restockAlerts.length > 0 && (
        <div
          className="restock-toast-overlay"
          onClick={() => { if (restockTimerRef.current) clearTimeout(restockTimerRef.current); setRestockAlerts([]); }}
        >
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
    </div>
  );
}
